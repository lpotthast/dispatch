use super::{
    AddCommentRequest,
    model::{CommentTarget, ItemScope},
    repository::CommentRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use crate::shared::view_models::CommentView;
use rootcause::{Result, prelude::*};
use std::sync::Arc;

pub(crate) struct CommentService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<CommentRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}

impl CommentService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<CommentRepository>,
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

    pub(crate) fn validate(input: &AddCommentRequest) -> Result<()> {
        if input.body.trim().is_empty() {
            bail!("comment body cannot be empty");
        }
        Ok(())
    }

    pub(crate) async fn add(
        &self,
        target: CommentTarget<'_>,
        input: AddCommentRequest,
        attribution: AttributionInput,
    ) -> Result<CommentView> {
        let transaction = self.transactions.begin().await?;
        let (scope, item_id) = self.resolve_in(&transaction, target).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, &scope.project_name, attribution, false)
            .await?;
        attribution.ensure_item_mutation("add item comments")?;
        Self::validate(&input)?;
        let comment = self
            .repository
            .insert_in(&transaction, item_id, input)
            .await?;
        self.repository
            .record_addition_in(&transaction, scope.project_id, item_id, attribution.event())
            .await?;
        transaction.commit().await?;
        self.events
            .publish_comment_changed(&scope.project_name, item_id);
        Ok(comment)
    }

    pub(crate) async fn list(&self, project: &str, item_id: i64) -> Result<Vec<CommentView>> {
        let transaction = self.transactions.begin().await?;
        self.resolve_in(
            &transaction,
            CommentTarget::ProjectItem { project, item_id },
        )
        .await?;
        let comments = self.repository.list_in(&transaction, item_id).await?;
        transaction.commit().await?;
        Ok(comments)
    }

    // Administrative maintenance preserves the existing comment identity and creation timestamp.
    pub(crate) async fn update(
        &self,
        item_id: i64,
        id: i64,
        input: AddCommentRequest,
    ) -> Result<CommentView> {
        Self::validate(&input)?;
        let transaction = self.transactions.begin().await?;
        let (scope, _) = self
            .resolve_in(&transaction, CommentTarget::Item(item_id))
            .await?;
        let existing = self.repository.get_in(&transaction, item_id, id).await?;
        let comment = self
            .repository
            .update_in(&transaction, existing, input)
            .await?;
        transaction.commit().await?;
        self.events
            .publish_comment_changed(&scope.project_name, item_id);
        Ok(comment)
    }

    pub(crate) async fn delete(&self, item_id: i64, id: i64) -> Result<()> {
        let transaction = self.transactions.begin().await?;
        let (scope, _) = self
            .resolve_in(&transaction, CommentTarget::Item(item_id))
            .await?;
        self.repository.get_in(&transaction, item_id, id).await?;
        self.repository.delete_in(&transaction, id).await?;
        transaction.commit().await?;
        self.events
            .publish_comment_changed(&scope.project_name, item_id);
        Ok(())
    }

    async fn resolve_in(
        &self,
        transaction: &Transaction,
        target: CommentTarget<'_>,
    ) -> Result<(ItemScope, i64)> {
        match target {
            CommentTarget::ProjectItem { project, item_id } => {
                let project_id = self.projects.id_in(transaction, project).await?;
                self.repository
                    .ensure_item_in(transaction, project_id, item_id)
                    .await?;
                Ok((
                    ItemScope {
                        project_id,
                        project_name: project.to_owned(),
                    },
                    item_id,
                ))
            }
            CommentTarget::Item(item_id) => Ok((
                self.repository.item_scope_in(transaction, item_id).await?,
                item_id,
            )),
        }
    }
}
