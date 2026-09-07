use super::super::model::{ClaimCandidate, ClaimSelector};
use std::collections::BTreeMap;

use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, Condition as SeaCondition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::{
    backend::{
        entities::work_item::{self, WorkItem, WorkItemModel},
        items::labels::repository::records as work_item_labels,
        items::labels::workflow as workflow_labels,
    },
    shared::view_models::WorkItemLabelView,
};

#[cfg(not(test))]
const CLAIM_SCAN_BATCH_SIZE: u64 = 64;
#[cfg(test)]
const CLAIM_SCAN_BATCH_SIZE: u64 = 2;

pub(crate) struct ClaimCandidateScanner<'a, C> {
    conn: &'a C,
    project_id: i64,
    selector: &'a ClaimSelector,
    cursor: Option<ClaimScanCursor>,
}

impl<'a, C> ClaimCandidateScanner<'a, C>
where
    C: ConnectionTrait,
{
    pub(crate) fn new(conn: &'a C, project_id: i64, selector: &'a ClaimSelector) -> Self {
        Self {
            conn,
            project_id,
            selector,
            cursor: None,
        }
    }

    pub(crate) async fn next_matching_candidate(&mut self) -> Result<Option<ClaimCandidate>> {
        loop {
            let candidates =
                claimable_items_after_cursor(self.conn, self.project_id, self.cursor.as_ref())
                    .await?;
            let Some(last_candidate) = candidates.last() else {
                return Ok(None);
            };
            let next_batch_cursor = ClaimScanCursor::from(last_candidate);
            let labels_by_item =
                labels_for_candidate_items(self.conn, self.project_id, &candidates).await?;

            for candidate in candidates {
                let labels = labels_for_item(&labels_by_item, candidate.id);
                if !self.selector.matches(labels) {
                    continue;
                }

                self.cursor = Some(ClaimScanCursor::from(&candidate));
                return Ok(Some(ClaimCandidate {
                    updated_at: candidate.updated_at.clone(),
                    item_id: candidate.id,
                    observed_version: candidate.version,
                    source_state: workflow_labels::source_state_for_new_claim(labels),
                }));
            }

            self.cursor = Some(next_batch_cursor);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClaimScanCursor {
    updated_at: String,
    item_id: i64,
}

impl From<&WorkItemModel> for ClaimScanCursor {
    fn from(item: &WorkItemModel) -> Self {
        Self {
            updated_at: item.updated_at.clone(),
            item_id: item.id,
        }
    }
}

async fn claimable_items_after_cursor<C>(
    conn: &C,
    project_id: i64,
    cursor: Option<&ClaimScanCursor>,
) -> Result<Vec<WorkItemModel>>
where
    C: ConnectionTrait,
{
    let mut query = WorkItem::find()
        .filter(work_item::Column::ProjectId.eq(project_id))
        .filter(work_item::Column::ClaimedBy.is_null())
        .filter(work_item::Column::FinishedAt.is_null())
        .order_by_asc(work_item::Column::UpdatedAt)
        .order_by_asc(work_item::Column::Id)
        .limit(CLAIM_SCAN_BATCH_SIZE);

    if let Some(cursor) = cursor {
        query = query.filter(
            SeaCondition::any()
                .add(work_item::Column::UpdatedAt.gt(cursor.updated_at.clone()))
                .add(
                    SeaCondition::all()
                        .add(work_item::Column::UpdatedAt.eq(cursor.updated_at.clone()))
                        .add(work_item::Column::Id.gt(cursor.item_id)),
                ),
        );
    }

    Ok(query
        .all(conn)
        .await
        .context("failed to list claimable work items")?)
}

async fn labels_for_candidate_items<C>(
    conn: &C,
    project_id: i64,
    items: &[WorkItemModel],
) -> Result<BTreeMap<i64, Vec<WorkItemLabelView>>>
where
    C: ConnectionTrait,
{
    if items.is_empty() {
        return Ok(BTreeMap::new());
    }

    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    work_item_labels::for_items(conn, project_id, &item_ids).await
}

fn labels_for_item(
    labels_by_item: &BTreeMap<i64, Vec<WorkItemLabelView>>,
    item_id: i64,
) -> &[WorkItemLabelView] {
    labels_by_item
        .get(&item_id)
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

pub(super) async fn next_in(
    transaction: &crate::backend::storage::Transaction,
    project_id: i64,
    selector: &ClaimSelector,
    previous: Option<&ClaimCandidate>,
) -> Result<Option<ClaimCandidate>> {
    let mut scanner = ClaimCandidateScanner::new(transaction.connection(), project_id, selector);
    scanner.cursor = previous.map(|candidate| ClaimScanCursor {
        updated_at: candidate.updated_at.clone(),
        item_id: candidate.item_id,
    });
    scanner.next_matching_candidate().await
}
