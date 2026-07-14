use std::collections::BTreeMap;

use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter};

use crate::{
    backend::{
        agent_ids,
        entities::{
            agent_run::{self, AgentRun, AgentRunModel},
            work_item::WorkItemModel,
            work_item_origin::{self, WorkItemOrigin},
        },
        projects, work_item_comments, work_item_groups, work_item_labels, workflow_labels,
    },
    shared::view_models::{
        AgentReasoningEffort, WorkItemClaimSourceView, WorkItemGroupSummaryView, WorkItemLabelView,
        WorkItemOriginKind, WorkItemOriginView, WorkItemView,
    },
};

use super::storage::Store;

pub(crate) async fn models_to_views(
    store: &Store,
    project_id: i64,
    items: Vec<WorkItemModel>,
) -> Result<Vec<WorkItemView>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let mut labels =
        work_item_labels::for_items(store.db().as_ref(), project_id, &item_ids).await?;
    let mut comment_counts =
        work_item_comments::counts_for_items(store.db().as_ref(), &item_ids).await?;
    let mut claim_sources =
        claim_sources_for_items(store.db().as_ref(), project_id, &items).await?;
    let mut origins = origins_for_items(store.db().as_ref(), project_id, &item_ids).await?;
    let groups = work_item_groups::summaries_for_items(
        store,
        project_id,
        items.iter().filter_map(|item| item.work_group_id),
    )
    .await?;

    let mut views = Vec::with_capacity(items.len());
    for item in items {
        let item_id = item.id;
        let work_group = item.work_group_id.and_then(|id| groups.get(&id).cloned());
        views.push(to_view(
            item,
            labels.remove(&item_id).unwrap_or_default(),
            comment_counts.remove(&item_id).unwrap_or(0),
            claim_sources.remove(&item_id),
            work_group,
            origins.remove(&item_id),
        )?);
    }
    Ok(views)
}

pub(crate) async fn model_to_view(store: &Store, item: WorkItemModel) -> Result<WorkItemView> {
    let work_group_id = item.work_group_id;
    let labels = work_item_labels::for_item(store.db().as_ref(), item.project_id, item.id).await?;
    let comment_count = work_item_comments::counts_for_items(store.db().as_ref(), &[item.id])
        .await?
        .remove(&item.id)
        .unwrap_or(0);
    let mut claim_sources = claim_sources_for_items(
        store.db().as_ref(),
        item.project_id,
        std::slice::from_ref(&item),
    )
    .await?;
    let claim_source = claim_sources.remove(&item.id);
    let origin = origins_for_items(store.db().as_ref(), item.project_id, &[item.id])
        .await?
        .remove(&item.id);
    let work_group = work_item_groups::summaries_for_items(store, item.project_id, work_group_id)
        .await?
        .remove(&work_group_id.unwrap_or_default());
    to_view(
        item,
        labels,
        comment_count,
        claim_source,
        work_group,
        origin,
    )
}

async fn origins_for_items<C>(
    conn: &C,
    project_id: i64,
    item_ids: &[i64],
) -> Result<BTreeMap<i64, WorkItemOriginView>>
where
    C: ConnectionTrait,
{
    if item_ids.is_empty() {
        return Ok(BTreeMap::new());
    }
    let origins = WorkItemOrigin::find()
        .filter(work_item_origin::Column::ProjectId.eq(project_id))
        .filter(work_item_origin::Column::WorkItemId.is_in(item_ids.iter().copied()))
        .all(conn)
        .await
        .context("failed to load work item origins")?;
    origins
        .into_iter()
        .map(|origin| {
            Ok((
                origin.work_item_id,
                WorkItemOriginView {
                    kind: origin.origin_kind.parse::<WorkItemOriginKind>()?,
                    actor_id: origin.actor_id,
                    agent_run_id: origin.agent_run_id,
                    producing_evaluation_id: origin.producing_evaluation_id,
                    trigger_id: origin.trigger_id,
                    trigger_revision_id: origin.trigger_revision_id,
                    trigger_name: origin.trigger_name,
                    bundle_key: origin.bundle_key,
                    created_at: origin.created_at,
                },
            ))
        })
        .collect()
}

async fn claim_sources_for_items<C>(
    conn: &C,
    project_id: i64,
    items: &[WorkItemModel],
) -> Result<BTreeMap<i64, WorkItemClaimSourceView>>
where
    C: ConnectionTrait,
{
    let run_to_item = items
        .iter()
        .filter_map(|item| {
            let run_id = agent_ids::parse_dispatch_run_agent_id(item.claimed_by.as_deref()?)?;
            Some((run_id, item.id))
        })
        .collect::<BTreeMap<_, _>>();
    if run_to_item.is_empty() {
        return Ok(BTreeMap::new());
    }

    let run_ids = run_to_item.keys().copied().collect::<Vec<_>>();
    let runs = AgentRun::find()
        .filter(agent_run::Column::ProjectId.eq(project_id))
        .filter(agent_run::Column::Id.is_in(run_ids))
        .all(conn)
        .await
        .context("failed to list claimed item agent runs")?;

    let mut claim_sources = BTreeMap::new();
    for run in runs {
        let Some(item_id) = run_to_item.get(&run.id).copied() else {
            continue;
        };
        if run.work_item_id != Some(item_id) {
            continue;
        }
        claim_sources.insert(item_id, claim_source_from_run(run));
    }

    Ok(claim_sources)
}

fn claim_source_from_run(run: AgentRunModel) -> WorkItemClaimSourceView {
    WorkItemClaimSourceView {
        run_id: run.id,
        trigger_id: run.trigger_id,
        trigger_name: projects::normalize_optional(run.trigger_name),
    }
}

fn to_view(
    item: WorkItemModel,
    labels: Vec<WorkItemLabelView>,
    comment_count: i64,
    claim_source: Option<WorkItemClaimSourceView>,
    work_group: Option<WorkItemGroupSummaryView>,
    origin: Option<WorkItemOriginView>,
) -> Result<WorkItemView> {
    let state = workflow_labels::current_state(&labels);

    Ok(WorkItemView {
        id: item.id,
        project_id: item.project_id,
        title: item.title,
        description: item.description,
        state,
        labels,
        version: item.version,
        claimed_by: item.claimed_by,
        claimed_at: item.claimed_at,
        claim_expires_at: item.claim_expires_at,
        claim_source,
        finished_at: item.finished_at,
        agent_model_override: projects::normalize_optional(item.agent_model_override),
        agent_reasoning_effort_override: item
            .agent_reasoning_effort_override
            .as_deref()
            .map(str::parse::<AgentReasoningEffort>)
            .transpose()?,
        created_at: item.created_at,
        updated_at: item.updated_at,
        comment_count,
        work_group,
        origin,
    })
}
