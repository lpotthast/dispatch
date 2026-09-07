pub(crate) mod records;

use super::{AddCommentRequest, model::ItemScope};
use crate::backend::{
    entities::{comment, project, work_item},
    items::events::repository as work_item_events,
    storage::Transaction,
};
use crate::shared::view_models::{CommentView, WorkItemEventType};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QuerySelect,
};

pub(crate) struct CommentRepository;

impl CommentRepository {
    pub(crate) async fn item_scope_in(
        &self,
        transaction: &Transaction,
        item_id: i64,
    ) -> Result<ItemScope> {
        let (project_id, project_name) = work_item::Entity::find_by_id(item_id)
            .select_only()
            .column(work_item::Column::ProjectId)
            .column_as(project::Column::Name, "project_name")
            .join(
                sea_orm::JoinType::InnerJoin,
                work_item::Entity::belongs_to(project::Entity)
                    .from(work_item::Column::ProjectId)
                    .to(project::Column::Id)
                    .into(),
            )
            .into_tuple::<(i64, String)>()
            .one(transaction.connection())
            .await
            .context("failed to load comment item scope")?
            .ok_or_else(|| report!("work item {item_id} does not exist"))?;
        Ok(ItemScope {
            project_id,
            project_name,
        })
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

    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        item_id: i64,
    ) -> Result<Vec<CommentView>> {
        records::list_for_item(transaction.connection(), item_id)
            .await?
            .into_iter()
            .map(records::to_view)
            .collect()
    }

    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        item_id: i64,
        id: i64,
    ) -> Result<CommentView> {
        let row = comment::Entity::find_by_id(id)
            .filter(comment::Column::WorkItemId.eq(item_id))
            .one(transaction.connection())
            .await
            .context("failed to load comment")?
            .ok_or_else(|| report!("comment {id} does not exist on item {item_id}"))?;
        records::to_view(row)
    }

    pub(crate) async fn insert_in(
        &self,
        transaction: &Transaction,
        item_id: i64,
        input: AddCommentRequest,
    ) -> Result<CommentView> {
        records::to_view(
            records::insert_in_tx(
                transaction.connection(),
                item_id,
                input.author_type,
                input.author_name,
                &input.body,
            )
            .await?,
        )
    }

    pub(crate) async fn record_addition_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        attribution: crate::backend::items::events::model::EventAttribution<'_>,
    ) -> Result<()> {
        let item = crate::backend::items::repository::records::get(
            transaction.connection(),
            project_id,
            item_id,
        )
        .await?;
        crate::backend::items::repository::records::touch(transaction.connection(), item).await?;
        work_item_events::record_event_with_attribution_in_tx(
            transaction.connection(),
            project_id,
            Some(item_id),
            WorkItemEventType::CommentAdded,
            "Added comment",
            attribution,
        )
        .await?;
        Ok(())
    }

    pub(crate) async fn update_in(
        &self,
        transaction: &Transaction,
        existing: CommentView,
        input: AddCommentRequest,
    ) -> Result<CommentView> {
        let active = comment::ActiveModel {
            id: Set(existing.id),
            author_type: Set(input.author_type.as_storage().to_owned()),
            author_name: Set(input.author_name),
            body: Set(input.body),
            ..Default::default()
        };
        records::to_view(
            active
                .update(transaction.connection())
                .await
                .context("failed to update comment")?,
        )
    }

    pub(crate) async fn delete_in(&self, transaction: &Transaction, id: i64) -> Result<()> {
        comment::Entity::delete_by_id(id)
            .exec(transaction.connection())
            .await
            .context("failed to delete comment")?;
        Ok(())
    }
}
