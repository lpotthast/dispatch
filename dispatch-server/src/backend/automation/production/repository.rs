use crate::backend::{
    entities::{automation_evaluation, automation_trigger, work_item, work_item_origin},
    storage::{Transaction, utc_now},
};
use dispatch_types::{
    AutomationEvaluationOutcome, AutomationTriggerView, ProduceDeduplication, WorkItemOriginKind,
};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    QueryOrder, QuerySelect, QueryTrait,
};

pub(crate) struct ProductionRepository;
impl ProductionRepository {
    pub(crate) async fn ensure_rule_in(
        &self,
        transaction: &Transaction,
        rule: &AutomationTriggerView,
    ) -> Result<()> {
        let current = automation_trigger::Entity::find_by_id(rule.id)
            .filter(automation_trigger::Column::ProjectId.eq(rule.project_id))
            .one(transaction.connection())
            .await
            .context("failed to resolve producing automation")?
            .ok_or_else(|| report!("producing automation does not exist in this project"))?;
        if current.current_revision_id != rule.current_revision_id {
            bail!("producing automation changed; retry its evaluation");
        }
        Ok(())
    }
    pub(crate) async fn unfinished_duplicate_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        trigger_id: i64,
        policy: &ProduceDeduplication,
    ) -> Result<Option<i64>> {
        let mut origins = work_item_origin::Entity::find()
            .select_only()
            .column(work_item_origin::Column::WorkItemId)
            .filter(work_item_origin::Column::ProjectId.eq(project_id))
            .filter(
                work_item_origin::Column::OriginKind
                    .eq(WorkItemOriginKind::ProducingAutomation.as_storage()),
            );
        origins = match policy {
            ProduceDeduplication::Always => return Ok(None),
            ProduceDeduplication::WhileUnfinishedForTrigger => {
                origins.filter(work_item_origin::Column::TriggerId.eq(trigger_id))
            }
            ProduceDeduplication::WhileUnfinishedForKey { key } => {
                origins.filter(work_item_origin::Column::DeduplicationKey.eq(key))
            }
        };
        Ok(work_item::Entity::find()
            .select_only()
            .column(work_item::Column::Id)
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::FinishedAt.is_null())
            .filter(work_item::Column::Id.in_subquery(origins.into_query()))
            .order_by_desc(work_item::Column::Id)
            .into_tuple::<i64>()
            .one(transaction.connection())
            .await
            .context("failed to inspect produced-work duplicate")?)
    }
    pub(crate) async fn record_in(
        &self,
        transaction: &Transaction,
        rule: &AutomationTriggerView,
        outcome: AutomationEvaluationOutcome,
        item_id: Option<i64>,
    ) -> Result<i64> {
        record_on(transaction.connection(), rule, outcome, item_id, None, None).await
    }
    pub(crate) async fn attach_item_in(
        &self,
        transaction: &Transaction,
        evaluation_id: i64,
        item_id: i64,
    ) -> Result<()> {
        automation_evaluation::ActiveModel {
            id: Set(evaluation_id),
            work_item_id: Set(Some(item_id)),
            ..Default::default()
        }
        .update(transaction.connection())
        .await
        .context("failed to attach produced item to evaluation")?;
        Ok(())
    }
}

pub(crate) async fn record_on<C: ConnectionTrait>(
    conn: &C,
    automation: &AutomationTriggerView,
    outcome: AutomationEvaluationOutcome,
    work_item_id: Option<i64>,
    run_id: Option<i64>,
    error: Option<String>,
) -> Result<i64> {
    let now = utc_now();
    Ok(automation_evaluation::ActiveModel {
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
    .context("failed to record automation evaluation")?
    .id)
}
