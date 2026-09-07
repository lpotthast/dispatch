mod candidates;
use super::model::{ClaimCandidate, ClaimRecord, ClaimSelector};
use crate::backend::{
    entities::work_item::{self, WorkItem, WorkItemActiveModel},
    items::events::model::agent_event_attribution,
    items::events::repository as work_item_events,
    items::labels::repository::records as work_item_labels,
    items::repository::{records, views},
    storage::Transaction,
    storage::utc_now,
};
use dispatch_types::{AuthorType, CommentView, WorkItemView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
    Statement,
};
pub(crate) struct ClaimRepository;
impl ClaimRepository {
    pub(super) async fn next_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        selector: &ClaimSelector,
        previous: Option<&ClaimCandidate>,
    ) -> Result<Option<ClaimCandidate>> {
        candidates::next_in(transaction, project_id, selector, previous).await
    }
    pub(super) async fn specific_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        selector: Option<&ClaimSelector>,
    ) -> Result<Option<ClaimCandidate>> {
        let item = records::get(transaction.connection(), project_id, item_id).await?;
        if item.claimed_by.is_some() || item.finished_at.is_some() {
            return Ok(None);
        }
        let labels =
            work_item_labels::for_item(transaction.connection(), project_id, item_id).await?;
        if selector.is_some_and(|selector| !selector.matches(&labels)) {
            return Ok(None);
        }
        Ok(Some(ClaimCandidate {
            item_id,
            observed_version: item.version,
            updated_at: item.updated_at,
            source_state: crate::backend::items::labels::workflow::source_state_for_new_claim(
                &labels,
            ),
        }))
    }
    pub(super) async fn claim_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        candidate: &ClaimCandidate,
        agent_id: &str,
        expected_version: Option<i64>,
    ) -> Result<bool> {
        claim_candidate_in_tx(
            transaction.connection(),
            project_id,
            candidate.item_id,
            agent_id,
            &candidate.source_state,
            expected_version,
        )
        .await
    }
    pub(super) async fn save_in(
        &self,
        transaction: &Transaction,
        item: &WorkItemView,
    ) -> Result<()> {
        WorkItemActiveModel {
            id: Set(item.id),
            claimed_by: Set(item.claimed_by.clone()),
            claimed_at: Set(item.claimed_at.clone()),
            claim_expires_at: Set(item.claim_expires_at.clone()),
            finished_at: Set(item.finished_at.clone()),
            version: Set(item.version),
            updated_at: Set(item.updated_at.clone()),
            ..Default::default()
        }
        .update(transaction.connection())
        .await
        .context("failed to update claimed item")?;
        Ok(())
    }
    pub(super) async fn claimed_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<WorkItemView>> {
        let rows = WorkItem::find()
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::ClaimedBy.is_not_null())
            .all(transaction.connection())
            .await
            .context("failed to list claimed work items")?;
        views::models_to_views(transaction.connection(), project_id, rows).await
    }
    pub(super) async fn record_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
        agent_id: &str,
        record: ClaimRecord<'_>,
    ) -> Result<Option<CommentView>> {
        let conn = transaction.connection();
        if let Some(labels) = record.labels {
            crate::backend::items::labels::repository::workflow::apply_plan(
                conn, project_id, item_id, labels,
            )
            .await?;
        }
        let comment = if let Some((author, body)) = record.comment {
            Some(crate::backend::comments::repository::records::to_view(
                crate::backend::comments::repository::records::insert_in_tx(
                    conn,
                    item_id,
                    author,
                    (author == AuthorType::Agent).then(|| agent_id.to_owned()),
                    body,
                )
                .await?,
            )?)
        } else {
            None
        };
        work_item_events::record_event_with_attribution_in_tx(
            conn,
            project_id,
            Some(item_id),
            record.event_type,
            record.body,
            agent_event_attribution(agent_id),
        )
        .await?;
        Ok(comment)
    }
}
async fn claim_candidate_in_tx<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
    agent_id: &str,
    _source_state: &str,
    expected_version: Option<i64>,
) -> Result<bool>
where
    C: ConnectionTrait,
{
    let now = utc_now();
    let sql = r#"
        UPDATE work_items
        SET claimed_by = ?,
            claimed_at = ?,
            claim_expires_at = NULL,
            version = version + 1,
            updated_at = ?
        WHERE id = ?
          AND project_id = ?
          AND claimed_by IS NULL
          AND finished_at IS NULL
          AND (? IS NULL OR version = ?)
        RETURNING id
        "#;

    let claimed_id = conn
        .query_one(bound_statement(
            conn.get_database_backend(),
            sql,
            vec![
                agent_id.to_owned().into(),
                now.clone().into(),
                now.into(),
                item_id.into(),
                project_id.into(),
                expected_version.into(),
                expected_version.into(),
            ],
        ))
        .await
        .context("failed to claim work item")?
        .map(|row| row.try_get::<i64>("", "id"))
        .transpose()
        .context("failed to read claimed item id")?;

    Ok(claimed_id.is_some())
}
fn bound_statement(
    backend: sea_orm::DbBackend,
    sql: &str,
    values: Vec<sea_orm::Value>,
) -> Statement {
    let sql = if backend == sea_orm::DbBackend::Postgres {
        let mut index = 0;
        sql.chars()
            .map(|character| {
                if character == '?' {
                    index += 1;
                    format!("${index}")
                } else {
                    character.to_string()
                }
            })
            .collect()
    } else {
        sql.to_owned()
    };
    Statement::from_sql_and_values(backend, sql, values)
}
