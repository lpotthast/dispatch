use super::{
    model::{RuleSelection, ScheduleChange, WorkItemAutomationCandidate},
    repository::ScheduleRepository,
};
use crate::backend::{
    automation::{
        launch::{model::StartAutomation, service::LaunchService},
        production::service::ProductionService,
        rules::configuration::next_evaluation_at,
        scheduling::policy::{fairness_score, trigger_due},
    },
    events::UiEventBus,
    items::labels::conditions::ValidatedLabelCondition,
    items::{claims::service::ClaimService, repository::ItemRepository},
    projects::repository::ProjectRepository,
    runs::{
        admission::service::RunAdmissionService, launch::model::AgentLaunchTargetV1,
        model::AutomationTriggerOrigin,
    },
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{
    AutomationActivation, AutomationEffect, AutomationEvaluationOutcome, AutomationTriggerView,
    TriggerRunOutcome,
};
use rootcause::Result;
use std::{collections::HashMap, sync::Arc};
use time::OffsetDateTime;
use tokio::sync::{Mutex, watch};
pub(crate) struct SchedulerService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<ScheduleRepository>,
    items: Arc<ItemRepository>,
    claims: Arc<ClaimService>,
    production: Arc<ProductionService>,
    admission: Arc<RunAdmissionService>,
    launch: Arc<LaunchService>,
    events: UiEventBus,
    coordination: Mutex<()>,
}
impl SchedulerService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<ScheduleRepository>,
        items: Arc<ItemRepository>,
        claims: Arc<ClaimService>,
        production: Arc<ProductionService>,
        admission: Arc<RunAdmissionService>,
        launch: Arc<LaunchService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            items,
            claims,
            production,
            admission,
            launch,
            events,
            coordination: Mutex::new(()),
        }
    }
    pub(crate) async fn run_due(
        &self,
        active_project_ids: Option<&[i64]>,
        cancellations: Option<&HashMap<i64, watch::Receiver<bool>>>,
    ) -> Result<Vec<TriggerRunOutcome>> {
        let _permit = self.coordination.lock().await;
        let cancellation_for = |id| cancellations.and_then(|map| map.get(&id)).cloned();
        let mut outcomes = Vec::new();
        let tx = self.transactions.begin().await?;
        let queued = self
            .repository
            .list_in(&tx, RuleSelection::Queued, active_project_ids)
            .await?;
        tx.commit().await?;
        for queued_rule in queued {
            let tx = self.transactions.begin().await?;
            let rule = self
                .repository
                .get_in(&tx, queued_rule.project_id, queued_rule.id)
                .await?;
            if rule.pending_evaluation_count == 0 {
                continue;
            }
            let project = self.projects.name_in(&tx, rule.project_id).await?;
            if rule.effect == AutomationEffect::ConsumeWork {
                let settings = self.projects.settings_in(&tx, &project).await?;
                if self
                    .admission
                    .enforce_in(
                        &tx,
                        &project,
                        &settings,
                        rule.mutability,
                        Some(rule.id),
                        &rule.execution,
                    )
                    .await
                    .is_err()
                {
                    continue;
                }
            }
            self.repository.consume_in(&tx, &rule).await?;
            tx.commit().await?;
            let cancellation = cancellation_for(rule.project_id);
            if let Some(outcome) = self.evaluate(&project, rule, None, cancellation).await {
                outcomes.push(outcome);
            }
        }
        let tx = self.transactions.begin().await?;
        let enabled = self
            .repository
            .list_in(&tx, RuleSelection::Enabled, active_project_ids)
            .await?;
        let mut scopes = Vec::with_capacity(enabled.len());
        for rule in enabled {
            let project = self.projects.name_in(&tx, rule.project_id).await?;
            scopes.push((project, rule));
        }
        tx.commit().await?;
        for (project, rule) in scopes {
            match rule.activation {
                AutomationActivation::Manual | AutomationActivation::WorkItem => {}
                AutomationActivation::Cron => {
                    if trigger_due(&rule) {
                        let cancellation = cancellation_for(rule.project_id);
                        if let Some(outcome) =
                            self.evaluate(&project, rule, None, cancellation).await
                        {
                            outcomes.push(outcome);
                        }
                    }
                }
                AutomationActivation::WorkItemCreated => {
                    let tx = self.transactions.begin().await?;
                    self.projects.name_in(&tx, rule.project_id).await?;
                    let events = self
                        .repository
                        .created_events_in(&tx, rule.project_id, rule.last_event_id)
                        .await?;
                    tx.commit().await?;
                    let mut last = rule.last_event_id;
                    for event in events {
                        last = Some(event.id);
                        if let Some(outcome) = self
                            .evaluate(
                                &project,
                                rule.clone(),
                                event.work_item_id,
                                cancellation_for(rule.project_id),
                            )
                            .await
                        {
                            outcomes.push(outcome);
                        }
                    }
                    if last != rule.last_event_id {
                        self.advance(&rule, ScheduleChange::Cursor(last)).await?;
                    }
                }
            }
        }
        if let Some(project_ids) = active_project_ids {
            for id in project_ids {
                if let Some(project) = self.projects.find_name(*id).await?
                    && let Some(outcome) = self.run_next(&project, cancellation_for(*id)).await?
                {
                    outcomes.push(outcome);
                }
            }
        }
        Ok(outcomes)
    }
    pub(crate) async fn run_next(
        &self,
        project: &str,
        cancellation: Option<watch::Receiver<bool>>,
    ) -> Result<Option<TriggerRunOutcome>> {
        let tx = self.transactions.begin().await?;
        let settings = self.projects.settings_in(&tx, project).await?;
        let rules = self
            .repository
            .list_in(&tx, RuleSelection::WorkItems(settings.project_id), None)
            .await?;
        let mut items = self.items.list_in(&tx, settings.project_id, None).await?;
        items.reverse();
        let mut candidates = Vec::new();
        let mut checked = Vec::new();
        for rule in rules {
            if !trigger_due(&rule) {
                continue;
            }
            if self
                .admission
                .enforce_in(
                    &tx,
                    project,
                    &settings,
                    rule.mutability,
                    Some(rule.id),
                    &rule.execution,
                )
                .await
                .is_err()
            {
                continue;
            }
            let ids = match rule.work_item_selector.as_ref() {
                Some(selector) => {
                    let selector = ValidatedLabelCondition::new(selector)?;
                    items
                        .iter()
                        .filter(|item| {
                            item.claimed_by.is_none()
                                && item.finished_at.is_none()
                                && selector.matches_automation_selector(&item.labels)
                        })
                        .map(|item| item.id)
                        .collect::<Vec<_>>()
                }
                None => Vec::new(),
            };
            if ids.is_empty() {
                checked.push(rule);
            } else {
                candidates.push(WorkItemAutomationCandidate {
                    view: rule,
                    item_ids: ids,
                });
            }
        }
        if candidates.is_empty() {
            for rule in &checked {
                self.advance_in(&tx, rule, ScheduleChange::Checked).await?;
            }
            tx.commit().await?;
            for _ in checked {
                self.events.publish_automation_changed(project);
            }
            return Ok(None);
        }
        tx.commit().await?;
        let max = candidates
            .iter()
            .map(|candidate| candidate.view.evaluation_count)
            .max()
            .unwrap_or_default();
        let now = OffsetDateTime::now_utc();
        candidates.sort_by(|left, right| {
            fairness_score(&right.view, max, now)
                .cmp(&fairness_score(&left.view, max, now))
                .then_with(|| left.view.evaluation_count.cmp(&right.view.evaluation_count))
                .then_with(|| left.view.id.cmp(&right.view.id))
        });
        self.run_candidates(project, candidates, cancellation).await
    }
    pub(crate) async fn run_candidates(
        &self,
        project: &str,
        candidates: Vec<WorkItemAutomationCandidate>,
        cancellation: Option<watch::Receiver<bool>>,
    ) -> Result<Option<TriggerRunOutcome>> {
        let max = candidates
            .iter()
            .map(|candidate| candidate.view.evaluation_count)
            .max()
            .unwrap_or_default();
        let now = OffsetDateTime::now_utc();
        let routing = candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.view.id,
                    candidate.view.priority,
                    candidate.view.exclusive,
                    fairness_score(&candidate.view, max, now),
                    candidate.item_ids.clone(),
                )
            })
            .collect::<Vec<_>>();
        for candidate in candidates {
            for item_id in candidate.item_ids {
                let winner = routing
                    .iter()
                    .filter(|(_, _, exclusive, _, ids)| *exclusive && ids.contains(&item_id))
                    .max_by(|left, right| {
                        left.1
                            .cmp(&right.1)
                            .then_with(|| left.3.cmp(&right.3))
                            .then_with(|| right.0.cmp(&left.0))
                    })
                    .map(|entry| entry.0);
                if winner.is_some_and(|winner| winner != candidate.view.id)
                    || (winner.is_none() && candidate.view.exclusive)
                {
                    continue;
                }
                if let Some(outcome) = self
                    .evaluate(
                        project,
                        candidate.view.clone(),
                        Some(item_id),
                        cancellation.clone(),
                    )
                    .await
                {
                    return Ok(Some(outcome));
                }
            }
        }
        Ok(None)
    }
    async fn evaluate(
        &self,
        project: &str,
        rule: AutomationTriggerView,
        item_id: Option<i64>,
        cancellation: Option<watch::Receiver<bool>>,
    ) -> Option<TriggerRunOutcome> {
        if rule.effect == AutomationEffect::ProduceWork {
            let result = self.produce(project, &rule).await;
            let (item_id, item, error) = match result {
                Ok(item) => (Some(item.id), Some(item), None),
                Err(error) => {
                    let error = error.to_string();
                    if let Err(record_error) = self
                        .record(
                            &rule,
                            AutomationEvaluationOutcome::Failed,
                            None,
                            None,
                            Some(error.clone()),
                        )
                        .await
                    {
                        tracing::error!(error=%record_error,"failed to record producing automation failure");
                    }
                    (None, None, Some(error))
                }
            };
            return Some(TriggerRunOutcome {
                trigger_id: rule.id,
                trigger_name: rule.name,
                work_item_id: item_id,
                work_item: item,
                run: None,
                error,
            });
        }
        let available = match rule.work_item_selector.as_ref() {
            Some(selector) => match item_id {
                Some(id) => {
                    self.claims
                        .has_claimable_specific_item_matching_condition(project, id, selector)
                        .await
                }
                None => {
                    self.claims
                        .has_claimable_item_matching_condition(project, selector)
                        .await
                }
            },
            None => Ok(false),
        };
        match available {
            Ok(true) => Some(self.run_rule(project, rule, item_id, cancellation).await),
            result => {
                let update = self.advance(&rule, ScheduleChange::Checked).await;
                let error = result.err().or_else(|| update.err());
                error.map(|error| TriggerRunOutcome {
                    trigger_id: rule.id,
                    trigger_name: rule.name,
                    work_item_id: item_id,
                    work_item: None,
                    run: None,
                    error: Some(error.to_string()),
                })
            }
        }
    }
    async fn run_rule(
        &self,
        project: &str,
        rule: AutomationTriggerView,
        item_id: Option<i64>,
        cancellation: Option<watch::Receiver<bool>>,
    ) -> TriggerRunOutcome {
        let result: Result<_> = async {
            let target = if let Some(id) = item_id {
                let tx = self.transactions.begin().await?;
                let project_id = self.projects.id_in(&tx, project).await?;
                let item = self.items.get_in(&tx, project_id, id).await?;
                tx.commit().await?;
                AgentLaunchTargetV1::specific(item.id, item.version)?
            } else if let Some(selector) = rule.work_item_selector.as_ref() {
                AgentLaunchTargetV1::selector(selector)?
            } else {
                AgentLaunchTargetV1::none()
            };
            self.launch
                .start_until(
                    project,
                    StartAutomation {
                        tool: Some(rule.tool_name),
                        launch_target: target,
                        work_item_selector: rule.work_item_selector.clone(),
                        extra_prompt: Some(rule.prompt.clone()),
                        mutability: Some(rule.mutability),
                        personality_id: rule.personality_id,
                        trigger: Some(AutomationTriggerOrigin {
                            trigger_id: rule.id,
                            trigger_name: rule.name.clone(),
                            trigger_revision_id: rule.current_revision_id,
                        }),
                        execution: rule.execution.clone(),
                        postconditions: rule.postconditions.clone(),
                    },
                    cancellation,
                )
                .await
        }
        .await;
        let (run, mut error) = match result {
            Ok(run) => (Some(run), None),
            Err(error) => (None, Some(error.to_string())),
        };
        let item_id = run.as_ref().and_then(|run| run.work_item_id).or(item_id);
        if let Err(record_error) = self
            .record(
                &rule,
                if run.is_some() {
                    AutomationEvaluationOutcome::StartedRun
                } else {
                    AutomationEvaluationOutcome::Failed
                },
                item_id,
                run.as_ref().map(|run| run.id),
                error.clone(),
            )
            .await
        {
            error = Some(record_error.to_string());
        }
        TriggerRunOutcome {
            trigger_id: rule.id,
            trigger_name: rule.name,
            work_item_id: item_id,
            work_item: None,
            run,
            error,
        }
    }
    async fn produce(
        &self,
        project: &str,
        rule: &AutomationTriggerView,
    ) -> Result<dispatch_types::WorkItemView> {
        let _permit = self.production.acquire().await;
        let tx = self.transactions.begin().await?;
        let (item, created) = self.production.produce_in(&tx, project, rule).await?;
        let current = self
            .repository
            .get_in(&tx, rule.project_id, rule.id)
            .await?;
        self.advance_in(&tx, &current, ScheduleChange::Evaluated)
            .await?;
        tx.commit().await?;
        if created {
            self.events.publish_work_item_changed(project, item.id);
        }
        self.events.publish_automation_changed(project);
        Ok(item)
    }
    async fn advance_in(
        &self,
        tx: &Transaction,
        rule: &AutomationTriggerView,
        change: ScheduleChange,
    ) -> Result<()> {
        let next = match rule.activation {
            AutomationActivation::WorkItem | AutomationActivation::Cron => {
                Some(next_evaluation_at(&rule.schedule)?)
            }
            _ => rule.next_evaluation_at.clone(),
        };
        self.repository.advance_in(tx, rule, change, next).await
    }
    async fn advance(&self, rule: &AutomationTriggerView, change: ScheduleChange) -> Result<()> {
        let tx = self.transactions.begin().await?;
        let project = self.projects.name_in(&tx, rule.project_id).await?;
        let current = self
            .repository
            .get_in(&tx, rule.project_id, rule.id)
            .await?;
        self.advance_in(&tx, &current, change).await?;
        tx.commit().await?;
        self.events.publish_automation_changed(&project);
        Ok(())
    }
    async fn record(
        &self,
        rule: &AutomationTriggerView,
        outcome: AutomationEvaluationOutcome,
        item_id: Option<i64>,
        run_id: Option<i64>,
        error: Option<String>,
    ) -> Result<()> {
        let tx = self.transactions.begin().await?;
        let project = self.projects.name_in(&tx, rule.project_id).await?;
        let current = self
            .repository
            .get_in(&tx, rule.project_id, rule.id)
            .await?;
        self.repository
            .record_in(&tx, rule, outcome, item_id, run_id, error)
            .await?;
        self.advance_in(&tx, &current, ScheduleChange::Evaluated)
            .await?;
        tx.commit().await?;
        self.events.publish_automation_changed(&project);
        Ok(())
    }
}
