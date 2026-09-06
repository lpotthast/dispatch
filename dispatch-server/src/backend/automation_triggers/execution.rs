use std::{collections::HashMap, time::Duration as StdDuration};

use crudkit_core::condition::Condition;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, TransactionTrait,
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::sync::watch;

use super::{model_to_view, next_evaluation_at, policy::validate_produced_work_spec};
use crate::{
    backend::{
        agent_run_launch::AgentLaunchTargetV1,
        automation::{self, AutomationTriggerOrigin, StartAutomation},
        automation_admission,
        automation_controller::AutomationController,
        codex_app_server::SharedCodexStatus,
        entities::{
            automation_evaluation::AutomationEvaluationActiveModel,
            automation_trigger::{
                self, AutomationTrigger, AutomationTriggerActiveModel, AutomationTriggerModel,
            },
            work_item::{self, WorkItem},
            work_item_event,
            work_item_origin::{self, WorkItemOrigin},
        },
        events, item_claims,
        items::{self, CreateWorkItem},
        label_conditions,
        process_sessions::ProcessSessionRegistry,
        projects,
        storage::{Store, utc_now},
        work_item_creation::{self, CreateWorkItemPlan, InsertWorkItemOrigin},
        work_item_events,
    },
    shared::view_models::{
        AutomationActivation, AutomationEffect, AutomationEvaluationOutcome, AutomationTriggerView,
        ProduceDeduplication, ProducedWorkSpec, TriggerRunOutcome, WorkItemOriginKind,
    },
};

const SCHEDULER_TICK_SECONDS: u64 = 1;
const MAINTENANCE_TICK_SECONDS: u64 = 15;
pub(super) const PRIORITY_SCORE_SECONDS: i64 = 300;
const EVALUATION_COUNT_SCORE_SECONDS: i64 = 300;
const NEVER_RUN_SCORE_SECONDS: i64 = 24 * 60 * 60;

#[cfg(test)]
pub(super) async fn run_due_triggers(store: &Store) -> Result<Vec<TriggerRunOutcome>> {
    run_due_triggers_with_sessions(store, None).await
}

#[cfg(test)]
pub(super) async fn run_due_triggers_with_sessions(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
) -> Result<Vec<TriggerRunOutcome>> {
    run_due_triggers_with_sessions_for_projects(store, sessions, None, None, None).await
}

pub(super) async fn run_due_triggers_with_sessions_for_projects(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    active_project_ids: Option<&[i64]>,
    project_cancellations: Option<&HashMap<i64, watch::Receiver<bool>>>,
) -> Result<Vec<TriggerRunOutcome>> {
    let scope = AutomationProjectScope::new(active_project_ids, project_cancellations);
    let mut outcomes =
        run_queued_evaluations(store, sessions.clone(), codex_status.clone(), scope).await?;
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::Enabled.eq(true))
        .order_by_asc(automation_trigger::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load enabled automation triggers")?;

    for trigger in triggers {
        let view = model_to_view(trigger.clone())?;
        if matches!(
            view.activation,
            AutomationActivation::Manual | AutomationActivation::WorkItem
        ) {
            continue;
        }
        if !scope.includes_project(view.project_id) {
            continue;
        }
        let project_name = projects::project_name_by_id(store, view.project_id).await?;
        match view.activation {
            AutomationActivation::Manual => {}
            AutomationActivation::WorkItem => {}
            AutomationActivation::Cron => {
                if trigger_is_due(view.next_evaluation_at.as_deref())
                    && let Some(outcome) = evaluate_trigger_once(
                        store,
                        &project_name,
                        trigger,
                        None,
                        sessions.clone(),
                        codex_status.clone(),
                        scope.cancellation_for(view.project_id),
                    )
                    .await
                {
                    outcomes.push(outcome);
                }
            }
            AutomationActivation::WorkItemCreated => {
                let events =
                    new_item_created_events(store, view.project_id, view.last_event_id).await?;
                let mut last_event_id = view.last_event_id;
                for event in events {
                    last_event_id = Some(event.id);
                    if let Some(outcome) = evaluate_trigger_once(
                        store,
                        &project_name,
                        trigger.clone(),
                        event.work_item_id,
                        sessions.clone(),
                        codex_status.clone(),
                        scope.cancellation_for(view.project_id),
                    )
                    .await
                    {
                        outcomes.push(outcome);
                    }
                }
                if last_event_id != view.last_event_id {
                    update_trigger_event_cursor(store, trigger, last_event_id).await?;
                }
            }
        }
    }
    if let Some(active_project_ids) = scope.active_project_ids() {
        for project_id in active_project_ids {
            let Some(project_name) = projects::find_project_name_by_id(store, *project_id).await?
            else {
                continue;
            };
            if let Some(outcome) = run_next_work_item_automation_for_project(
                store,
                &project_name,
                sessions.clone(),
                codex_status.clone(),
                scope.cancellation_for(*project_id),
            )
            .await?
            {
                outcomes.push(outcome);
            }
        }
    }
    Ok(outcomes)
}

#[derive(Clone, Copy)]
struct AutomationProjectScope<'a> {
    active_project_ids: Option<&'a [i64]>,
    project_cancellations: Option<&'a HashMap<i64, watch::Receiver<bool>>>,
}

impl<'a> AutomationProjectScope<'a> {
    fn new(
        active_project_ids: Option<&'a [i64]>,
        project_cancellations: Option<&'a HashMap<i64, watch::Receiver<bool>>>,
    ) -> Self {
        Self {
            active_project_ids,
            project_cancellations,
        }
    }

    fn includes_project(&self, project_id: i64) -> bool {
        match self.active_project_ids {
            Some(active_project_ids) => active_project_ids.contains(&project_id),
            None => true,
        }
    }

    fn active_project_ids(&self) -> Option<&'a [i64]> {
        self.active_project_ids
    }

    fn cancellation_for(&self, project_id: i64) -> Option<watch::Receiver<bool>> {
        self.project_cancellations
            .and_then(|cancellations| cancellations.get(&project_id))
            .cloned()
    }
}

pub async fn schedule_trigger_evaluation(
    store: &Store,
    project_name: &str,
    trigger_id: i64,
) -> Result<AutomationTriggerView> {
    let project_id = projects::project_id(store, project_name).await?;
    let trigger = AutomationTrigger::find_by_id(trigger_id)
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .one(store.db().as_ref())
        .await
        .context("failed to load automation trigger")?
        .ok_or_else(|| report!("trigger {trigger_id} does not exist in this project"))?;

    let now = utc_now();
    let pending_evaluation_count = trigger.pending_evaluation_count.saturating_add(1);
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.pending_evaluation_count = Set(pending_evaluation_count);
    active.last_evaluation_queued_at = Set(Some(now.clone()));
    active.updated_at = Set(now);
    let transaction = store
        .db()
        .begin()
        .await
        .context("failed to begin queued automation update")?;
    let updated = active
        .update(&transaction)
        .await
        .context("failed to queue automation evaluation")?;
    let view = model_to_view(updated)?;
    transaction
        .commit()
        .await
        .context("failed to commit queued automation update")?;
    events::publish_automation_changed(project_name);
    Ok(view)
}

async fn run_queued_evaluations(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    scope: AutomationProjectScope<'_>,
) -> Result<Vec<TriggerRunOutcome>> {
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::PendingEvaluationCount.gt(0))
        .order_by_desc(automation_trigger::Column::Priority)
        .order_by_asc(automation_trigger::Column::LastEvaluationQueuedAt)
        .order_by_asc(automation_trigger::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load queued automation evaluations")?;

    let mut outcomes = Vec::new();
    for trigger in triggers {
        if !scope.includes_project(trigger.project_id) {
            continue;
        }
        let project_name = projects::project_name_by_id(store, trigger.project_id).await?;
        let view = model_to_view(trigger.clone())?;
        if view.effect == AutomationEffect::ConsumeWork {
            let settings = projects::get_settings(store, &project_name).await?;
            if automation_admission::enforce_rule_start_allowed(
                store,
                &project_name,
                &settings,
                view.mutability,
                Some(view.id),
                &view.execution,
            )
            .await
            .is_err()
            {
                continue;
            }
        }
        let trigger = consume_queued_evaluation(store, trigger).await?;
        let project_id = trigger.project_id;
        if let Some(outcome) = evaluate_trigger_once(
            store,
            &project_name,
            trigger,
            None,
            sessions.clone(),
            codex_status.clone(),
            scope.cancellation_for(project_id),
        )
        .await
        {
            outcomes.push(outcome);
        }
    }
    Ok(outcomes)
}

async fn consume_queued_evaluation(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    let pending_evaluation_count = trigger.pending_evaluation_count.saturating_sub(1);
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.pending_evaluation_count = Set(pending_evaluation_count);
    active.updated_at = Set(utc_now());
    let transaction = store
        .db()
        .begin()
        .await
        .context("failed to begin queued automation consumption")?;
    let updated = active
        .update(&transaction)
        .await
        .context("failed to consume queued automation evaluation")?;
    transaction
        .commit()
        .await
        .context("failed to commit queued automation consumption")?;
    Ok(updated)
}

pub(super) async fn run_next_work_item_automation_for_project(
    store: &Store,
    project_name: &str,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> Result<Option<TriggerRunOutcome>> {
    let project_id = projects::project_id(store, project_name).await?;
    let triggers = AutomationTrigger::find()
        .filter(automation_trigger::Column::ProjectId.eq(project_id))
        .filter(automation_trigger::Column::Enabled.eq(true))
        .filter(
            automation_trigger::Column::Activation.eq(AutomationActivation::WorkItem.as_storage()),
        )
        .filter(automation_trigger::Column::Effect.eq(AutomationEffect::ConsumeWork.as_storage()))
        .order_by_asc(automation_trigger::Column::Id)
        .all(store.db().as_ref())
        .await
        .context("failed to load work-item automation entries")?;

    let mut candidates = Vec::new();
    let mut checked_without_match = Vec::new();
    for trigger in triggers {
        let view = model_to_view(trigger.clone())?;
        if !trigger_is_due(view.next_evaluation_at.as_deref()) {
            continue;
        }
        let settings = projects::get_settings(store, project_name).await?;
        if automation_admission::enforce_rule_start_allowed(
            store,
            project_name,
            &settings,
            view.mutability,
            Some(view.id),
            &view.execution,
        )
        .await
        .is_err()
        {
            continue;
        }
        let Some(selector) = view.work_item_selector.as_ref() else {
            checked_without_match.push(trigger);
            continue;
        };
        let item_ids = matching_claimable_item_ids(store, project_name, selector).await?;
        if !item_ids.is_empty() {
            candidates.push(WorkItemAutomationCandidate {
                trigger,
                view,
                item_ids,
            });
        } else {
            checked_without_match.push(trigger);
        }
    }

    let Some(max_evaluation_count) = candidates
        .iter()
        .map(|candidate| candidate.view.evaluation_count)
        .max()
    else {
        for trigger in checked_without_match {
            update_trigger_after_check(store, trigger).await?;
        }
        return Ok(None);
    };
    let now = OffsetDateTime::now_utc();
    candidates.sort_by(|left, right| {
        work_item_automation_score(&right.view, max_evaluation_count, now)
            .cmp(&work_item_automation_score(
                &left.view,
                max_evaluation_count,
                now,
            ))
            .then_with(|| left.view.evaluation_count.cmp(&right.view.evaluation_count))
            .then_with(|| left.view.id.cmp(&right.view.id))
    });

    run_first_available_work_item_automation_candidate(
        store,
        project_name,
        candidates,
        sessions,
        codex_status,
        cancellation,
    )
    .await
}

pub(super) struct WorkItemAutomationCandidate {
    pub(super) trigger: AutomationTriggerModel,
    pub(super) view: AutomationTriggerView,
    pub(super) item_ids: Vec<i64>,
}

pub(super) async fn run_first_available_work_item_automation_candidate(
    store: &Store,
    project_name: &str,
    candidates: Vec<WorkItemAutomationCandidate>,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> Result<Option<TriggerRunOutcome>> {
    let max_evaluation_count = candidates
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
                work_item_automation_score(&candidate.view, max_evaluation_count, now),
                candidate.item_ids.clone(),
            )
        })
        .collect::<Vec<_>>();
    for candidate in candidates {
        for item_id in &candidate.item_ids {
            let exclusive_winner = routing
                .iter()
                .filter(|(_, _, exclusive, _, item_ids)| *exclusive && item_ids.contains(item_id))
                .max_by(|left, right| {
                    left.1
                        .cmp(&right.1)
                        .then_with(|| left.3.cmp(&right.3))
                        .then_with(|| right.0.cmp(&left.0))
                })
                .map(|entry| entry.0);
            if let Some(exclusive_winner) = exclusive_winner
                && exclusive_winner != candidate.view.id
            {
                continue;
            }
            if exclusive_winner.is_none() && candidate.view.exclusive {
                continue;
            }
            if let Some(outcome) = evaluate_trigger_once(
                store,
                project_name,
                candidate.trigger.clone(),
                Some(*item_id),
                sessions.clone(),
                codex_status.clone(),
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

async fn matching_claimable_item_ids(
    store: &Store,
    project_name: &str,
    selector: &Condition,
) -> Result<Vec<i64>> {
    let selector = label_conditions::ValidatedLabelCondition::new(selector)?;
    let mut items = items::list_items(store, project_name, None).await?;
    items.reverse();
    Ok(items
        .into_iter()
        .filter(|item| {
            item.claimed_by.is_none()
                && item.finished_at.is_none()
                && selector.matches_automation_selector(&item.labels)
        })
        .map(|item| item.id)
        .collect())
}

pub(super) fn work_item_automation_score(
    automation: &AutomationTriggerView,
    max_evaluation_count: i64,
    now: OffsetDateTime,
) -> i64 {
    let age_seconds = automation
        .last_evaluated_at
        .as_deref()
        .and_then(|last_evaluated_at| OffsetDateTime::parse(last_evaluated_at, &Rfc3339).ok())
        .map(|last_evaluated_at| (now - last_evaluated_at).whole_seconds().max(0))
        .unwrap_or(NEVER_RUN_SCORE_SECONDS);
    let evaluation_count_gap = max_evaluation_count.saturating_sub(automation.evaluation_count);
    age_seconds
        .saturating_add(evaluation_count_gap.saturating_mul(EVALUATION_COUNT_SCORE_SECONDS))
        .saturating_add(automation.priority.saturating_mul(PRIORITY_SCORE_SECONDS))
}

pub fn spawn_scheduler_until(
    store: Store,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    controller: AutomationController,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    tokio::spawn(async move {
        let mut automation_interval =
            tokio::time::interval(StdDuration::from_secs(SCHEDULER_TICK_SECONDS));
        let mut maintenance_interval =
            tokio::time::interval(StdDuration::from_secs(MAINTENANCE_TICK_SECONDS));
        loop {
            tokio::select! {
                _ = automation_interval.tick() => {
                    let project_cancellations = controller.project_cancellations().await;
                    if !project_cancellations.is_empty() {
                        let active_project_ids = project_cancellations
                            .keys()
                            .copied()
                            .collect::<Vec<_>>();
                        if let Err(err) = run_due_triggers_with_sessions_for_projects(
                            &store,
                            sessions.clone(),
                            codex_status.clone(),
                            Some(&active_project_ids),
                            Some(&project_cancellations),
                        )
                        .await
                        {
                            tracing::error!(
                                error = %format_args!("{err:#}"),
                                "automation trigger scheduler failed"
                            );
                        }
                    }
                }
                _ = maintenance_interval.tick() => {
                    if let Err(err) = automation::recover_configured_stale_claims(&store).await {
                        tracing::error!(
                            error = %format_args!("{err:#}"),
                            "stale claim recovery failed"
                        );
                    }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    });
}

#[allow(clippy::too_many_arguments)]
async fn evaluate_trigger_once(
    store: &Store,
    project_name: &str,
    trigger: AutomationTriggerModel,
    work_item_id: Option<i64>,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> Option<TriggerRunOutcome> {
    let view = match model_to_view(trigger.clone()) {
        Ok(view) => view,
        Err(err) => {
            return Some(TriggerRunOutcome {
                trigger_id: trigger.id,
                trigger_name: trigger.name,
                work_item_id,
                work_item: None,
                run: None,
                error: Some(err.to_string()),
            });
        }
    };

    match view.effect {
        AutomationEffect::ProduceWork => {
            let result = create_work_item_from_trigger(store, project_name, &view).await;
            let _ = update_trigger_after_evaluation(store, trigger).await;
            let (work_item_id, work_item, error) = match result {
                Ok(work_item) => (Some(work_item.id), Some(work_item), None),
                Err(err) => {
                    let error = err.to_string();
                    let _ = insert_evaluation_in_conn(
                        store.db().as_ref(),
                        &view,
                        AutomationEvaluationOutcome::Failed,
                        None,
                        None,
                        Some(error.clone()),
                    )
                    .await;
                    (None, None, Some(error))
                }
            };
            Some(TriggerRunOutcome {
                trigger_id: view.id,
                trigger_name: view.name,
                work_item_id,
                work_item,
                run: None,
                error,
            })
        }
        AutomationEffect::ConsumeWork => {
            match trigger_has_consumable_work(store, project_name, &view, work_item_id).await {
                Ok(true) => Some(
                    run_trigger_once(
                        store,
                        project_name,
                        trigger,
                        work_item_id,
                        sessions,
                        codex_status,
                        cancellation,
                    )
                    .await,
                ),
                Ok(false) => {
                    let _ = update_trigger_after_check(store, trigger).await;
                    None
                }
                Err(err) => {
                    let _ = update_trigger_after_check(store, trigger).await;
                    Some(TriggerRunOutcome {
                        trigger_id: view.id,
                        trigger_name: view.name,
                        work_item_id,
                        work_item: None,
                        run: None,
                        error: Some(err.to_string()),
                    })
                }
            }
        }
    }
}

pub(super) async fn create_work_item_from_trigger(
    store: &Store,
    project_name: &str,
    automation: &AutomationTriggerView,
) -> Result<crate::shared::view_models::WorkItemView> {
    let spec = automation
        .produced_work
        .clone()
        .unwrap_or(ProducedWorkSpec {
            title: None,
            state: crate::shared::view_models::DEFAULT_STATE_LABEL.to_owned(),
            initial_labels: Vec::new(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            deduplication: ProduceDeduplication::Always,
        });
    validate_produced_work_spec(&spec)?;
    let create = CreateWorkItemPlan::new(CreateWorkItem {
        title: spec
            .title
            .clone()
            .unwrap_or_else(|| automation.name.clone()),
        description: automation.prompt.clone(),
        state: spec.state.clone(),
        agent_model_override: spec.agent_model_override.clone(),
        agent_reasoning_effort_override: spec.agent_reasoning_effort_override,
        initial_labels: spec.initial_labels.clone(),
    })?;
    let project_id = projects::project_id(store, project_name).await?;
    let settings = projects::get_settings_by_id(store, project_id).await?;
    items::validate_effective_agent_selection(
        &settings,
        create.agent_model_override(),
        create.agent_reasoning_effort_override(),
    )?;

    let _production_guard = store.lock_automation_production().await;
    let txn = store
        .db()
        .begin()
        .await
        .context("failed to start produced-work evaluation")?;
    if let Some(existing_item_id) =
        unfinished_duplicate_item_id(&txn, project_id, automation.id, &spec.deduplication).await?
    {
        insert_evaluation_in_conn(
            &txn,
            automation,
            AutomationEvaluationOutcome::SkippedDuplicate,
            Some(existing_item_id),
            None,
            None,
        )
        .await?;
        txn.commit()
            .await
            .context("failed to commit duplicate produced-work evaluation")?;
        return items::get_item(store, project_name, existing_item_id).await;
    }

    let evaluation = insert_evaluation_in_conn(
        &txn,
        automation,
        AutomationEvaluationOutcome::CreatedWork,
        None,
        None,
        None,
    )
    .await?;
    let now = utc_now();
    let item = work_item_creation::insert_planned_with_origin_in_tx(
        &txn,
        project_id,
        create,
        now,
        InsertWorkItemOrigin {
            kind: WorkItemOriginKind::ProducingAutomation,
            actor_id: None,
            agent_run_id: None,
            producing_evaluation_id: Some(evaluation.id),
            trigger_id: Some(automation.id),
            trigger_revision_id: automation.current_revision_id,
            trigger_name: Some(automation.name.clone()),
            bundle_key: automation.managed_bundle_key.clone(),
            deduplication_key: deduplication_key(&spec.deduplication),
        },
        work_item_events::EventAttribution::default(),
    )
    .await?;
    let mut evaluation_active: AutomationEvaluationActiveModel = evaluation.into();
    evaluation_active.work_item_id = Set(Some(item.id));
    evaluation_active
        .update(&txn)
        .await
        .context("failed to attach produced item to evaluation")?;
    txn.commit()
        .await
        .context("failed to commit produced-work evaluation")?;
    events::publish_work_item_changed(project_name, item.id);
    crate::backend::work_item_views::model_to_view(store, item).await
}

fn deduplication_key(policy: &ProduceDeduplication) -> Option<String> {
    match policy {
        ProduceDeduplication::WhileUnfinishedForKey { key } => Some(key.clone()),
        _ => None,
    }
}

async fn unfinished_duplicate_item_id<C>(
    conn: &C,
    project_id: i64,
    trigger_id: i64,
    policy: &ProduceDeduplication,
) -> Result<Option<i64>>
where
    C: ConnectionTrait,
{
    let mut query = WorkItemOrigin::find()
        .filter(work_item_origin::Column::ProjectId.eq(project_id))
        .filter(
            work_item_origin::Column::OriginKind
                .eq(WorkItemOriginKind::ProducingAutomation.as_storage()),
        )
        .order_by_desc(work_item_origin::Column::WorkItemId);
    query = match policy {
        ProduceDeduplication::Always => return Ok(None),
        ProduceDeduplication::WhileUnfinishedForTrigger => {
            query.filter(work_item_origin::Column::TriggerId.eq(trigger_id))
        }
        ProduceDeduplication::WhileUnfinishedForKey { key } => {
            query.filter(work_item_origin::Column::DeduplicationKey.eq(key))
        }
    };
    for origin in query
        .all(conn)
        .await
        .context("failed to inspect produced-work deduplication origins")?
    {
        if WorkItem::find_by_id(origin.work_item_id)
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::FinishedAt.is_null())
            .one(conn)
            .await
            .context("failed to inspect produced-work duplicate")?
            .is_some()
        {
            return Ok(Some(origin.work_item_id));
        }
    }
    Ok(None)
}

async fn insert_evaluation_in_conn<C>(
    conn: &C,
    automation: &AutomationTriggerView,
    outcome: AutomationEvaluationOutcome,
    work_item_id: Option<i64>,
    run_id: Option<i64>,
    error: Option<String>,
) -> Result<crate::backend::entities::automation_evaluation::Model>
where
    C: ConnectionTrait,
{
    let now = utc_now();
    Ok(AutomationEvaluationActiveModel {
        project_id: Set(automation.project_id),
        trigger_id: Set(Some(automation.id)),
        trigger_revision_id: Set(automation.current_revision_id),
        trigger_name: Set(automation.name.clone()),
        activation_cause: Set(automation.activation.as_storage().to_owned()),
        outcome: Set(outcome.as_storage().to_owned()),
        work_item_id: Set(work_item_id),
        run_id: Set(run_id),
        error: Set(error),
        created_at: Set(now.clone()),
        completed_at: Set(Some(now)),
        ..Default::default()
    }
    .insert(conn)
    .await
    .context("failed to record automation evaluation")?)
}

async fn trigger_has_consumable_work(
    store: &Store,
    project_name: &str,
    automation: &AutomationTriggerView,
    work_item_id: Option<i64>,
) -> Result<bool> {
    let Some(selector) = automation.work_item_selector.as_ref() else {
        return Ok(false);
    };
    if let Some(work_item_id) = work_item_id {
        return item_claims::has_claimable_specific_item_matching_condition(
            store,
            project_name,
            work_item_id,
            selector,
        )
        .await;
    };
    item_claims::has_claimable_item_matching_condition(store, project_name, selector).await
}

#[allow(clippy::too_many_arguments)]
async fn run_trigger_once(
    store: &Store,
    project_name: &str,
    trigger: AutomationTriggerModel,
    work_item_id: Option<i64>,
    sessions: Option<ProcessSessionRegistry>,
    codex_status: Option<SharedCodexStatus>,
    cancellation: Option<watch::Receiver<bool>>,
) -> TriggerRunOutcome {
    let view = match model_to_view(trigger.clone()) {
        Ok(view) => view,
        Err(err) => {
            return TriggerRunOutcome {
                trigger_id: trigger.id,
                trigger_name: trigger.name,
                work_item_id,
                work_item: None,
                run: None,
                error: Some(err.to_string()),
            };
        }
    };

    let launch_target = match work_item_id {
        Some(item_id) => match items::get_item(store, project_name, item_id).await {
            Ok(item) => match AgentLaunchTargetV1::specific(item.id, item.version) {
                Ok(target) => target,
                Err(err) => {
                    return TriggerRunOutcome {
                        trigger_id: trigger.id,
                        trigger_name: trigger.name,
                        work_item_id,
                        work_item: None,
                        run: None,
                        error: Some(err.to_string()),
                    };
                }
            },
            Err(err) => {
                return TriggerRunOutcome {
                    trigger_id: trigger.id,
                    trigger_name: trigger.name,
                    work_item_id,
                    work_item: None,
                    run: None,
                    error: Some(err.to_string()),
                };
            }
        },
        None => match view.work_item_selector.as_ref() {
            Some(selector) => match AgentLaunchTargetV1::selector(selector) {
                Ok(target) => target,
                Err(err) => {
                    return TriggerRunOutcome {
                        trigger_id: trigger.id,
                        trigger_name: trigger.name,
                        work_item_id,
                        work_item: None,
                        run: None,
                        error: Some(err.to_string()),
                    };
                }
            },
            None => AgentLaunchTargetV1::none(),
        },
    };

    let result = automation::start_automation_with_sessions_until(
        store,
        project_name,
        StartAutomation {
            tool: Some(view.tool_name),
            launch_target,
            work_item_selector: view.work_item_selector.clone(),
            extra_prompt: Some(view.prompt.clone()),
            mutability: Some(view.mutability),
            personality_id: view.personality_id,
            trigger: Some(AutomationTriggerOrigin {
                trigger_id: view.id,
                trigger_name: view.name.clone(),
                trigger_revision_id: view.current_revision_id,
            }),
            execution: view.execution.clone(),
            postconditions: view.postconditions.clone(),
        },
        sessions,
        codex_status,
        cancellation,
    )
    .await;

    let (run, error) = match result {
        Ok(run) => (Some(run), None),
        Err(err) => (None, Some(err.to_string())),
    };
    let _ = insert_evaluation_in_conn(
        store.db().as_ref(),
        &view,
        if run.is_some() {
            AutomationEvaluationOutcome::StartedRun
        } else {
            AutomationEvaluationOutcome::Failed
        },
        run.as_ref()
            .and_then(|run| run.work_item_id)
            .or(work_item_id),
        run.as_ref().map(|run| run.id),
        error.clone(),
    )
    .await;
    let _ = update_trigger_after_evaluation(store, trigger).await;
    let outcome_work_item_id = run
        .as_ref()
        .and_then(|run| run.work_item_id)
        .or(work_item_id);

    TriggerRunOutcome {
        trigger_id: view.id,
        trigger_name: view.name,
        work_item_id: outcome_work_item_id,
        work_item: None,
        run,
        error,
    }
}

async fn update_trigger_after_evaluation(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    let view = model_to_view(trigger.clone())?;
    let now = utc_now();
    let next = match view.activation {
        AutomationActivation::WorkItem | AutomationActivation::Cron => {
            Some(next_evaluation_at(&view.schedule)?)
        }
        AutomationActivation::Manual => view.next_evaluation_at,
        AutomationActivation::WorkItemCreated => view.next_evaluation_at,
    };
    let evaluation_count = trigger.evaluation_count.saturating_add(1);
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.last_evaluated_at = Set(Some(now.clone()));
    active.next_evaluation_at = Set(next);
    active.evaluation_count = Set(evaluation_count);
    active.updated_at = Set(now);
    let transaction = store
        .db()
        .begin()
        .await
        .context("failed to begin automation evaluation update")?;
    let updated = active
        .update(&transaction)
        .await
        .context("failed to update automation trigger after evaluation")?;
    transaction
        .commit()
        .await
        .context("failed to commit automation evaluation update")?;
    publish_project_id_event(store, updated.project_id).await;
    Ok(updated)
}

async fn update_trigger_after_check(
    store: &Store,
    trigger: AutomationTriggerModel,
) -> Result<AutomationTriggerModel> {
    let view = model_to_view(trigger.clone())?;
    let mut active: AutomationTriggerActiveModel = trigger.into();
    let next = match view.activation {
        AutomationActivation::WorkItem | AutomationActivation::Cron => {
            Some(next_evaluation_at(&view.schedule)?)
        }
        AutomationActivation::Manual | AutomationActivation::WorkItemCreated => {
            view.next_evaluation_at
        }
    };
    active.next_evaluation_at = Set(next);
    active.updated_at = Set(utc_now());
    let transaction = store
        .db()
        .begin()
        .await
        .context("failed to begin automation schedule update")?;
    let updated = active
        .update(&transaction)
        .await
        .context("failed to update automation trigger after check")?;
    transaction
        .commit()
        .await
        .context("failed to commit automation schedule update")?;
    publish_project_id_event(store, updated.project_id).await;
    Ok(updated)
}

async fn update_trigger_event_cursor(
    store: &Store,
    trigger: AutomationTriggerModel,
    last_event_id: Option<i64>,
) -> Result<AutomationTriggerModel> {
    let mut active: AutomationTriggerActiveModel = trigger.into();
    active.last_event_id = Set(last_event_id);
    active.updated_at = Set(utc_now());
    let transaction = store
        .db()
        .begin()
        .await
        .context("failed to begin automation event-cursor update")?;
    let updated = active
        .update(&transaction)
        .await
        .context("failed to update automation trigger event cursor")?;
    transaction
        .commit()
        .await
        .context("failed to commit automation event-cursor update")?;
    publish_project_id_event(store, updated.project_id).await;
    Ok(updated)
}

async fn publish_project_id_event(store: &Store, project_id: i64) {
    match projects::project_name_by_id(store, project_id).await {
        Ok(project_name) => events::publish_automation_changed(&project_name),
        Err(err) => {
            tracing::warn!(
                project_id,
                error = %format_args!("{err:#}"),
                "failed to resolve project for automation trigger UI event"
            );
        }
    }
}

async fn new_item_created_events(
    store: &Store,
    project_id: i64,
    last_event_id: Option<i64>,
) -> Result<Vec<work_item_event::Model>> {
    let mut query = work_item_event::Entity::find()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq("item_created"))
        .order_by_asc(work_item_event::Column::Id);
    if let Some(last_event_id) = last_event_id {
        query = query.filter(work_item_event::Column::Id.gt(last_event_id));
    }
    Ok(query
        .all(store.db().as_ref())
        .await
        .context("failed to load item-created events")?)
}

pub(crate) async fn latest_item_created_event_id(
    store: &Store,
    project_id: i64,
) -> Result<Option<i64>> {
    let event = work_item_event::Entity::find()
        .filter(work_item_event::Column::ProjectId.eq(project_id))
        .filter(work_item_event::Column::EventType.eq("item_created"))
        .order_by_desc(work_item_event::Column::Id)
        .limit(1)
        .one(store.db().as_ref())
        .await
        .context("failed to load latest item-created event")?;
    Ok(event.map(|event| event.id))
}

fn trigger_is_due(next_evaluation_at: Option<&str>) -> bool {
    let Some(next_evaluation_at) = next_evaluation_at else {
        return true;
    };
    let Ok(next) = OffsetDateTime::parse(next_evaluation_at, &Rfc3339) else {
        return true;
    };
    next <= OffsetDateTime::now_utc()
}
