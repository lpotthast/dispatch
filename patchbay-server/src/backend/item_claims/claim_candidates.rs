use std::collections::BTreeMap;

use crudkit_core::condition::Condition;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, Condition as SeaCondition, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect,
};

use crate::{
    backend::{
        entities::work_item::{self, WorkItem, WorkItemModel},
        label_conditions, work_item_labels, workflow_labels,
    },
    shared::view_models::WorkItemLabelView,
};

#[cfg(not(test))]
const CLAIM_SCAN_BATCH_SIZE: u64 = 64;
#[cfg(test)]
const CLAIM_SCAN_BATCH_SIZE: u64 = 2;

pub(super) struct ClaimCandidate {
    pub(super) item_id: i64,
    pub(super) source_state: String,
}

pub(super) enum ClaimSelector {
    State(String),
    AutomationCondition(label_conditions::ValidatedLabelCondition),
}

impl ClaimSelector {
    pub(super) fn state(state: impl Into<String>) -> Result<Self> {
        Ok(Self::State(workflow_labels::normalize_state_value(state)?))
    }

    pub(super) fn automation_condition(condition: &Condition) -> Result<Self> {
        Ok(Self::AutomationCondition(
            label_conditions::ValidatedLabelCondition::new(condition)?,
        ))
    }

    fn matches(&self, labels: &[WorkItemLabelView]) -> bool {
        match self {
            Self::State(state) => {
                !workflow_labels::is_automation_blocked(labels)
                    && workflow_labels::current_state(labels).as_deref() == Some(state.as_str())
            }
            Self::AutomationCondition(selector) => selector.matches_automation_selector(labels),
        }
    }
}

pub(super) struct ClaimCandidateScanner<'a, C> {
    conn: &'a C,
    project_id: i64,
    selector: &'a ClaimSelector,
    cursor: Option<ClaimScanCursor>,
}

impl<'a, C> ClaimCandidateScanner<'a, C>
where
    C: ConnectionTrait,
{
    pub(super) fn new(conn: &'a C, project_id: i64, selector: &'a ClaimSelector) -> Self {
        Self {
            conn,
            project_id,
            selector,
            cursor: None,
        }
    }

    pub(super) async fn next_matching_candidate(&mut self) -> Result<Option<ClaimCandidate>> {
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
                    item_id: candidate.id,
                    source_state: workflow_labels::source_state_for_new_claim(labels),
                }));
            }

            self.cursor = Some(next_batch_cursor);
        }
    }
}

pub(super) async fn has_matching_candidate<C>(
    conn: &C,
    project_id: i64,
    selector: &ClaimSelector,
) -> Result<bool>
where
    C: ConnectionTrait,
{
    let mut scanner = ClaimCandidateScanner::new(conn, project_id, selector);
    Ok(scanner.next_matching_candidate().await?.is_some())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, FEEDBACK_REQUESTED_LABEL_KEY, STATE_LABEL_KEY,
    };

    fn label(key: &str, value: Option<&str>) -> WorkItemLabelView {
        WorkItemLabelView {
            id: 1,
            project_id: 1,
            work_item_id: 1,
            key: key.to_owned(),
            value: value.map(ToOwned::to_owned),
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn state_selector_matches_current_state_and_skips_workflow_blockers() {
        let selector = ClaimSelector::state(" open ").unwrap();

        assert!(selector.matches(&[label(STATE_LABEL_KEY, Some("open"))]));
        assert!(!selector.matches(&[label(STATE_LABEL_KEY, Some("idea"))]));
        assert!(!selector.matches(&[
            label(STATE_LABEL_KEY, Some("open")),
            label(AUTOMATION_BLOCKED_LABEL_KEY, None),
        ]));
        assert!(!selector.matches(&[
            label(STATE_LABEL_KEY, Some("open")),
            label(FEEDBACK_REQUESTED_LABEL_KEY, None),
        ]));
    }
}
