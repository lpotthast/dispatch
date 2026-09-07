pub(crate) mod conditions;
pub(crate) mod records;
pub(crate) mod workflow;
use super::mutations::{AppliedLabelMutation, LabelMutationEvent};
use crate::backend::{
    items::events::{model::EventAttribution, repository as work_item_events},
    items::repository::{records as items, views},
    storage::Transaction,
};
use dispatch_types::{ProjectLabelView, WorkItemLabelView, WorkItemView};
use rootcause::Result;
pub(crate) struct LabelRepository;
impl LabelRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
    ) -> Result<Vec<WorkItemLabelView>> {
        records::for_item(transaction.connection(), project_id, item_id).await
    }
    pub(crate) async fn project_labels_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<ProjectLabelView>> {
        records::project_label_summaries(transaction.connection(), project_id).await
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        label_id: i64,
    ) -> Result<WorkItemLabelView> {
        records::get_for_item(transaction.connection(), project_id, item_id, label_id)
            .await
            .map(records::to_view)
    }
    pub(crate) async fn contains_key_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        key: &str,
        except_id: Option<i64>,
    ) -> Result<bool> {
        records::item_has_key(
            transaction.connection(),
            project_id,
            item_id,
            key,
            except_id,
        )
        .await
    }
    pub(crate) async fn add_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        key: &str,
        value: Option<&str>,
    ) -> Result<()> {
        records::insert_in_tx(transaction.connection(), project_id, item_id, key, value).await?;
        Ok(())
    }
    pub(crate) async fn update_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        label_id: i64,
        plan: &AppliedLabelMutation,
    ) -> Result<()> {
        let existing =
            records::get_for_item(transaction.connection(), project_id, item_id, label_id).await?;
        records::update_in_tx(
            transaction.connection(),
            existing,
            plan.key.clone(),
            plan.value.clone(),
        )
        .await?;
        Ok(())
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        key: &str,
        label_id: i64,
    ) -> Result<()> {
        records::delete_by_id_in_tx(transaction.connection(), project_id, key, label_id).await?;
        Ok(())
    }
    pub(crate) async fn finish_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        event: LabelMutationEvent,
        attribution: EventAttribution<'_>,
    ) -> Result<WorkItemView> {
        let existing = items::get(transaction.connection(), project_id, item_id).await?;
        let updated = items::touch(transaction.connection(), existing).await?;
        work_item_events::record_event_with_attribution_in_tx(
            transaction.connection(),
            project_id,
            Some(item_id),
            event.event_type,
            &event.body,
            attribution,
        )
        .await?;
        views::model_to_view(transaction.connection(), updated).await
    }
}
