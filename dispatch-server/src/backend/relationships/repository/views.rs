use crate::backend::{
    entities::{
        work_item::{self, WorkItem},
        work_item_relationship::WorkItemRelationshipModel,
    },
    items::labels::repository::records as work_item_labels,
    items::labels::workflow as workflow_labels,
};
use crate::shared::view_models::{WorkItemRelationshipItemSummary, WorkItemRelationshipView};
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};
use std::collections::BTreeMap;
pub(super) async fn relationship_to_view<C>(
    conn: &C,
    relationship: WorkItemRelationshipModel,
) -> Result<WorkItemRelationshipView>
where
    C: ConnectionTrait,
{
    relationships_to_views(conn, relationship.project_id, &[relationship])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| report!("failed to build relationship view"))
}

pub(super) async fn relationships_to_views<C>(
    conn: &C,
    project_id: i64,
    relationships: &[WorkItemRelationshipModel],
) -> Result<Vec<WorkItemRelationshipView>>
where
    C: ConnectionTrait,
{
    if relationships.is_empty() {
        return Ok(Vec::new());
    }

    let summaries = relationship_item_summaries(conn, project_id, relationships).await?;

    relationships
        .iter()
        .map(|relationship| {
            let source = summaries
                .get(&relationship.source_work_item_id)
                .cloned()
                .ok_or_else(|| {
                    report!(
                        "relationship {} references missing source item {}",
                        relationship.id,
                        relationship.source_work_item_id
                    )
                })?;
            let target = summaries
                .get(&relationship.target_work_item_id)
                .cloned()
                .ok_or_else(|| {
                    report!(
                        "relationship {} references missing target item {}",
                        relationship.id,
                        relationship.target_work_item_id
                    )
                })?;
            Ok(WorkItemRelationshipView {
                id: relationship.id,
                project_id: relationship.project_id,
                kind: relationship.kind.clone(),
                source_work_item_id: relationship.source_work_item_id,
                target_work_item_id: relationship.target_work_item_id,
                source,
                target,
                created_at: relationship.created_at.clone(),
                updated_at: relationship.updated_at.clone(),
            })
        })
        .collect()
}

async fn relationship_item_summaries<C>(
    conn: &C,
    project_id: i64,
    relationships: &[WorkItemRelationshipModel],
) -> Result<BTreeMap<i64, WorkItemRelationshipItemSummary>>
where
    C: ConnectionTrait,
{
    let item_ids = relationship_item_ids(relationships);
    let items = WorkItem::find()
        .filter(work_item::Column::ProjectId.eq(project_id))
        .filter(work_item::Column::Id.is_in(item_ids.clone()))
        .all(conn)
        .await
        .context("failed to load relationship item summaries")?;
    let mut labels_by_item = work_item_labels::for_items(conn, project_id, &item_ids).await?;

    Ok(items
        .into_iter()
        .map(|item| {
            let labels = labels_by_item.remove(&item.id).unwrap_or_default();
            (
                item.id,
                WorkItemRelationshipItemSummary {
                    id: item.id,
                    title: item.title,
                    state: workflow_labels::current_state(&labels),
                    version: item.version,
                },
            )
        })
        .collect())
}

fn relationship_item_ids(relationships: &[WorkItemRelationshipModel]) -> Vec<i64> {
    let mut item_ids = relationships
        .iter()
        .flat_map(|relationship| {
            [
                relationship.source_work_item_id,
                relationship.target_work_item_id,
            ]
        })
        .collect::<Vec<_>>();
    item_ids.sort_unstable();
    item_ids.dedup();
    item_ids
}
