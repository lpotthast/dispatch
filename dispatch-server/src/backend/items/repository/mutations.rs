use super::{ItemRepository, records, views};
use crate::backend::{
    entities::work_item::{WorkItem, WorkItemActiveModel},
    items::events::{model::EventAttribution, repository as work_item_events},
    items::labels::catalog::repository as label_keys,
    items::labels::repository::records as work_item_labels,
    items::labels::workflow as workflow_labels,
    items::policy::AppliedWorkItemUpdate,
    storage::Transaction,
};
use dispatch_types::{WorkItemEventType, WorkItemView};
use rootcause::{Result, prelude::*};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
impl ItemRepository {
    pub(crate) async fn save_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        applied: AppliedWorkItemUpdate,
        attribution: EventAttribution<'_>,
    ) -> Result<WorkItemView> {
        let item = applied.item;
        let item_id = item.id;
        WorkItemActiveModel {
            id: Set(item_id),
            title: Set(item.title),
            description: Set(item.description),
            agent_model_override: Set(item.agent_model_override),
            agent_reasoning_effort_override: Set(item
                .agent_reasoning_effort_override
                .map(|effort| effort.as_storage().to_owned())),
            version: Set(item.version),
            updated_at: Set(item.updated_at),
            ..Default::default()
        }
        .update(transaction.connection())
        .await
        .context("failed to update work item")?;
        if applied.record_item_updated_event {
            work_item_events::record_event_with_attribution_in_tx(
                transaction.connection(),
                project_id,
                Some(item_id),
                WorkItemEventType::ItemUpdated,
                "Updated item",
                attribution,
            )
            .await?;
        }
        if let Some(state) = applied.state {
            crate::backend::items::labels::repository::workflow::apply_plan(
                transaction.connection(),
                project_id,
                item_id,
                workflow_labels::state_workflow_label_plan(&state),
            )
            .await?;
            work_item_events::record_event_with_attribution_in_tx(
                transaction.connection(),
                project_id,
                Some(item_id),
                WorkItemEventType::ItemMoved,
                &workflow_labels::state_move_event_body(&state),
                attribution,
            )
            .await?;
        }
        views::model_to_view(
            transaction.connection(),
            records::get(transaction.connection(), project_id, item_id).await?,
        )
        .await
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
    ) -> Result<(u64, Vec<i64>)> {
        let related_item_ids =
            crate::backend::relationships::repository::records::related_item_ids_for_item(
                transaction.connection(),
                project_id,
                item_id,
            )
            .await?;
        let label_keys_for_item =
            work_item_labels::keys_for_item(transaction.connection(), project_id, item_id).await?;

        work_item_events::record_event_in_tx(
            transaction.connection(),
            project_id,
            Some(item_id),
            WorkItemEventType::ItemDeleted,
            "Deleted item",
        )
        .await?;
        for related_item_id in &related_item_ids {
            work_item_events::record_event_in_tx(
                transaction.connection(),
                project_id,
                Some(*related_item_id),
                WorkItemEventType::RelationshipDeleted,
                &format!("Deleted relationships touching removed item #{item_id}"),
            )
            .await?;
        }
        let delete_result = WorkItem::delete_by_id(item_id)
            .exec(transaction.connection())
            .await
            .context("failed to delete work item")?;
        for key in label_keys_for_item {
            label_keys::forget_if_unused_in_tx(transaction.connection(), project_id, &key).await?;
        }
        Ok((delete_result.rows_affected, related_item_ids))
    }
}
