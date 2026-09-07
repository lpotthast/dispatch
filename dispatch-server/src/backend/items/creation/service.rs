use super::{
    model::{CreateWorkItem, InsertWorkItemOrigin},
    policy::CreateWorkItemPlan,
    repository::ItemCreationRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    items::events::model::EventAttribution,
    projects::{ProjectReference, repository::ProjectRepository},
    storage::{Transaction, TransactionManager},
};
use dispatch_types::WorkItemView;
use rootcause::Result;
use std::sync::Arc;

pub(crate) struct ItemCreationService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<ItemCreationRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}
impl ItemCreationService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<ItemCreationRepository>,
        attribution: Arc<AttributionService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            attribution,
            events,
        }
    }
    pub(crate) async fn create(
        &self,
        project: ProjectReference<'_>,
        input: CreateWorkItem,
        attribution: AttributionInput,
    ) -> Result<WorkItemView> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.resolve_in(&transaction, project).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, &project.name, attribution, false)
            .await?;
        attribution.ensure_item_mutation("create work items")?;
        let item = self
            .create_in(
                &transaction,
                project.id,
                input,
                attribution.item_origin(),
                attribution.event(),
            )
            .await?;
        transaction.commit().await?;
        self.events
            .publish_work_item_changed(&project.name, item.id);
        Ok(item)
    }
    /// The coordinating workflow owns this transaction and publishes only after its commit.
    pub(crate) async fn create_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        input: CreateWorkItem,
        origin: InsertWorkItemOrigin,
        attribution: EventAttribution<'_>,
    ) -> Result<WorkItemView> {
        let plan = CreateWorkItemPlan::new(input)?;
        let project = self.projects.by_id_in(transaction, project_id).await?;
        // The typed project carries the effective defaults used by item-level validation.
        crate::backend::projects::validate_agent_model_reasoning_effort(
            "effective agent model",
            plan.agent_model_override()
                .or(project.default_agent_model.as_deref()),
            "effective agent reasoning effort",
            plan.agent_reasoning_effort_override()
                .or(project.default_agent_reasoning_effort),
        )?;
        self.repository
            .insert_in(transaction, project_id, plan, origin, attribution)
            .await
    }
}
