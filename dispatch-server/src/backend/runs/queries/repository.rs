use super::model::{ItemRunPreview, ItemRunPreviews, RunFilter};
use crate::backend::runs::repository::encoding::decode_in;
use crate::backend::{
    entities::{
        agent_run::{self, AgentRun},
        project::{self, Project},
        work_item::{self, WorkItem},
        work_item_event,
        work_item_origin::{self, WorkItemOrigin},
    },
    storage::Transaction,
};
use dispatch_types::{AgentRunStatus, AgentRunView, WorkItemSummaryView};
use rootcause::{Result, prelude::*};
use sea_orm::{
    ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, Statement,
};
use std::{
    collections::{BTreeSet, HashMap},
    str::FromStr,
};
const ITEM_RUN_PREVIEW_LIMIT: i64 = 3;
pub(crate) struct RunQueryRepository;
impl RunQueryRepository {
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        filter: RunFilter,
        limit: Option<u64>,
    ) -> Result<Vec<AgentRunView>> {
        let mut query = AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .order_by_desc(agent_run::Column::CreatedAt)
            .order_by_desc(agent_run::Column::Id);
        let context = match filter {
            RunFilter::Project => "failed to list agent runs",
            RunFilter::Item(id) => {
                query = query.filter(agent_run::Column::WorkItemId.eq(id));
                "failed to list item agent runs"
            }
            RunFilter::Trigger(id) => {
                query = query.filter(agent_run::Column::TriggerId.eq(id));
                "failed to list trigger agent runs"
            }
        };
        if let Some(limit) = limit {
            query = query.limit(limit);
        }
        let records = query.all(transaction.connection()).await.context(context)?;
        let mut views = Vec::with_capacity(records.len());
        for record in records {
            views.push(decode_in(transaction, record).await?);
        }
        Ok(views)
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<AgentRunView> {
        let record = AgentRun::find_by_id(run_id)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load agent run")?
            .ok_or_else(|| report!("agent run {run_id} does not exist in this project"))?;
        decode_in(transaction, record).await
    }
    pub(crate) async fn previews_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_ids: &[i64],
    ) -> Result<HashMap<i64, ItemRunPreviews>> {
        if item_ids.is_empty() {
            return Ok(HashMap::new());
        }

        crate::backend::metrics::time_repository("agent_runs.board_previews", async {
            let item_placeholders = (0..item_ids.len())
                .map(|index| format!("?{}", index + 2))
                .collect::<Vec<_>>()
                .join(", ");
            let preview_limit_parameter = item_ids.len() + 2;
            let mut values = Vec::<sea_orm::Value>::with_capacity(item_ids.len() + 2);
            values.push(project_id.into());
            values.extend(item_ids.iter().copied().map(Into::into));
            values.push(ITEM_RUN_PREVIEW_LIMIT.into());
            let rows = transaction
                .connection()
                .query_all(Statement::from_sql_and_values(
                    sea_orm::DbBackend::Sqlite,
                    format!(
                        r#"
                WITH run_counts AS (
                    SELECT work_item_id, COUNT(*) AS total
                    FROM agent_runs
                    WHERE project_id = ?1
                      AND work_item_id IN ({item_placeholders})
                    GROUP BY work_item_id
                ),
                ranked_runs AS (
                    SELECT
                        work_item_id,
                        id,
                        status,
                        result_summary,
                        created_at,
                        ROW_NUMBER() OVER (
                            PARTITION BY work_item_id
                            ORDER BY created_at DESC, id DESC
                        ) AS preview_rank
                    FROM agent_runs
                    WHERE project_id = ?1
                      AND work_item_id IN ({item_placeholders})
                )
                SELECT
                    ranked_runs.work_item_id,
                    ranked_runs.id,
                    ranked_runs.status,
                    ranked_runs.result_summary,
                    ranked_runs.created_at,
                    run_counts.total
                FROM ranked_runs
                INNER JOIN run_counts
                    ON run_counts.work_item_id = ranked_runs.work_item_id
                WHERE ranked_runs.preview_rank <= ?{preview_limit_parameter}
                ORDER BY ranked_runs.work_item_id, ranked_runs.preview_rank
                "#
                    ),
                    values,
                ))
                .await
                .context("failed to list Board item agent runs")?;

            let mut previews = HashMap::<i64, ItemRunPreviews>::new();
            for row in rows {
                let item_id = row
                    .try_get::<i64>("", "work_item_id")
                    .context("failed to read Board run preview work item id")?;
                let total = row
                    .try_get::<i64>("", "total")
                    .context("failed to read Board item run count")?;
                let total = usize::try_from(total).context("invalid Board item run count")?;
                let item = previews.entry(item_id).or_default();
                item.total = total;
                item.latest.push(ItemRunPreview {
                    id: row
                        .try_get::<i64>("", "id")
                        .context("failed to read Board run preview id")?,
                    status: AgentRunStatus::from_str(
                        &row.try_get::<String>("", "status")
                            .context("failed to read Board run preview status")?,
                    )?,
                    result_summary: row
                        .try_get::<String>("", "result_summary")
                        .context("failed to read Board run preview summary")?,
                    created_at: row
                        .try_get::<String>("", "created_at")
                        .context("failed to read Board run preview creation time")?,
                });
            }
            Ok(previews)
        })
        .await
    }

    pub(crate) async fn active_project_names_in(
        &self,
        transaction: &Transaction,
    ) -> Result<Vec<String>> {
        let project_ids = AgentRun::find()
            .select_only()
            .column(agent_run::Column::ProjectId)
            .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
            .into_tuple::<i64>()
            .all(transaction.connection())
            .await
            .context("failed to load projects with running agent runs")?
            .into_iter()
            .collect::<BTreeSet<_>>();
        if project_ids.is_empty() {
            return Ok(Vec::new());
        }

        Ok(Project::find()
            .select_only()
            .column(project::Column::Name)
            .filter(project::Column::Id.is_in(project_ids))
            .order_by_asc(project::Column::Name)
            .into_tuple::<String>()
            .all(transaction.connection())
            .await
            .context("failed to load active project names")?)
    }

    pub(crate) async fn item_summaries_in(
        &self,
        project_id: i64,
        transaction: &Transaction,
        run_id: i64,
        created: bool,
    ) -> Result<Vec<WorkItemSummaryView>> {
        let item_ids = if created {
            WorkItemOrigin::find()
                .filter(work_item_origin::Column::ProjectId.eq(project_id))
                .filter(work_item_origin::Column::AgentRunId.eq(run_id))
                .all(transaction.connection())
                .await
                .context("failed to load items created by run")?
                .into_iter()
                .map(|origin| origin.work_item_id)
                .collect::<Vec<_>>()
        } else {
            let created_ids = WorkItemOrigin::find()
                .filter(work_item_origin::Column::ProjectId.eq(project_id))
                .filter(work_item_origin::Column::AgentRunId.eq(run_id))
                .all(transaction.connection())
                .await
                .context("failed to load run-created items")?
                .into_iter()
                .map(|origin| origin.work_item_id)
                .collect::<std::collections::BTreeSet<_>>();
            work_item_event::Entity::find()
                .filter(work_item_event::Column::ProjectId.eq(project_id))
                .filter(work_item_event::Column::AgentRunId.eq(run_id))
                .all(transaction.connection())
                .await
                .context("failed to load items modified by run")?
                .into_iter()
                .filter_map(|event| event.work_item_id)
                .filter(|item_id| !created_ids.contains(item_id))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
        };
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = WorkItem::find()
            .filter(work_item::Column::ProjectId.eq(project_id))
            .filter(work_item::Column::Id.is_in(item_ids))
            .order_by_asc(work_item::Column::Id)
            .all(transaction.connection())
            .await
            .context("failed to load run item summaries")?;
        let summaries = crate::backend::items::repository::views::models_to_views(
            transaction.connection(),
            project_id,
            models,
        )
        .await?
        .into_iter()
        .map(|view| WorkItemSummaryView {
            id: view.id,
            title: view.title,
            state: view.state,
            updated_at: view.updated_at,
        })
        .collect();
        Ok(summaries)
    }
}
