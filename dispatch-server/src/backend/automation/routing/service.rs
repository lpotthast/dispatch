use super::policy::clause_results;
use super::repository::RoutingRepository;
use crate::backend::automation::scheduling::policy::{fairness_score, trigger_due};
use crate::backend::{
    automation::rules::repository::RuleRepository, items::labels::conditions as label_conditions,
    items::repository::ItemRepository, projects::repository::ProjectRepository,
    runs::admission::service::RunAdmissionService, storage::TransactionManager,
};
use dispatch_types::{
    AutomationEffect, RoutingExplainRequest, RoutingExplanationView, RoutingRuleExplanationView,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
use time::OffsetDateTime;
pub(crate) struct RoutingService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    rules: Arc<RuleRepository>,
    items: Arc<ItemRepository>,
    repository: Arc<RoutingRepository>,
    admission: Arc<RunAdmissionService>,
}
impl RoutingService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        rules: Arc<RuleRepository>,
        items: Arc<ItemRepository>,
        repository: Arc<RoutingRepository>,
        admission: Arc<RunAdmissionService>,
    ) -> Self {
        Self {
            transactions,
            projects,
            rules,
            items,
            repository,
            admission,
        }
    }
    pub(crate) async fn explain(
        &self,
        project_name: &str,
        request: RoutingExplainRequest,
    ) -> Result<RoutingExplanationView> {
        if request.item_id.is_none() && request.rule.is_none() {
            bail!("routing explanation requires an item id or unsaved rule");
        }
        let transaction = self.transactions.begin().await?;
        let settings = self
            .projects
            .settings_in(&transaction, project_name)
            .await?;
        let project_id = settings.project_id;
        if let Some(rule) = request.rule {
            let selector = rule
                .selector
                .as_ref()
                .ok_or_else(|| report!("unsaved work-consuming rule requires a selector"))?;
            let condition = label_conditions::ValidatedLabelCondition::new(selector)?;
            let preview = self
                .repository
                .preview_in(&transaction, project_id, &condition)
                .await?;
            let clause_results = if preview.example_items.is_empty() {
                Vec::new()
            } else {
                clause_results(selector, &preview.first_example_labels)?
            };
            transaction.commit().await?;
            return Ok(RoutingExplanationView {
                item_id: None,
                rules: vec![RoutingRuleExplanationView {
                    trigger_id: None,
                    trigger_name: rule.name,
                    selector_matches: preview.matching_item_count != 0,
                    clause_results,
                    due: true,
                    admission_allowed: true,
                    fairness_score: 0,
                    priority: rule.priority,
                    exclusive: rule.exclusive,
                    suppressed_by_exclusive: false,
                    blockers: Vec::new(),
                    would_win: false,
                }],
                winner_trigger_id: None,
                matching_item_count: Some(preview.matching_item_count),
                example_items: preview.example_items,
            });
        }

        let item_id = request.item_id.expect("item id was checked");
        let item = self.items.get_in(&transaction, project_id, item_id).await?;
        let triggers = self
            .rules
            .list_in(&transaction, project_id)
            .await?
            .into_iter()
            .filter(|trigger| trigger.enabled && trigger.effect == AutomationEffect::ConsumeWork)
            .collect::<Vec<_>>();
        let max_evaluation_count = triggers
            .iter()
            .map(|trigger| trigger.evaluation_count)
            .max()
            .unwrap_or_default();
        let now = OffsetDateTime::now_utc();
        let mut rules = Vec::with_capacity(triggers.len());
        for trigger in triggers {
            let due = trigger_due(&trigger);
            let selector_matches = trigger
                .work_item_selector
                .as_ref()
                .map(label_conditions::ValidatedLabelCondition::new)
                .transpose()?
                .is_some_and(|selector| selector.matches_automation_selector(&item.labels));
            let clause_results = trigger
                .work_item_selector
                .as_ref()
                .map(|selector| clause_results(selector, &item.labels))
                .transpose()?
                .unwrap_or_default();
            let blockers = match self
                .admission
                .enforce_in(
                    &transaction,
                    project_name,
                    &settings,
                    trigger.mutability,
                    Some(trigger.id),
                    &trigger.execution,
                )
                .await
            {
                Ok(()) => Vec::new(),
                Err(err) => vec![err.to_string()],
            };
            let fairness_score = fairness_score(&trigger, max_evaluation_count, now);
            let priority = trigger.priority;
            rules.push(RoutingRuleExplanationView {
                trigger_id: Some(trigger.id),
                trigger_name: trigger.name,
                selector_matches,
                clause_results,
                due,
                admission_allowed: blockers.is_empty(),
                fairness_score,
                priority,
                exclusive: trigger.exclusive,
                suppressed_by_exclusive: false,
                blockers,
                would_win: false,
            });
        }
        let eligible = |rule: &&RoutingRuleExplanationView| {
            rule.selector_matches && rule.due && rule.admission_allowed
        };
        let exclusive_winner = rules
            .iter()
            .filter(eligible)
            .filter(|rule| rule.exclusive)
            .max_by(|left, right| {
                left.priority
                    .cmp(&right.priority)
                    .then_with(|| left.fairness_score.cmp(&right.fairness_score))
                    .then_with(|| right.trigger_id.cmp(&left.trigger_id))
            })
            .and_then(|rule| rule.trigger_id);
        let winner = exclusive_winner.or_else(|| {
            rules
                .iter()
                .filter(eligible)
                .max_by(|left, right| {
                    left.fairness_score
                        .cmp(&right.fairness_score)
                        .then_with(|| right.trigger_id.cmp(&left.trigger_id))
                })
                .and_then(|rule| rule.trigger_id)
        });
        for rule in &mut rules {
            let eligible = rule.selector_matches && rule.due && rule.admission_allowed;
            rule.suppressed_by_exclusive =
                exclusive_winner.is_some() && eligible && !rule.exclusive;
            rule.would_win = rule.trigger_id == winner;
        }

        transaction.commit().await?;
        Ok(RoutingExplanationView {
            item_id: Some(item_id),
            rules,
            winner_trigger_id: winner,
            matching_item_count: None,
            example_items: Vec::new(),
        })
    }
}
