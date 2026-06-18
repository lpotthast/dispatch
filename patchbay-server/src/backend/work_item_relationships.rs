use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, Condition, ConnectionTrait, EntityTrait,
    QueryFilter, QueryOrder,
};

use crate::backend::{
    entities::work_item_relationship::{
        self, WorkItemRelationship, WorkItemRelationshipActiveModel, WorkItemRelationshipModel,
    },
    storage::utc_now,
};

pub(crate) async fn for_item<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
) -> Result<Vec<WorkItemRelationshipModel>>
where
    C: ConnectionTrait,
{
    Ok(WorkItemRelationship::find()
        .filter(work_item_relationship::Column::ProjectId.eq(project_id))
        .filter(
            Condition::any()
                .add(work_item_relationship::Column::SourceWorkItemId.eq(item_id))
                .add(work_item_relationship::Column::TargetWorkItemId.eq(item_id)),
        )
        .order_by_desc(work_item_relationship::Column::UpdatedAt)
        .order_by_desc(work_item_relationship::Column::Id)
        .all(conn)
        .await
        .context("failed to list item relationships")?)
}

pub(crate) async fn related_item_ids_for_item<C>(
    conn: &C,
    project_id: i64,
    item_id: i64,
) -> Result<Vec<i64>>
where
    C: ConnectionTrait,
{
    let relationships = for_item(conn, project_id, item_id).await?;
    let mut item_ids = relationships
        .into_iter()
        .flat_map(|relationship| {
            [
                relationship.source_work_item_id,
                relationship.target_work_item_id,
            ]
        })
        .filter(|related_id| *related_id != item_id)
        .collect::<Vec<_>>();
    item_ids.sort_unstable();
    item_ids.dedup();
    Ok(item_ids)
}

pub(crate) async fn get<C>(
    conn: &C,
    project_id: i64,
    relationship_id: i64,
) -> Result<WorkItemRelationshipModel>
where
    C: ConnectionTrait,
{
    WorkItemRelationship::find_by_id(relationship_id)
        .filter(work_item_relationship::Column::ProjectId.eq(project_id))
        .one(conn)
        .await
        .context_with(|| format!("failed to load relationship {relationship_id}"))?
        .ok_or_else(|| report!("relationship {relationship_id} does not exist in this project"))
}

pub(crate) async fn exact_relationship_exists<C>(
    conn: &C,
    project_id: i64,
    source_work_item_id: i64,
    target_work_item_id: i64,
    kind: &str,
    except_relationship_id: Option<i64>,
) -> Result<bool>
where
    C: ConnectionTrait,
{
    let mut query = WorkItemRelationship::find()
        .filter(work_item_relationship::Column::ProjectId.eq(project_id))
        .filter(work_item_relationship::Column::SourceWorkItemId.eq(source_work_item_id))
        .filter(work_item_relationship::Column::TargetWorkItemId.eq(target_work_item_id))
        .filter(work_item_relationship::Column::Kind.eq(kind));
    if let Some(relationship_id) = except_relationship_id {
        query = query.filter(work_item_relationship::Column::Id.ne(relationship_id));
    }

    Ok(query
        .one(conn)
        .await
        .context("failed to check duplicate relationship")?
        .is_some())
}

pub(crate) async fn insert_in_tx<C>(
    conn: &C,
    project_id: i64,
    source_work_item_id: i64,
    target_work_item_id: i64,
    kind: &str,
) -> Result<WorkItemRelationshipModel>
where
    C: ConnectionTrait,
{
    let now = utc_now();
    let active = WorkItemRelationshipActiveModel {
        project_id: Set(project_id),
        source_work_item_id: Set(source_work_item_id),
        target_work_item_id: Set(target_work_item_id),
        kind: Set(kind.to_owned()),
        created_at: Set(now.clone()),
        updated_at: Set(now),
        ..Default::default()
    };
    Ok(active
        .insert(conn)
        .await
        .context("failed to create work item relationship")?)
}

pub(crate) async fn update_kind_in_tx<C>(
    conn: &C,
    relationship: WorkItemRelationshipModel,
    kind: &str,
) -> Result<WorkItemRelationshipModel>
where
    C: ConnectionTrait,
{
    let mut active: WorkItemRelationshipActiveModel = relationship.into();
    active.kind = Set(kind.to_owned());
    active.updated_at = Set(utc_now());
    Ok(active
        .update(conn)
        .await
        .context("failed to update work item relationship")?)
}

pub(crate) async fn delete_by_id_in_tx<C>(conn: &C, relationship_id: i64) -> Result<()>
where
    C: ConnectionTrait,
{
    WorkItemRelationship::delete_by_id(relationship_id)
        .exec(conn)
        .await
        .context_with(|| format!("failed to delete relationship {relationship_id}"))?;
    Ok(())
}
