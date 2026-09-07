use super::{model::LaneFields, repository::LaneRepository};
use crate::backend::{
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{TransactionManager, utc_now},
};
use dispatch_types::SwimLaneView;
use rootcause::Result;
use std::sync::Arc;
pub(crate) struct LaneService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<LaneRepository>,
    events: UiEventBus,
}
impl LaneService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<LaneRepository>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            events,
        }
    }
    #[cfg(test)]
    pub(crate) async fn list(&self, project: &str) -> Result<Vec<SwimLaneView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let records = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(records)
    }
    pub(crate) async fn list_by_id(&self, project_id: i64) -> Result<Vec<SwimLaneView>> {
        let transaction = self.transactions.begin().await?;
        let records = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(records)
    }
    pub(crate) async fn create(&self, project_id: i64, input: LaneFields) -> Result<SwimLaneView> {
        let input = input.validate()?;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let record = self
            .repository
            .insert_in(&transaction, project_id, input)
            .await?;
        transaction.commit().await?;
        self.events.publish_swim_lane_changed(&project);
        Ok(record)
    }
    pub(crate) async fn update(
        &self,
        project_id: i64,
        id: i64,
        input: LaneFields,
    ) -> Result<SwimLaneView> {
        let input = input.validate()?;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let mut existing = self.repository.get_in(&transaction, project_id, id).await?;
        existing.identifier = input.identifier;
        existing.name = input.name;
        existing.position = input.position;
        existing.filter = input.filter;
        existing.item_order = input.item_order;
        existing.can_create_items = input.can_create_items;
        existing.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, existing).await?;
        transaction.commit().await?;
        self.events.publish_swim_lane_changed(&project);
        Ok(record)
    }
    pub(crate) async fn delete(&self, project_id: i64, id: i64) -> Result<u64> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        self.repository.get_in(&transaction, project_id, id).await?;
        let count = self
            .repository
            .delete_in(&transaction, project_id, id)
            .await?;
        transaction.commit().await?;
        self.events.publish_swim_lane_changed(&project);
        Ok(count)
    }
}
