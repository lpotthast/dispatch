use super::super::model::GroupAssignmentItem;
use super::{GroupRepository, group_view};
use crate::backend::{
    entities::{
        work_item::{self, WorkItem},
        work_item_group::{self, WorkItemGroup, WorkItemGroupActiveModel},
    },
    items::events::{model::EventAttribution, repository as work_item_events},
    storage::{Transaction, utc_now},
};
use dispatch_types::{WorkItemEventType, WorkItemGroupView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter,
    sea_query::Expr,
};
use std::collections::BTreeSet;
impl GroupRepository {
    pub(crate) async fn count_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        group_id: i64,
    ) -> Result<u64> {
        Ok(WorkItem::find()
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::WorkGroupId.eq(group_id))
            .count(transaction.connection())
            .await
            .context("failed to count work-group items")?)
    }
    pub(crate) async fn find_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        key: &str,
    ) -> Result<Option<WorkItemGroupView>> {
        let model = WorkItemGroup::find()
            .filter(work_item_group::Column::ProjectId.eq(project_id))
            .filter(work_item_group::Column::GroupKey.eq(key))
            .one(transaction.connection())
            .await
            .context("failed to look up work group")?;
        match model {
            Some(group) => {
                let count = self.count_in(transaction, project_id, group.id).await?;
                Ok(Some(group_view(group, count)))
            }
            None => Ok(None),
        }
    }
    pub(crate) async fn create_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        key: String,
        name: String,
        attribution: EventAttribution<'_>,
    ) -> Result<WorkItemGroupView> {
        let now = utc_now();
        let group = WorkItemGroupActiveModel {
            project_id: Set(project_id),
            group_key: Set(key),
            name: Set(name),
            actor_id: Set(attribution.actor_id.map(ToOwned::to_owned)),
            agent_run_id: Set(attribution.agent_run_id),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(transaction.connection())
        .await
        .context("failed to create work group")?;
        Ok(group_view(group, 0))
    }
    pub(crate) async fn items_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        ids: BTreeSet<i64>,
    ) -> Result<Vec<GroupAssignmentItem>> {
        crate::backend::items::repository::records::get_many(
            transaction.connection(),
            project_id,
            ids,
        )
        .await
        .map(|items| {
            items
                .into_values()
                .map(|item| GroupAssignmentItem {
                    id: item.id,
                    group_id: item.work_group_id,
                })
                .collect()
        })
    }
    pub(crate) async fn assign_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        group_id: i64,
        group_key: &str,
        ids: &[i64],
        attribution: EventAttribution<'_>,
    ) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        WorkItem::update_many()
            .col_expr(work_item::Column::WorkGroupId, Expr::value(group_id))
            .col_expr(
                work_item::Column::Version,
                Expr::col(work_item::Column::Version).add(1),
            )
            .col_expr(work_item::Column::UpdatedAt, Expr::value(utc_now()))
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::Id.is_in(ids.iter().copied()))
            .exec(transaction.connection())
            .await
            .context("failed to assign work items to group")?;
        for id in ids {
            work_item_events::record_event_with_attribution_in_tx(
                transaction.connection(),
                project_id,
                Some(*id),
                WorkItemEventType::ItemUpdated,
                &format!("Assigned to work group {group_key}"),
                attribution,
            )
            .await?;
        }
        Ok(())
    }
}
