use std::collections::BTreeMap;

use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter};

use crate::{
    backend::{
        entities::{
            agent_run::{self, AgentRun, AgentRunModel},
            work_item::WorkItemModel,
            work_item_origin::{self, WorkItemOrigin},
        },
        execution::identity as agent_ids,
        items::labels::repository::records as work_item_labels,
        items::labels::workflow as workflow_labels,
        projects,
    },
    shared::view_models::{
        AgentReasoningEffort, BoardWorkItemView, WorkItemClaimSourceView, WorkItemGroupSummaryView,
        WorkItemLabelView, WorkItemOriginKind, WorkItemOriginView, WorkItemView,
    },
};

#[derive(Debug, FromQueryResult)]
pub(crate) struct BoardWorkItemModel {
    id: i64,
    work_group_id: Option<i64>,
    title: String,
    description_excerpt: String,
    claimed_by: Option<String>,
    claimed_at: Option<String>,
    created_at: String,
    updated_at: String,
    comment_count: i64,
}

pub(crate) async fn models_to_views<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    items: Vec<WorkItemModel>,
) -> Result<Vec<WorkItemView>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let group_ids = items
        .iter()
        .filter_map(|item| item.work_group_id)
        .collect::<Vec<_>>();
    let (mut labels, mut comment_counts, mut claim_sources, mut origins, groups) =
        crate::backend::metrics::time_repository("work_items.enrich", async {
            tokio::try_join!(
                work_item_labels::for_items(conn, project_id, &item_ids),
                crate::backend::comments::repository::records::counts_for_items(conn, &item_ids),
                claim_sources_for_items(conn, project_id, &items),
                origins_for_items(conn, project_id, &item_ids),
                crate::backend::items::groups::repository::summaries_for_items(
                    conn, project_id, group_ids
                ),
            )
        })
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

pub(crate) async fn board_models_to_views<C: ConnectionTrait>(
    conn: &C,
    project_id: i64,
    items: Vec<BoardWorkItemModel>,
) -> Result<Vec<BoardWorkItemView>> {
    if items.is_empty() {
        return Ok(Vec::new());
    }

    let item_ids = items.iter().map(|item| item.id).collect::<Vec<_>>();
    let group_ids = items
        .iter()
        .filter_map(|item| item.work_group_id)
        .collect::<Vec<_>>();
    let run_to_item = claimed_run_to_item(
        items
            .iter()
            .map(|item| (item.id, item.claimed_by.as_deref())),
    );
    let (mut labels, mut claim_sources, groups) =
        crate::backend::metrics::time_repository("work_items.enrich_board", async {
            tokio::try_join!(
                work_item_labels::for_items(conn, project_id, &item_ids),
                claim_sources_for_run_to_item(conn, project_id, run_to_item),
                crate::backend::items::groups::repository::summaries_for_items(
                    conn, project_id, group_ids
                ),
            )
        })
        .await?;

    Ok(items
        .into_iter()
        .map(|item| BoardWorkItemView {
            id: item.id,
            title: item.title,
            description_excerpt: item.description_excerpt,
            labels: labels.remove(&item.id).unwrap_or_default(),
            claimed_by: item.claimed_by,
            claimed_at: item.claimed_at,
            claim_source: claim_sources.remove(&item.id),
            created_at: item.created_at,
            updated_at: item.updated_at,
            comment_count: item.comment_count,
            work_group: item.work_group_id.and_then(|id| groups.get(&id).cloned()),
        })
        .collect())
}

pub(crate) async fn model_to_view<C: ConnectionTrait>(
    conn: &C,
    item: WorkItemModel,
) -> Result<WorkItemView> {
    let project_id = item.project_id;
    models_to_views(conn, project_id, vec![item])
        .await?
        .pop()
        .ok_or_else(|| report!("failed to build work item view"))
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
    let run_to_item = claimed_run_to_item(
        items
            .iter()
            .map(|item| (item.id, item.claimed_by.as_deref())),
    );
    claim_sources_for_run_to_item(conn, project_id, run_to_item).await
}

fn claimed_run_to_item<'a>(
    items: impl IntoIterator<Item = (i64, Option<&'a str>)>,
) -> BTreeMap<i64, i64> {
    items
        .into_iter()
        .filter_map(|(item_id, claimed_by)| {
            let run_id = agent_ids::parse_dispatch_run_agent_id(claimed_by?)?;
            Some((run_id, item_id))
        })
        .collect()
}

async fn claim_sources_for_run_to_item<C>(
    conn: &C,
    project_id: i64,
    run_to_item: BTreeMap<i64, i64>,
) -> Result<BTreeMap<i64, WorkItemClaimSourceView>>
where
    C: ConnectionTrait,
{
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
