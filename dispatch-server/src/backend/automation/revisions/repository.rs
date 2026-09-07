use crate::backend::{
    entities::{
        agent_run::{self, AgentRun},
        automation_evaluation::{self, AutomationEvaluation},
        automation_trigger_revision::{self, AutomationTriggerRevision},
        work_item_origin::{self, WorkItemOrigin},
    },
    storage::Transaction,
};
use dispatch_types::{AgentRunStatus, AutomationEvaluationView, RevisionAnalyticsView};
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
pub(crate) struct RevisionQueryRepository;
impl RevisionQueryRepository {
    pub(crate) async fn evaluations_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        trigger_id: Option<i64>,
        limit: u64,
    ) -> Result<Vec<AutomationEvaluationView>> {
        let mut query = AutomationEvaluation::find()
            .filter(automation_evaluation::Column::ProjectId.eq(project_id));
        if let Some(trigger_id) = trigger_id {
            query = query.filter(automation_evaluation::Column::TriggerId.eq(trigger_id));
        }
        query
            .order_by_desc(automation_evaluation::Column::Id)
            .limit(limit.clamp(1, 500))
            .all(transaction.connection())
            .await
            .context("failed to list automation evaluations")?
            .into_iter()
            .map(|evaluation| {
                Ok(AutomationEvaluationView {
                    id: evaluation.id,
                    project_id: evaluation.project_id,
                    trigger_id: evaluation.trigger_id,
                    trigger_revision_id: evaluation.trigger_revision_id,
                    trigger_name: evaluation.trigger_name,
                    activation_cause: evaluation.activation_cause,
                    outcome: evaluation.outcome.parse()?,
                    work_item_id: evaluation.work_item_id,
                    run_id: evaluation.run_id,
                    error: evaluation.error,
                    created_at: evaluation.created_at,
                    completed_at: evaluation.completed_at,
                })
            })
            .collect()
    }

    pub(crate) async fn analytics_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        revision_id: i64,
    ) -> Result<RevisionAnalyticsView> {
        let revision = AutomationTriggerRevision::find_by_id(revision_id)
            .filter(automation_trigger_revision::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load automation revision")?
            .ok_or_else(|| report!("automation revision {revision_id} does not exist"))?;
        let runs = AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::TriggerRevisionId.eq(revision.id))
            .all(transaction.connection())
            .await
            .context("failed to load revision runs")?;
        let run_ids = runs.iter().map(|run| run.id).collect::<Vec<_>>();
        let origins = WorkItemOrigin::find()
            .filter(work_item_origin::Column::ProjectId.eq(project_id))
            .all(transaction.connection())
            .await
            .context("failed to load revision item origins")?;
        let mut analytics = RevisionAnalyticsView {
            revision_id,
            run_count: runs.len() as u64,
            created_item_count: origins
                .iter()
                .filter(|origin| {
                    origin.trigger_revision_id == Some(revision_id)
                        || origin.agent_run_id.is_some_and(|id| run_ids.contains(&id))
                })
                .count() as u64,
            ..Default::default()
        };
        for run in runs {
            match run.status.parse::<AgentRunStatus>()? {
                AgentRunStatus::Completed => analytics.completed_count += 1,
                AgentRunStatus::Failed => analytics.failed_count += 1,
                _ => {}
            }
            match run.semantic_postcondition_status.as_str() {
                "passed" => analytics.semantic_passed_count += 1,
                "failed" => analytics.semantic_failed_count += 1,
                _ => {}
            }
            analytics.input_tokens += run.input_tokens.unwrap_or_default().max(0) as u64;
            analytics.cached_input_tokens +=
                run.cached_input_tokens.unwrap_or_default().max(0) as u64;
            analytics.output_tokens += run.output_tokens.unwrap_or_default().max(0) as u64;
            if let (Some(started), Some(finished)) = (run.started_at, run.finished_at)
                && let (Ok(started), Ok(finished)) = (
                    OffsetDateTime::parse(&started, &Rfc3339),
                    OffsetDateTime::parse(&finished, &Rfc3339),
                )
            {
                analytics.total_duration_seconds +=
                    (finished - started).whole_seconds().max(0) as u64;
            }
        }
        Ok(analytics)
    }
}
