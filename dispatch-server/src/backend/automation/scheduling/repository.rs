use super::model::{CreatedItemEvent, RuleSelection, ScheduleChange};
use crate::backend::{
    automation::{production::repository::record_on, rules::repository::encoding::model_to_view},
    entities::{
        automation_trigger::{self, AutomationTrigger, AutomationTriggerActiveModel},
        work_item_event,
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::{
    AutomationActivation, AutomationEffect, AutomationEvaluationOutcome, AutomationTriggerView,
};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
pub(crate) struct ScheduleRepository;
impl ScheduleRepository {
    pub(crate) async fn list_in(
        &self,
        tx: &Transaction,
        selection: RuleSelection,
        projects: Option<&[i64]>,
    ) -> Result<Vec<AutomationTriggerView>> {
        let mut query = AutomationTrigger::find();
        if let Some(projects) = projects {
            query =
                query.filter(automation_trigger::Column::ProjectId.is_in(projects.iter().copied()));
        }
        query = match selection {
            RuleSelection::Enabled => query
                .filter(automation_trigger::Column::Enabled.eq(true))
                .order_by_asc(automation_trigger::Column::Id),
            RuleSelection::Queued => query
                .filter(automation_trigger::Column::PendingEvaluationCount.gt(0))
                .order_by_desc(automation_trigger::Column::Priority)
                .order_by_asc(automation_trigger::Column::LastEvaluationQueuedAt)
                .order_by_asc(automation_trigger::Column::Id),
            RuleSelection::WorkItems(project_id) => query
                .filter(automation_trigger::Column::ProjectId.eq(project_id))
                .filter(automation_trigger::Column::Enabled.eq(true))
                .filter(
                    automation_trigger::Column::Activation
                        .eq(AutomationActivation::WorkItem.as_storage()),
                )
                .filter(
                    automation_trigger::Column::Effect
                        .eq(AutomationEffect::ConsumeWork.as_storage()),
                )
                .order_by_asc(automation_trigger::Column::Id),
        };
        query
            .all(tx.connection())
            .await
            .context("failed to load scheduled automation rules")?
            .into_iter()
            .map(model_to_view)
            .collect()
    }
    pub(crate) async fn get_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<AutomationTriggerView> {
        model_to_view(
            AutomationTrigger::find_by_id(id)
                .filter(automation_trigger::Column::ProjectId.eq(project_id))
                .one(tx.connection())
                .await
                .context("failed to resolve scheduled automation rule")?
                .ok_or_else(|| report!("automation rule {id} does not belong to this project"))?,
        )
    }
    pub(crate) async fn consume_in(
        &self,
        tx: &Transaction,
        rule: &AutomationTriggerView,
    ) -> Result<()> {
        AutomationTriggerActiveModel {
            id: Set(rule.id),
            pending_evaluation_count: Set(rule.pending_evaluation_count.saturating_sub(1)),
            updated_at: Set(utc_now()),
            ..Default::default()
        }
        .update(tx.connection())
        .await
        .context("failed to consume queued automation evaluation")?;
        Ok(())
    }
    pub(crate) async fn advance_in(
        &self,
        tx: &Transaction,
        rule: &AutomationTriggerView,
        change: ScheduleChange,
        next: Option<String>,
    ) -> Result<()> {
        let now = utc_now();
        let mut active = AutomationTriggerActiveModel {
            id: Set(rule.id),
            updated_at: Set(now.clone()),
            ..Default::default()
        };
        match change {
            ScheduleChange::Evaluated => {
                active.last_evaluated_at = Set(Some(now));
                active.evaluation_count = Set(rule.evaluation_count.saturating_add(1));
                active.next_evaluation_at = Set(next);
            }
            ScheduleChange::Checked => active.next_evaluation_at = Set(next),
            ScheduleChange::Cursor(id) => active.last_event_id = Set(id.max(rule.last_event_id)),
        }
        active
            .update(tx.connection())
            .await
            .context("failed to advance automation schedule")?;
        Ok(())
    }
    pub(crate) async fn created_events_in(
        &self,
        tx: &Transaction,
        project_id: i64,
        last_event_id: Option<i64>,
    ) -> Result<Vec<CreatedItemEvent>> {
        let mut query = work_item_event::Entity::find()
            .filter(work_item_event::Column::ProjectId.eq(project_id))
            .filter(work_item_event::Column::EventType.eq("item_created"))
            .order_by_asc(work_item_event::Column::Id);
        if let Some(last_event_id) = last_event_id {
            query = query.filter(work_item_event::Column::Id.gt(last_event_id));
        }
        Ok(query
            .all(tx.connection())
            .await
            .context("failed to load item-created events")?
            .into_iter()
            .map(|event| CreatedItemEvent {
                id: event.id,
                work_item_id: event.work_item_id,
            })
            .collect())
    }
    pub(crate) async fn record_in(
        &self,
        tx: &Transaction,
        rule: &AutomationTriggerView,
        outcome: AutomationEvaluationOutcome,
        work_item_id: Option<i64>,
        run_id: Option<i64>,
        error: Option<String>,
    ) -> Result<()> {
        record_on(tx.connection(), rule, outcome, work_item_id, run_id, error).await?;
        Ok(())
    }
}
