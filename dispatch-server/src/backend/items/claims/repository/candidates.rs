use super::super::model::{ClaimCandidate, ClaimSelector};
use crate::backend::{
    entities::work_item::{self, WorkItem},
    items::labels::{
        repository::{conditions, records as work_item_labels},
        workflow as workflow_labels,
    },
    storage::Transaction,
};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, Condition, EntityTrait, FromQueryResult, QueryFilter, QueryOrder, QuerySelect,
    Select,
};

#[derive(FromQueryResult)]
struct CandidateRow {
    id: i64,
    version: i64,
    updated_at: String,
}

fn matching_items(project_id: i64, selector: &ClaimSelector) -> Select<WorkItem> {
    let condition = match selector {
        ClaimSelector::State(state) => Condition::all()
            .add(conditions::items_in_states(project_id, [state.as_str()]))
            .add(conditions::unblocked_items(project_id)),
        ClaimSelector::AutomationCondition(condition) => {
            conditions::matching_automation_items(project_id, condition)
        }
    };
    WorkItem::find()
        .filter(work_item::Column::ProjectId.eq(project_id))
        .filter(work_item::Column::ClaimedBy.is_null())
        .filter(work_item::Column::FinishedAt.is_null())
        .filter(condition)
        .order_by_asc(work_item::Column::UpdatedAt)
        .order_by_asc(work_item::Column::Id)
}

pub(super) async fn matching_ids_in(
    transaction: &Transaction,
    project_id: i64,
    selector: &ClaimSelector,
) -> Result<Vec<i64>> {
    Ok(matching_items(project_id, selector)
        .select_only()
        .column(work_item::Column::Id)
        .into_tuple()
        .all(transaction.connection())
        .await
        .context("failed to list matching claimable item ids")?)
}

pub(super) async fn next_in(
    transaction: &Transaction,
    project_id: i64,
    selector: &ClaimSelector,
    previous: Option<&ClaimCandidate>,
) -> Result<Option<ClaimCandidate>> {
    let mut query = matching_items(project_id, selector);
    if let Some(previous) = previous {
        query = query.filter(
            Condition::any()
                .add(work_item::Column::UpdatedAt.gt(previous.updated_at.clone()))
                .add(
                    Condition::all()
                        .add(work_item::Column::UpdatedAt.eq(previous.updated_at.clone()))
                        .add(work_item::Column::Id.gt(previous.item_id)),
                ),
        );
    }
    let Some(item) = query
        .select_only()
        .columns([
            work_item::Column::Id,
            work_item::Column::Version,
            work_item::Column::UpdatedAt,
        ])
        .into_model::<CandidateRow>()
        .one(transaction.connection())
        .await
        .context("failed to find next claimable work item")?
    else {
        return Ok(None);
    };
    let labels = work_item_labels::for_item(transaction.connection(), project_id, item.id).await?;
    Ok(Some(ClaimCandidate {
        item_id: item.id,
        observed_version: item.version,
        updated_at: item.updated_at,
        source_state: workflow_labels::source_state_for_new_claim(&labels),
    }))
}
