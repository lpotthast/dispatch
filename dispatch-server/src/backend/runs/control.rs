//! Operator cancellation coordinates the owning workflow with the live process session.
use super::queries::service::RunQueryService;
use crate::backend::{
    execution::sessions::ProcessSessionRegistry, knowledge::jobs::service::JobService,
};
use dispatch_types::{AgentRunStatus, knowledge::jobs::JobAction};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
pub(crate) struct RunControlService {
    runs: Arc<RunQueryService>,
    jobs: Arc<JobService>,
    sessions: ProcessSessionRegistry,
}
impl RunControlService {
    pub(crate) fn new(
        runs: Arc<RunQueryService>,
        jobs: Arc<JobService>,
        sessions: ProcessSessionRegistry,
    ) -> Self {
        Self {
            runs,
            jobs,
            sessions,
        }
    }
    pub(crate) async fn cancel(&self, project: &str, run_id: i64) -> Result<()> {
        let run = self.runs.get(project, run_id).await?;
        if let Some(job) = run.knowledge_job_id {
            self.jobs.action(project, job, JobAction::Cancel).await?;
            return Ok(());
        }
        if run.status != AgentRunStatus::Running {
            bail!("automation run {run_id} is not running");
        }
        if !self.sessions.cancel_run(project, run_id) {
            bail!("automation run {run_id} does not have an active session");
        }
        Ok(())
    }
}
