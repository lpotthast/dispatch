use super::{
    policy::{
        CreateRelationshipMutation, DeleteRelationshipMutation, RelationshipEndpoints,
        UpdateRelationshipMutation,
    },
    repository::RelationshipRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use crate::shared::view_models::{
    DeleteWorkItemRelationshipResponse, WorkItemRelationshipListEntry, WorkItemRelationshipView,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;

pub(crate) struct RelationshipService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<RelationshipRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}

impl RelationshipService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<RelationshipRepository>,
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

    pub(crate) async fn list(
        &self,
        project: &str,
        item_id: i64,
    ) -> Result<Vec<WorkItemRelationshipListEntry>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let views = self
            .repository
            .list_in(&transaction, project_id, item_id)
            .await?;
        let entries = views
            .into_iter()
            .map(|relationship| {
                Ok(WorkItemRelationshipListEntry {
                    direction: RelationshipEndpoints::from_relationship(&relationship)?
                        .direction_for_item(item_id),
                    relationship,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        transaction.commit().await?;
        Ok(entries)
    }

    pub(crate) async fn create(
        &self,
        project: &str,
        source_id: i64,
        target_id: i64,
        kind: String,
        input: AttributionInput,
    ) -> Result<WorkItemRelationshipListEntry> {
        let transaction = self.transactions.begin().await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project, input, false)
            .await?;
        attribution.ensure_item_mutation("create item relationships")?;
        let mutation = CreateRelationshipMutation::new(source_id, target_id, kind)?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let endpoints = mutation.endpoints();
        self.repository
            .ensure_item_in(&transaction, project_id, source_id)
            .await?;
        self.repository
            .ensure_item_in(&transaction, project_id, target_id)
            .await?;
        self.ensure_no_duplicate(&transaction, project_id, endpoints, mutation.kind(), None)
            .await?;
        let id = self
            .repository
            .insert_in(&transaction, project_id, endpoints, mutation.kind())
            .await?;
        self.repository
            .touch_endpoints_in(
                &transaction,
                project_id,
                endpoints,
                mutation.created_event(),
                attribution.event(),
            )
            .await?;
        let relationship = self.repository.get_in(&transaction, project_id, id).await?;
        transaction.commit().await?;
        self.publish_changes(project, endpoints);
        Ok(WorkItemRelationshipListEntry {
            relationship,
            direction: endpoints.direction_for_item(source_id),
        })
    }

    pub(crate) async fn update(
        &self,
        project: &str,
        requested_item_id: Option<i64>,
        id: i64,
        kind: String,
        input: AttributionInput,
    ) -> Result<WorkItemRelationshipView> {
        let transaction = self.transactions.begin().await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project, input, false)
            .await?;
        attribution.ensure_item_mutation("update item relationships")?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        if let Some(item_id) = requested_item_id {
            self.repository
                .ensure_item_in(&transaction, project_id, item_id)
                .await?;
        }
        let relationship = self.repository.get_in(&transaction, project_id, id).await?;
        let mutation = UpdateRelationshipMutation::new(&relationship, requested_item_id, kind)?;
        let endpoints = mutation.endpoints();
        self.ensure_no_duplicate(
            &transaction,
            project_id,
            endpoints,
            mutation.kind(),
            mutation.duplicate_exclusion(),
        )
        .await?;
        self.repository
            .update_in(&transaction, project_id, id, mutation.kind())
            .await?;
        self.repository
            .touch_endpoints_in(
                &transaction,
                project_id,
                endpoints,
                mutation.updated_event(),
                attribution.event(),
            )
            .await?;
        let relationship = self.repository.get_in(&transaction, project_id, id).await?;
        transaction.commit().await?;
        self.publish_changes(project, endpoints);
        Ok(relationship)
    }

    pub(crate) async fn delete(
        &self,
        project: &str,
        requested_item_id: Option<i64>,
        id: i64,
        input: AttributionInput,
    ) -> Result<DeleteWorkItemRelationshipResponse> {
        let transaction = self.transactions.begin().await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project, input, false)
            .await?;
        attribution.ensure_item_mutation("delete item relationships")?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        if let Some(item_id) = requested_item_id {
            self.repository
                .ensure_item_in(&transaction, project_id, item_id)
                .await?;
        }
        let relationship = self.repository.get_in(&transaction, project_id, id).await?;
        let mutation = DeleteRelationshipMutation::new(&relationship, requested_item_id)?;
        let endpoints = mutation.endpoints();
        self.repository
            .touch_endpoints_in(
                &transaction,
                project_id,
                endpoints,
                mutation.deleted_event(),
                attribution.event(),
            )
            .await?;
        // Return the deleted snapshot with both endpoint versions after their touches.
        let relationship = self.repository.get_in(&transaction, project_id, id).await?;
        self.repository.delete_in(&transaction, id).await?;
        transaction.commit().await?;
        self.publish_changes(project, endpoints);
        Ok(DeleteWorkItemRelationshipResponse {
            deleted: true,
            relationship,
        })
    }

    async fn ensure_no_duplicate(
        &self,
        transaction: &Transaction,
        project_id: i64,
        endpoints: RelationshipEndpoints,
        kind: &str,
        except_id: Option<i64>,
    ) -> Result<()> {
        if self
            .repository
            .duplicate_exists_in(transaction, project_id, endpoints, kind, except_id)
            .await?
        {
            bail!(
                "duplicate relationship already exists for source item {}, target item {}, and kind '{kind}'",
                endpoints.source_work_item_id,
                endpoints.target_work_item_id
            );
        }
        Ok(())
    }

    fn publish_changes(&self, project: &str, endpoints: RelationshipEndpoints) {
        self.events
            .publish_work_item_changed(project, endpoints.source_work_item_id);
        self.events
            .publish_work_item_changed(project, endpoints.target_work_item_id);
    }
}
