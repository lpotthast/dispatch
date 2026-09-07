mod mutations;
use crate::backend::{
    entities::{
        work_item::{self, WorkItem},
        work_item_group::{self, WorkItemGroup},
    },
    storage::Transaction,
};
use dispatch_types::{WorkItemGroupSummaryView, WorkItemGroupView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter, QueryOrder,
    QuerySelect,
};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct GroupRepository;
impl GroupRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<Vec<WorkItemGroupView>> {
        let groups = WorkItemGroup::find()
            .filter(work_item_group::Column::ProjectId.eq(project_id))
            .order_by_asc(work_item_group::Column::Name)
            .order_by_asc(work_item_group::Column::Id)
            .all(transaction.connection())
            .await
            .context("failed to list work groups")?;
        let counts = WorkItem::find()
            .select_only()
            .column(work_item::Column::WorkGroupId)
            .column_as(work_item::Column::Id.count(), "item_count")
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::WorkGroupId.is_not_null())
            .group_by(work_item::Column::WorkGroupId)
            .into_model::<WorkGroupItemCount>()
            .all(transaction.connection())
            .await
            .context("failed to count grouped work items")?
            .into_iter()
            .map(|count| {
                Ok((
                    count.work_group_id,
                    count
                        .item_count
                        .try_into()
                        .context("work-group item count cannot be negative")?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>>>()?;
        Ok(groups
            .into_iter()
            .map(|group| {
                let count = counts.get(&group.id).copied().unwrap_or(0);
                group_view(group, count)
            })
            .collect())
    }
}
#[derive(FromQueryResult)]
struct WorkGroupItemCount {
    work_group_id: i64,
    item_count: i64,
}

pub(crate) async fn summaries_for_items<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    group_ids: impl IntoIterator<Item = i64>,
) -> Result<BTreeMap<i64, WorkItemGroupSummaryView>> {
    let group_ids = group_ids.into_iter().collect::<BTreeSet<_>>();
    if group_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    Ok(WorkItemGroup::find()
        .filter(work_item_group::Column::ProjectId.eq(project_id))
        .filter(work_item_group::Column::Id.is_in(group_ids))
        .all(conn)
        .await
        .context("failed to load work groups for items")?
        .into_iter()
        .map(|group| {
            (
                group.id,
                WorkItemGroupSummaryView {
                    id: group.id,
                    key: group.group_key,
                    name: group.name,
                },
            )
        })
        .collect())
}

fn group_view(group: work_item_group::Model, item_count: u64) -> WorkItemGroupView {
    WorkItemGroupView {
        id: group.id,
        project_id: group.project_id,
        key: group.group_key,
        name: group.name,
        item_count,
        actor_id: group.actor_id,
        agent_run_id: group.agent_run_id,
        created_at: group.created_at,
        updated_at: group.updated_at,
    }
}
