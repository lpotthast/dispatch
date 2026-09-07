use super::{model::StateFields, repository::StateRepository};
use crate::backend::{
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{TransactionManager, utc_now},
};
use dispatch_types::WorkItemStateView;
use rootcause::Result;
use std::sync::Arc;
pub(crate) struct StateService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<StateRepository>,
    events: UiEventBus,
}
impl StateService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<StateRepository>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            events,
        }
    }
    pub(crate) async fn list(&self, project: &str) -> Result<Vec<WorkItemStateView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let records = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(records)
    }
    pub(crate) async fn list_by_id(&self, project_id: i64) -> Result<Vec<WorkItemStateView>> {
        let transaction = self.transactions.begin().await?;
        let records = self.repository.list_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(records)
    }
    pub(crate) async fn create(
        &self,
        project_id: i64,
        input: StateFields,
    ) -> Result<WorkItemStateView> {
        let input = input.validate()?;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let record = self
            .repository
            .insert_in(&transaction, project_id, input)
            .await?;
        transaction.commit().await?;
        self.events.publish_work_item_state_changed(&project);
        Ok(record)
    }
    pub(crate) async fn update(
        &self,
        project_id: i64,
        id: i64,
        input: StateFields,
    ) -> Result<WorkItemStateView> {
        let input = input.validate()?;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let mut existing = self.repository.get_in(&transaction, project_id, id).await?;
        existing.identifier = input.identifier;
        existing.name = input.name;
        existing.position = input.position;

        existing.updated_at = utc_now();
        let record = self.repository.save_in(&transaction, existing).await?;
        transaction.commit().await?;
        self.events.publish_work_item_state_changed(&project);
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
        self.events.publish_work_item_state_changed(&project);
        Ok(count)
    }
}
