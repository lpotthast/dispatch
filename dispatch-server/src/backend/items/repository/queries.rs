use super::ItemRepository;
use crate::backend::{
    entities::work_item::{self, WorkItem},
    items::labels::repository::records as work_item_labels,
    items::labels::workflow as workflow_labels,
    storage::Transaction,
};
use dispatch_types::{BoardWorkItemView, STATE_LABEL_KEY, WorkItemView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter, QueryOrder, Statement,
};
const BOARD_DESCRIPTION_EXCERPT_CHARS: i64 = 1_024;
impl ItemRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        state: Option<String>,
    ) -> Result<Vec<WorkItemView>> {
        crate::backend::metrics::time_repository("work_items.list", async {
            let item_ids = match state {
                Some(state) => {
                    let state = workflow_labels::normalize_state_value(state)?;
                    let ids = work_item_labels::item_ids_with_state(
                        transaction.connection(),
                        project_id,
                        &state,
                    )
                    .await?;
                    if ids.is_empty() {
                        return Ok(Vec::new());
                    }
                    Some(ids)
                }
                None => None,
            };
            let mut query = WorkItem::find()
                .filter(work_item::Column::ProjectId.eq(project_id))
                .order_by_desc(work_item::Column::UpdatedAt)
                .order_by_desc(work_item::Column::Id);

            if let Some(item_ids) = item_ids {
                query = query.filter(work_item::Column::Id.is_in(item_ids));
            }

            let items = query
                .all(transaction.connection())
                .await
                .context("failed to list work items")?;
            crate::backend::items::repository::views::models_to_views(
                transaction.connection(),
                project_id,
                items,
            )
            .await
        })
        .await
    }

    pub(crate) async fn board_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<BoardWorkItemView>> {
        crate::backend::metrics::time_repository("work_items.list_board", async {
            let items =
                crate::backend::items::repository::views::BoardWorkItemModel::find_by_statement(
                    Statement::from_sql_and_values(
                        sea_orm::DbBackend::Sqlite,
                        r#"
                SELECT
                    wi.id,
                    wi.work_group_id,
                    wi.title,
                    substr(wi.description, 1, ?2) AS description_excerpt,
                    wi.claimed_by,
                    wi.claimed_at,
                    wi.created_at,
                    wi.updated_at,
                    (
                        SELECT COUNT(*)
                        FROM comments AS comment
                        WHERE comment.work_item_id = wi.id
                    ) AS comment_count
                FROM work_items AS wi
                WHERE wi.project_id = ?1
                ORDER BY wi.updated_at DESC, wi.id DESC
                "#,
                        vec![project_id.into(), BOARD_DESCRIPTION_EXCERPT_CHARS.into()],
                    ),
                )
                .all(transaction.connection())
                .await
                .context("failed to list compact Board work items")?;
            crate::backend::items::repository::views::board_models_to_views(
                transaction.connection(),
                project_id,
                items,
            )
            .await
        })
        .await
    }

    pub(crate) async fn count_outside_states_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<i64> {
        let row = transaction
            .connection()
            .query_one(Statement::from_sql_and_values(
                sea_orm::DbBackend::Sqlite,
                r#"
            SELECT COUNT(*) AS count
            FROM work_items AS wi
            WHERE wi.project_id = ?1
              AND NOT EXISTS (
                  SELECT 1
                  FROM work_item_labels AS wil
                  JOIN work_item_states AS wis
                    ON wis.project_id = wi.project_id
                   AND wis.identifier = wil.label_value
                  WHERE wil.project_id = wi.project_id
                    AND wil.work_item_id = wi.id
                    AND wil.label_key = ?2
              )
            "#,
                vec![project_id.into(), STATE_LABEL_KEY.to_owned().into()],
            ))
            .await
            .context("failed to count work items outside authored states")?;

        row.map(|row| row.try_get::<i64>("", "count"))
            .transpose()
            .context("failed to read work items outside authored states count")?
            .ok_or_else(|| report!("missing work items outside authored states count"))
    }
}
