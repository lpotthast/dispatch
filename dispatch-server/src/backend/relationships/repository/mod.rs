pub(crate) mod records;
mod views;

use super::policy::{RelationshipEndpoints, RelationshipEvent};
use crate::backend::{items::events::repository as work_item_events, storage::Transaction};
use crate::shared::view_models::WorkItemRelationshipView;
use rootcause::Result;

pub(crate) struct RelationshipRepository;

impl RelationshipRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
    ) -> Result<Vec<WorkItemRelationshipView>> {
        self.ensure_item_in(transaction, project_id, item_id)
            .await?;
        let rows = records::for_item(transaction.connection(), project_id, item_id).await?;
        views::relationships_to_views(transaction.connection(), project_id, &rows).await
    }

    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
    ) -> Result<WorkItemRelationshipView> {
        let row = records::get(transaction.connection(), project_id, id).await?;
        views::relationship_to_view(transaction.connection(), row).await
    }

    pub(crate) async fn ensure_item_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
    ) -> Result<()> {
        crate::backend::items::repository::records::get(
            transaction.connection(),
            project_id,
            item_id,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn duplicate_exists_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        endpoints: RelationshipEndpoints,
        kind: &str,
        except_id: Option<i64>,
    ) -> Result<bool> {
        records::exact_relationship_exists(
            transaction.connection(),
            project_id,
            endpoints.source_work_item_id,
            endpoints.target_work_item_id,
            kind,
            except_id,
        )
        .await
    }

    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        endpoints: RelationshipEndpoints,
        kind: &str,
    ) -> Result<i64> {
        Ok(records::insert_in_tx(
            transaction.connection(),
            project_id,
            endpoints.source_work_item_id,
            endpoints.target_work_item_id,
            kind,
        )
        .await?
        .id)
    }

    pub(crate) async fn update_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        id: i64,
        kind: &str,
    ) -> Result<()> {
        let row = records::get(transaction.connection(), project_id, id).await?;
        records::update_kind_in_tx(transaction.connection(), row, kind).await?;
        Ok(())
    }

    pub(crate) async fn delete_in(&self, transaction: &Transaction, id: i64) -> Result<()> {
        records::delete_by_id_in_tx(transaction.connection(), id).await
    }

    pub(crate) async fn touch_endpoints_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        endpoints: RelationshipEndpoints,
        event: RelationshipEvent,
        attribution: crate::backend::items::events::model::EventAttribution<'_>,
    ) -> Result<()> {
        for (id, body) in [
            (endpoints.source_work_item_id, event.source_body),
            (endpoints.target_work_item_id, event.target_body),
        ] {
            let item = crate::backend::items::repository::records::get(
                transaction.connection(),
                project_id,
                id,
            )
            .await?;
            crate::backend::items::repository::records::touch(transaction.connection(), item)
                .await?;
            work_item_events::record_event_with_attribution_in_tx(
                transaction.connection(),
                project_id,
                Some(id),
                event.event_type,
                &body,
                attribution,
            )
            .await?;
        }
        Ok(())
    }
}
