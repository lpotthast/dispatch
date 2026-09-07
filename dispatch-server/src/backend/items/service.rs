use super::{policy::WorkItemUpdatePlan, repository::ItemRepository};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    projects::{self, ProjectReference, repository::ProjectRepository},
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{
    BoardWorkItemView, UpdateWorkItemRequest, WorkItemEventView, WorkItemPage,
    WorkItemSearchRequest, WorkItemView,
};
use rootcause::Result;
use std::sync::Arc;

pub(crate) struct ItemService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<ItemRepository>,
    history: Arc<super::events::repository::EventRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}
impl ItemService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<ItemRepository>,
        history: Arc<super::events::repository::EventRepository>,
        attribution: Arc<AttributionService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            history,
            attribution,
            events,
        }
    }
    pub(crate) async fn update(
        &self,
        project: ProjectReference<'_>,
        item_id: i64,
        update: UpdateWorkItemRequest,
        attribution: AttributionInput,
    ) -> Result<WorkItemView> {
        let plan = WorkItemUpdatePlan::new(update)?;
        let transaction = self.transactions.begin().await?;
        let project = self.projects.resolve_in(&transaction, project).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, &project.name, attribution, false)
            .await?;
        attribution.ensure_item_mutation("update work items")?;
        let existing = self
            .repository
            .get_in(&transaction, project.id, item_id)
            .await?;
        super::policy::check_expected_version(plan.expect_version(), existing.version)?;
        let applied = plan.apply_to(existing)?;
        projects::validate_agent_model_reasoning_effort(
            "effective agent model",
            applied
                .agent_model_override
                .as_deref()
                .or(project.default_agent_model.as_deref()),
            "effective agent reasoning effort",
            applied
                .agent_reasoning_effort_override
                .or(project.default_agent_reasoning_effort),
        )?;
        let item = self
            .repository
            .save_in(&transaction, project.id, applied, attribution.event())
            .await?;
        transaction.commit().await?;
        self.events
            .publish_work_item_changed(&project.name, item_id);
        Ok(item)
    }
    pub(crate) async fn delete(&self, project: ProjectReference<'_>, item_id: i64) -> Result<u64> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.resolve_in(&transaction, project).await?;
        self.repository
            .get_in(&transaction, project.id, item_id)
            .await?;
        let (count, related_ids) = self
            .repository
            .delete_in(&transaction, project.id, item_id)
            .await?;
        transaction.commit().await?;
        self.events
            .publish_work_item_changed(&project.name, item_id);
        for id in related_ids {
            self.events.publish_work_item_changed(&project.name, id);
        }
        Ok(count)
    }
    pub(crate) async fn get(&self, project: &str, item_id: i64) -> Result<WorkItemView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let item = self
            .repository
            .get_in(&transaction, project_id, item_id)
            .await?;
        transaction.commit().await?;
        Ok(item)
    }
    pub(crate) async fn list(
        &self,
        project: &str,
        state: Option<String>,
    ) -> Result<Vec<WorkItemView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let items = self.list_in(&transaction, project_id, state).await?;
        transaction.commit().await?;
        Ok(items)
    }
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        state: Option<String>,
    ) -> Result<Vec<WorkItemView>> {
        self.repository
            .list_in(transaction, project_id, state)
            .await
    }
    pub(crate) async fn board(&self, project_id: i64) -> Result<Vec<BoardWorkItemView>> {
        let transaction = self.transactions.begin().await?;
        let items = self.repository.board_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(items)
    }
    pub(crate) async fn count_outside_states(&self, project_id: i64) -> Result<i64> {
        let transaction = self.transactions.begin().await?;
        let count = self
            .repository
            .count_outside_states_in(&transaction, project_id)
            .await?;
        transaction.commit().await?;
        Ok(count)
    }
    pub(crate) async fn search(
        &self,
        project: &str,
        request: WorkItemSearchRequest,
    ) -> Result<WorkItemPage> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let page = self
            .repository
            .search_in(&transaction, project_id, request)
            .await?;
        transaction.commit().await?;
        Ok(page)
    }
    pub(crate) async fn events(
        &self,
        project: &str,
        item_id: Option<i64>,
        since_id: Option<i64>,
    ) -> Result<Vec<WorkItemEventView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let events = self
            .history
            .list_in(&transaction, project_id, item_id, since_id)
            .await?;
        transaction.commit().await?;
        Ok(events)
    }
}
