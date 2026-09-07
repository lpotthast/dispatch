//! Run administration coordinates runtime retirement and authoritative lifecycle writes.
use super::{
    admission::service::RunAdmissionService, cleanup::RunCleanupRuntime,
    model::RunArtifactLocations, repository::RunRepository, service::RunService,
};
use crate::backend::{
    execution::sessions::{DeletionAdmissionRejection, ProcessSessionRegistry},
    knowledge::jobs::service::JobService,
    projects::repository::ProjectRepository,
    storage::TransactionManager,
};
use rootcause::{Result, prelude::*};
use std::{sync::Arc, time::Duration};
pub(crate) struct RunAdministrationService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<RunRepository>,
    runs: Arc<RunService>,
    jobs: Arc<JobService>,
    admission: Arc<RunAdmissionService>,
    sessions: ProcessSessionRegistry,
    runtime: Arc<RunCleanupRuntime>,
}
impl RunAdministrationService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<RunRepository>,
        runs: Arc<RunService>,
        jobs: Arc<JobService>,
        admission: Arc<RunAdmissionService>,
        sessions: ProcessSessionRegistry,
        runtime: Arc<RunCleanupRuntime>,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            runs,
            jobs,
            admission,
            sessions,
            runtime,
        }
    }
    pub(crate) async fn delete(&self, project_id: i64, run_id: i64) -> Result<u64> {
        let _admission = self.admission.acquire().await;
        let tx = self.transactions.begin().await?;
        let project = self.projects.scope_by_id_in(&tx, project_id).await?;
        let run = self.repository.get_in(&tx, project_id, run_id).await?;
        tx.commit().await?;
        let guard = self
            .sessions
            .begin_run_deletion(project_id, run_id)
            .map_err(|reason| match reason {
                DeletionAdmissionRejection::InProgress => {
                    report!("run {run_id} deletion is already in progress")
                }
                DeletionAdmissionRejection::AlreadyDeleted => {
                    report!("run {run_id} was already deleted")
                }
            })?;
        if let Some(job) = run.knowledge_job_id {
            self.jobs
                .action(
                    &project.name,
                    job,
                    dispatch_types::knowledge::jobs::JobAction::Cancel,
                )
                .await?;
        }
        self.sessions.cancel_run(&project.name, run_id);
        tokio::time::timeout(Duration::from_secs(30), async {
            while self.sessions.get_for_project(project_id, run_id).is_some() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        })
        .await
        .context("timed out waiting for the run to stop before deletion")?;
        // Completion may have recorded workspace paths after the first observation.
        let tx = self.transactions.begin().await?;
        let current = self.repository.get_in(&tx, project_id, run_id).await?;
        tx.commit().await?;
        let runtime = self.runtime.clone();
        let cleanup_project = project.clone();
        tokio::task::spawn_blocking(move || {
            runtime.cleanup(&cleanup_project, &[RunArtifactLocations::from(&current)])
        })
        .await??;
        let deleted = self.runs.delete_record(&project, run_id).await?;
        guard.mark_deleted();
        Ok(deleted)
    }
}
