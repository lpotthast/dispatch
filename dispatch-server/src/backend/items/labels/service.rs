use super::{
    mutations::{AddLabelMutation, DeleteLabelMutation, UpdateLabelMutation},
    repository::LabelRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    items::{policy::check_expected_version, repository::ItemRepository},
    projects::repository::ProjectRepository,
    storage::TransactionManager,
};
use dispatch_types::{
    CreateWorkItemLabelRequest, DeleteWorkItemLabelResponse, ProjectLabelView,
    UpdateWorkItemLabelRequest, WorkItemLabelView, WorkItemView,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;

pub(crate) struct LabelService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<LabelRepository>,
    items: Arc<ItemRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}
enum Mutation {
    Add(CreateWorkItemLabelRequest),
    Update(i64, UpdateWorkItemLabelRequest),
    Delete(i64),
}
impl LabelService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<LabelRepository>,
        items: Arc<ItemRepository>,
        attribution: Arc<AttributionService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            items,
            attribution,
            events,
        }
    }
    pub(crate) async fn list(&self, project: &str, item_id: i64) -> Result<Vec<WorkItemLabelView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        self.items.get_in(&transaction, project_id, item_id).await?;
        let labels = self
            .repository
            .list_in(&transaction, project_id, item_id)
            .await?;
        transaction.commit().await?;
        Ok(labels)
    }
    pub(crate) async fn project_labels(&self, project: &str) -> Result<Vec<ProjectLabelView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let labels = self
            .repository
            .project_labels_in(&transaction, project_id)
            .await?;
        transaction.commit().await?;
        Ok(labels)
    }
    pub(crate) async fn project_labels_by_id(
        &self,
        project_id: i64,
    ) -> Result<Vec<ProjectLabelView>> {
        let transaction = self.transactions.begin().await?;
        let labels = self
            .repository
            .project_labels_in(&transaction, project_id)
            .await?;
        transaction.commit().await?;
        Ok(labels)
    }
    pub(crate) async fn add(
        &self,
        project: &str,
        item_id: i64,
        request: CreateWorkItemLabelRequest,
        expected: Option<i64>,
        attribution: AttributionInput,
    ) -> Result<WorkItemView> {
        self.edit(
            project,
            item_id,
            Mutation::Add(request),
            expected,
            attribution,
        )
        .await
    }
    pub(crate) async fn update(
        &self,
        project: &str,
        item_id: i64,
        label_id: i64,
        request: UpdateWorkItemLabelRequest,
        attribution: AttributionInput,
    ) -> Result<WorkItemView> {
        let expected = request.expect_version;
        self.edit(
            project,
            item_id,
            Mutation::Update(label_id, request),
            expected,
            attribution,
        )
        .await
    }
    pub(crate) async fn delete(
        &self,
        project: &str,
        item_id: i64,
        label_id: i64,
        expected: Option<i64>,
        attribution: AttributionInput,
    ) -> Result<DeleteWorkItemLabelResponse> {
        let work_item = self
            .edit(
                project,
                item_id,
                Mutation::Delete(label_id),
                expected,
                attribution,
            )
            .await?;
        Ok(DeleteWorkItemLabelResponse {
            deleted: true,
            label_id,
            work_item,
        })
    }
    async fn edit(
        &self,
        project: &str,
        item_id: i64,
        mutation: Mutation,
        expected: Option<i64>,
        attribution: AttributionInput,
    ) -> Result<WorkItemView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project, attribution, false)
            .await?;
        attribution.ensure_item_mutation(match mutation {
            Mutation::Add(_) => "add item labels",
            Mutation::Update(..) => "update item labels",
            Mutation::Delete(_) => "delete item labels",
        })?;
        let item = self.items.get_in(&transaction, project_id, item_id).await?;
        check_expected_version(expected, item.version)?;
        let event = match mutation {
            Mutation::Add(request) => {
                let plan = AddLabelMutation::new(request.key, request.value)?;
                if self
                    .repository
                    .contains_key_in(&transaction, project_id, item_id, &plan.key, None)
                    .await?
                {
                    bail!("item already has label '{}'", plan.key);
                }
                self.repository
                    .add_in(
                        &transaction,
                        project_id,
                        item_id,
                        &plan.key,
                        plan.value.as_deref(),
                    )
                    .await?;
                plan.added_event()
            }
            Mutation::Update(label_id, request) => {
                let plan = UpdateLabelMutation::new(request.key, request.value)?;
                let existing = self
                    .repository
                    .get_in(&transaction, project_id, item_id, label_id)
                    .await?;
                let plan = plan.apply_to(&existing)?;
                if self
                    .repository
                    .contains_key_in(&transaction, project_id, item_id, &plan.key, Some(label_id))
                    .await?
                {
                    bail!("item already has label '{}'", plan.key);
                }
                self.repository
                    .update_in(&transaction, project_id, item_id, label_id, &plan)
                    .await?;
                plan.updated_event()
            }
            Mutation::Delete(label_id) => {
                let existing = self
                    .repository
                    .get_in(&transaction, project_id, item_id, label_id)
                    .await?;
                let plan = DeleteLabelMutation::new(&existing)?;
                self.repository
                    .delete_in(&transaction, project_id, &existing.key, plan.label_id())
                    .await?;
                plan.deleted_event()
            }
        };
        let item = self
            .repository
            .finish_in(
                &transaction,
                project_id,
                item_id,
                event,
                attribution.event(),
            )
            .await?;
        transaction.commit().await?;
        self.events.publish_work_item_changed(project, item_id);
        Ok(item)
    }
}
