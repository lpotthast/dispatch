use crudkit_core::condition::{Condition, ConditionElement};
use rootcause::{Result, prelude::*};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    backend::{
        automation_admission, automation_triggers, items, label_conditions, projects,
        storage::Store,
    },
    shared::view_models::{
        AutomationEffect, AutomationTriggerView, RoutingExplainRequest, RoutingExplanationView,
        RoutingRuleExplanationView, SelectorClauseResultView, WorkItemSummaryView,
    },
};

const PRIORITY_SCORE_SECONDS: i64 = 300;
const EVALUATION_COUNT_SCORE_SECONDS: i64 = 300;
const NEVER_RUN_SCORE_SECONDS: i64 = 24 * 60 * 60;

pub(crate) async fn explain(
    store: &Store,
    project_name: &str,
    request: RoutingExplainRequest,
) -> Result<RoutingExplanationView> {
    if request.item_id.is_none() && request.rule.is_none() {
        bail!("routing explanation requires an item id or unsaved rule");
    }
    if let Some(rule) = request.rule {
        let selector = rule
            .selector
            .as_ref()
            .ok_or_else(|| report!("unsaved work-consuming rule requires a selector"))?;
        let condition = label_conditions::ValidatedLabelCondition::new(selector)?;
        let items = items::list_items(store, project_name, None).await?;
        let matching = items
            .iter()
            .filter(|item| condition.matches_automation_selector(&item.labels))
            .collect::<Vec<_>>();
        let examples = matching
            .iter()
            .take(10)
            .map(|item| WorkItemSummaryView {
                id: item.id,
                title: item.title.clone(),
                state: item.state.clone(),
                updated_at: item.updated_at.clone(),
            })
            .collect();
        let clause_results = matching
            .first()
            .map(|item| clause_results(selector, &item.labels))
            .transpose()?
            .unwrap_or_default();
        return Ok(RoutingExplanationView {
            item_id: None,
            rules: vec![RoutingRuleExplanationView {
                trigger_id: None,
                trigger_name: rule.name,
                selector_matches: !matching.is_empty(),
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
            matching_item_count: Some(matching.len() as u64),
            example_items: examples,
        });
    }

    let item_id = request.item_id.expect("item id was checked");
    let item = items::get_item(store, project_name, item_id).await?;
    let triggers = automation_triggers::list_triggers(store, project_name)
        .await?
        .into_iter()
        .filter(|trigger| trigger.enabled && trigger.effect == AutomationEffect::ConsumeWork)
        .collect::<Vec<_>>();
    let max_evaluation_count = triggers
        .iter()
        .map(|trigger| trigger.evaluation_count)
        .max()
        .unwrap_or_default();
    let settings = projects::get_settings(store, project_name).await?;
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
        let blockers = match automation_admission::enforce_rule_start_allowed(
            store,
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
        rule.suppressed_by_exclusive = exclusive_winner.is_some() && eligible && !rule.exclusive;
        rule.would_win = rule.trigger_id == winner;
    }

    Ok(RoutingExplanationView {
        item_id: Some(item_id),
        rules,
        winner_trigger_id: winner,
        matching_item_count: None,
        example_items: Vec::new(),
    })
}

fn trigger_due(trigger: &AutomationTriggerView) -> bool {
    trigger
        .next_evaluation_at
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
        .is_none_or(|next| next <= OffsetDateTime::now_utc())
}

fn fairness_score(
    automation: &AutomationTriggerView,
    max_evaluation_count: i64,
    now: OffsetDateTime,
) -> i64 {
    let age_seconds = automation
        .last_evaluated_at
        .as_deref()
        .and_then(|value| OffsetDateTime::parse(value, &Rfc3339).ok())
        .map(|last| (now - last).whole_seconds().max(0))
        .unwrap_or(NEVER_RUN_SCORE_SECONDS);
    age_seconds
        .saturating_add(
            max_evaluation_count
                .saturating_sub(automation.evaluation_count)
                .saturating_mul(EVALUATION_COUNT_SCORE_SECONDS),
        )
        .saturating_add(automation.priority.saturating_mul(PRIORITY_SCORE_SECONDS))
}

fn clause_results(
    condition: &Condition,
    labels: &[crate::shared::view_models::WorkItemLabelView],
) -> Result<Vec<SelectorClauseResultView>> {
    let mut results = Vec::new();
    collect_clause_results(condition, labels, "$", &mut results)?;
    Ok(results)
}

fn collect_clause_results(
    condition: &Condition,
    labels: &[crate::shared::view_models::WorkItemLabelView],
    path: &str,
    results: &mut Vec<SelectorClauseResultView>,
) -> Result<()> {
    let elements = match condition {
        Condition::All(elements) | Condition::Any(elements) => elements,
    };
    for (index, element) in elements.iter().enumerate() {
        let element_path = format!("{path}[{index}]");
        match element {
            ConditionElement::Clause(clause) => {
                let condition = Condition::All(vec![ConditionElement::Clause(clause.clone())]);
                let matched =
                    label_conditions::ValidatedLabelCondition::new(&condition)?.matches(labels);
                results.push(SelectorClauseResultView {
                    path: element_path,
                    column_name: Some(clause.column_name.clone()),
                    matched,
                    detail: format!(
                        "{} {:?} {:?}",
                        clause.column_name, clause.operator, clause.value
                    ),
                });
            }
            ConditionElement::Condition(nested) => {
                collect_clause_results(nested, labels, &element_path, results)?;
            }
        }
    }
    Ok(())
}
