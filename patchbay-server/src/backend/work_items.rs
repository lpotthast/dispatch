use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter,
};

use crate::backend::{
    entities::work_item::{self, WorkItem, WorkItemActiveModel, WorkItemModel},
    storage::utc_now,
};

pub(crate) async fn get<C>(conn: &C, project_id: i64, item_id: i64) -> Result<WorkItemModel>
where
    C: ConnectionTrait,
{
    WorkItem::find_by_id(item_id)
        .filter(work_item::Column::ProjectId.eq(project_id))
        .one(conn)
        .await
        .context_with(|| format!("failed to load item {item_id}"))?
        .ok_or_else(|| report!("item {item_id} does not exist in this project"))
}

pub(crate) async fn touch<C>(conn: &C, item: WorkItemModel) -> Result<WorkItemModel>
where
    C: ConnectionTrait,
{
    let active = touch_active_model(item, utc_now());
    Ok(active
        .update(conn)
        .await
        .context("failed to update item version")?)
}

pub(crate) fn touch_active_model(item: WorkItemModel, updated_at: String) -> WorkItemActiveModel {
    let version = item.version;
    let mut active: WorkItemActiveModel = item.into();
    active.version = Set(version + 1);
    active.updated_at = Set(updated_at);
    active
}

pub(crate) fn clear_claim_active_model(
    item: WorkItemModel,
    updated_at: String,
) -> WorkItemActiveModel {
    let version = item.version;
    let mut active: WorkItemActiveModel = item.into();
    active.claimed_by = Set(None);
    active.claimed_at = Set(None);
    active.claim_expires_at = Set(None);
    active.version = Set(version + 1);
    active.updated_at = Set(updated_at);
    active
}

pub(crate) fn check_expected_version(expected: Option<i64>, actual: i64) -> Result<()> {
    if let Some(expected) = expected
        && expected != actual
    {
        bail!("version conflict: expected {expected}, found {actual}");
    }
    Ok(())
}
