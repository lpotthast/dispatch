use rootcause::{Result, prelude::*};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait};

use crate::{
    backend::{
        entities::{
            work_item::{WorkItemActiveModel, WorkItemModel},
            work_item_origin::WorkItemOriginActiveModel,
        },
        items::events::repository as work_item_events,
        items::labels::policy as item_labels,
        items::labels::repository::records as work_item_labels,
        items::labels::workflow as workflow_labels,
    },
    shared::view_models::WorkItemEventType,
};

use super::{model::InsertWorkItemOrigin, policy::CreateWorkItemPlan};
use crate::backend::storage::Transaction;
use crate::shared::view_models::WorkItemView;

pub(crate) struct ItemCreationRepository;
impl ItemCreationRepository {
    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        plan: CreateWorkItemPlan,
        origin: InsertWorkItemOrigin,
        attribution: crate::backend::items::events::model::EventAttribution<'_>,
    ) -> Result<WorkItemView> {
        let row = insert_planned(
            transaction.connection(),
            project_id,
            plan,
            crate::backend::storage::utc_now(),
            origin,
            attribution,
        )
        .await?;
        crate::backend::items::repository::views::model_to_view(transaction.connection(), row).await
    }
}
pub(crate) struct PlannedWorkItemInsert {
    pub(crate) active: WorkItemActiveModel,
    pub(crate) state_label: String,
    pub(crate) initial_labels: Vec<item_labels::NormalizedLabel>,
}

fn prepare_insert(
    plan: CreateWorkItemPlan,
    project_id: i64,
    created_at: String,
) -> PlannedWorkItemInsert {
    let active = WorkItemActiveModel {
        project_id: Set(project_id),
        title: Set(plan.title),
        description: Set(plan.description),
        agent_model_override: Set(plan.agent_model_override),
        agent_reasoning_effort_override: Set(plan
            .agent_reasoning_effort_override
            .map(|effort| effort.as_storage().to_owned())),
        version: Set(1),
        created_at: Set(created_at.clone()),
        updated_at: Set(created_at),
        ..Default::default()
    };
    PlannedWorkItemInsert {
        active,
        state_label: plan.state_label,
        initial_labels: plan.initial_labels,
    }
}
async fn insert_planned<C>(
    conn: &C,
    project_id: i64,
    plan: CreateWorkItemPlan,
    created_at: String,
    origin: InsertWorkItemOrigin,
    attribution: crate::backend::items::events::model::EventAttribution<'_>,
) -> Result<WorkItemModel>
where
    C: ConnectionTrait,
{
    let insert = prepare_insert(plan, project_id, created_at);
    let item = insert
        .active
        .insert(conn)
        .await
        .context("failed to create work item")?;
    crate::backend::items::labels::repository::workflow::apply_plan(
        conn,
        project_id,
        item.id,
        workflow_labels::state_workflow_label_plan(&insert.state_label),
    )
    .await?;
    for label in &insert.initial_labels {
        work_item_labels::insert_in_tx(
            conn,
            project_id,
            item.id,
            &label.key,
            label.value.as_deref(),
        )
        .await?;
    }
    WorkItemOriginActiveModel {
        work_item_id: Set(item.id),
        project_id: Set(project_id),
        origin_kind: Set(origin.kind.as_storage().to_owned()),
        actor_id: Set(origin.actor_id),
        agent_run_id: Set(origin.agent_run_id),
        producing_evaluation_id: Set(origin.producing_evaluation_id),
        trigger_id: Set(origin.trigger_id),
        trigger_revision_id: Set(origin.trigger_revision_id),
        trigger_name: Set(origin.trigger_name),
        bundle_key: Set(origin.bundle_key),
        deduplication_key: Set(origin.deduplication_key),
        created_at: Set(item.created_at.clone()),
    }
    .insert(conn)
    .await
    .context("failed to create work item origin")?;
    work_item_events::record_event_with_attribution_in_tx(
        conn,
        project_id,
        Some(item.id),
        WorkItemEventType::ItemCreated,
        "Created item",
        attribution,
    )
    .await?;

    Ok(item)
}
