use super::{
    model::{CreateRunConfig, LaunchDetails, RunChange},
    queries::runtime::RunArtifacts,
    repository::RunRepository,
};
use crate::backend::{
    events::UiEventBus,
    execution::identity as agent_ids,
    items::claims::{
        model::{AutomationClaimFinalization, AutomationClaimOutcome},
        service::ClaimService,
    },
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{AgentRunCleanupStatus, AgentRunStatus, AgentRunTokenUsageView, AgentRunView};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
pub(crate) struct RunService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<RunRepository>,
    claims: Arc<ClaimService>,
    artifacts: Arc<RunArtifacts>,
    events: UiEventBus,
}
impl RunService {
    /// Administrative forms cannot supply the launch contract and execution inputs required for allocation.
    /// The route remains available with the same repository-error response as the former incomplete insert.
    pub(crate) async fn create_administrative(
        &self,
        project_id: i64,
        _metadata: super::model::RunMetadata,
    ) -> Result<std::convert::Infallible> {
        let transaction = self.transactions.begin().await?;
        self.projects.name_in(&transaction, project_id).await?;
        bail!("agent runs require prepared launch inputs; use the automation launch service");
    }
    pub(crate) async fn update_metadata(
        &self,
        project_id: i64,
        run_id: i64,
        metadata: super::model::RunMetadata,
    ) -> Result<()> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let current = self
            .repository
            .get_in(&transaction, project_id, run_id)
            .await?;
        if metadata.work_item_id != current.work_item_id {
            if current.launch_target.is_some() {
                bail!("run item attribution is bound to its launch contract");
            }
            if current.status == AgentRunStatus::Running {
                bail!("stop the run before changing its item attribution");
            }
        }
        if let Some(item) = metadata.work_item_id
            && !self
                .repository
                .item_exists_in(&transaction, project_id, item)
                .await?
        {
            bail!("work item {item} does not exist in this project");
        }
        self.repository
            .update_metadata_in(&transaction, project_id, run_id, &metadata)
            .await?;
        transaction.commit().await?;
        self.events
            .publish_agent_run_changed(&project, run_id, metadata.work_item_id);
        Ok(())
    }
    pub(super) async fn delete_record(
        &self,
        expected: &crate::backend::projects::model::ProjectScope,
        run_id: i64,
    ) -> Result<u64> {
        let project_id = expected.id;
        let transaction = self.transactions.begin().await?;
        let current = self
            .projects
            .scope_by_id_in(&transaction, project_id)
            .await?;
        if &current != expected {
            bail!("project working copy changed during run deletion; retry cleanup");
        }
        let project = self.projects.name_in(&transaction, project_id).await?;
        let run = self
            .repository
            .get_in(&transaction, project_id, run_id)
            .await?;
        let (run, changed_item, _) = self
            .finish_in(
                &transaction,
                &project,
                run,
                AgentRunStatus::Cancelled,
                None,
                "Deleted by run administration".into(),
            )
            .await?;
        let deleted = self
            .repository
            .delete_in(&transaction, project_id, run_id)
            .await?;
        transaction.commit().await?;
        if let Some(item) = changed_item {
            self.events.publish_work_item_changed(&project, item);
        }
        self.events
            .publish_agent_run_changed(&project, run_id, run.work_item_id);
        Ok(deleted)
    }

    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<RunRepository>,
        claims: Arc<ClaimService>,
        artifacts: Arc<RunArtifacts>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            claims,
            artifacts,
            events,
        }
    }
    pub(crate) async fn create(
        &self,
        project_id: i64,
        config: CreateRunConfig<'_>,
    ) -> Result<AgentRunView> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, project_id).await?;
        let run = self.create_in(&transaction, project_id, config).await?;
        transaction.commit().await?;
        self.events
            .publish_agent_run_changed(&project, run.id, run.work_item_id);
        Ok(run)
    }
    pub(crate) async fn create_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        config: CreateRunConfig<'_>,
    ) -> Result<AgentRunView> {
        self.projects.name_in(transaction, project_id).await?;
        self.repository
            .create_in(transaction, project_id, config)
            .await
    }
    async fn change(&self, run: AgentRunView, change: RunChange<'_>) -> Result<AgentRunView> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, run.project_id).await?;
        let run = self
            .repository
            .change_in(&transaction, run.project_id, run.id, change)
            .await?;
        transaction.commit().await?;
        self.events
            .publish_agent_run_changed(&project, run.id, run.work_item_id);
        Ok(run)
    }
    pub(crate) async fn launch(
        &self,
        run: AgentRunView,
        details: LaunchDetails,
    ) -> Result<AgentRunView> {
        self.change(run, RunChange::Launch(Box::new(details))).await
    }
    pub(crate) async fn process_id(
        &self,
        run: AgentRunView,
        id: Option<i64>,
    ) -> Result<AgentRunView> {
        self.change(run, RunChange::ProcessId(id)).await
    }
    pub(crate) async fn token_usage(
        &self,
        run: AgentRunView,
        usage: AgentRunTokenUsageView,
    ) -> Result<AgentRunView> {
        self.change(run, RunChange::TokenUsage(usage)).await
    }
    pub(crate) async fn commit_outcome(
        &self,
        run: AgentRunView,
        evaluation: &crate::backend::automation::launch::commit::CommitOutcomeEvaluation,
    ) -> Result<AgentRunView> {
        self.change(run, RunChange::Commit(evaluation)).await
    }
    pub(crate) async fn semantics(
        &self,
        run: AgentRunView,
        evaluation: &crate::backend::automation::postconditions::model::SemanticEvaluation,
    ) -> Result<AgentRunView> {
        self.change(run, RunChange::Semantics(evaluation)).await
    }
    pub(crate) async fn pull_request(
        &self,
        run: AgentRunView,
        url: Option<String>,
    ) -> Result<AgentRunView> {
        self.change(run, RunChange::PullRequest(url)).await
    }
    pub(crate) async fn cleanup(
        &self,
        run: AgentRunView,
        cleanup_status: AgentRunCleanupStatus,
        worktree_cleaned_at: Option<String>,
    ) -> Result<AgentRunView> {
        self.change(
            run,
            RunChange::Cleanup {
                cleanup_status,
                worktree_cleaned_at,
            },
        )
        .await
    }
    pub(crate) async fn finish(
        &self,
        run: AgentRunView,
        status: AgentRunStatus,
        exit_code: Option<i64>,
        result_summary: String,
    ) -> Result<AgentRunView> {
        let transaction = self.transactions.begin().await?;
        let project = self.projects.name_in(&transaction, run.project_id).await?;
        let (run, changed_item, changed) = self
            .finish_in(
                &transaction,
                &project,
                run,
                status,
                exit_code,
                result_summary,
            )
            .await?;
        transaction.commit().await?;
        if let Some(item_id) = changed_item {
            self.events.publish_work_item_changed(&project, item_id);
        }
        if changed {
            self.events
                .publish_agent_run_changed(&project, run.id, run.work_item_id);
        }
        Ok(run)
    }
    async fn finish_in(
        &self,
        transaction: &Transaction,
        project: &str,
        run: AgentRunView,
        status: AgentRunStatus,
        exit_code: Option<i64>,
        result_summary: String,
    ) -> Result<(AgentRunView, Option<i64>, bool)> {
        let outcome = match status {
            AgentRunStatus::Completed => AutomationClaimOutcome::CompletedUnfinished,
            AgentRunStatus::Cancelled => AutomationClaimOutcome::Cancelled,
            AgentRunStatus::Failed => AutomationClaimOutcome::Failed,
            AgentRunStatus::Running => bail!("cannot finish a run with running status"),
        };
        let current = self
            .repository
            .get_in(transaction, run.project_id, run.id)
            .await?;
        if current.status != AgentRunStatus::Running {
            return Ok((current, None, false));
        }
        let changed_item = if current.work_item_id.is_some()
            && current.knowledge_job_id.is_none()
            && !matches!(
                current.purpose,
                Some(
                    dispatch_types::AgentRunPurposeV1::KnowledgeCycle
                        | dispatch_types::AgentRunPurposeV1::KnowledgeAnswer
                )
            ) {
            self.claims
                .finalize_automation_in(
                    transaction,
                    AutomationClaimFinalization {
                        project_id: current.project_id,
                        project_name: project,
                        run_id: current.id,
                        claimed_item_id: current.work_item_id,
                        agent_id: &agent_ids::dispatch_run_agent_id(current.id),
                        outcome,
                        detail: Some(&result_summary),
                    },
                )
                .await?
        } else {
            None
        };
        let run = self
            .repository
            .change_in(
                transaction,
                current.project_id,
                current.id,
                RunChange::Finish {
                    status,
                    exit_code,
                    result_summary,
                },
            )
            .await?;
        Ok((run, changed_item, true))
    }
    pub(crate) async fn cancel_project(&self, project_id: i64) -> Result<Vec<AgentRunView>> {
        let transaction = self.transactions.begin().await?;
        let running = self
            .repository
            .list_in(&transaction, project_id, true, None)
            .await?;
        if running.is_empty() {
            transaction.commit().await?;
            return Ok(Vec::new());
        }
        let project = self.projects.name_in(&transaction, project_id).await?;
        let mut changes = Vec::with_capacity(running.len());
        for run in running {
            changes.push(
                self.finish_in(
                    &transaction,
                    &project,
                    run,
                    AgentRunStatus::Cancelled,
                    None,
                    "Marked cancelled by automation stop".into(),
                )
                .await?,
            );
        }
        transaction.commit().await?;
        for (run, changed_item, changed) in &changes {
            if let Some(item_id) = changed_item {
                self.events.publish_work_item_changed(&project, *item_id);
            }
            if *changed {
                self.events
                    .publish_agent_run_changed(&project, run.id, run.work_item_id);
            }
        }
        Ok(changes.into_iter().map(|(run, _, _)| run).collect())
    }
    pub(crate) async fn list_for_project(
        &self,
        project_id: i64,
        running_only: bool,
        run_id: Option<i64>,
    ) -> Result<Vec<AgentRunView>> {
        let transaction = self.transactions.begin().await?;
        let runs = self
            .repository
            .list_in(&transaction, project_id, running_only, run_id)
            .await?;
        transaction.commit().await?;
        Ok(runs)
    }
    #[cfg(test)]
    pub(crate) async fn find(&self, run_id: i64) -> Result<Option<AgentRunView>> {
        let transaction = self.transactions.begin().await?;
        let run = self.repository.find_in(&transaction, run_id).await?;
        transaction.commit().await?;
        Ok(run)
    }
    pub(crate) async fn find_scoped(
        &self,
        project_id: i64,
        run_id: i64,
    ) -> Result<Option<AgentRunView>> {
        let transaction = self.transactions.begin().await?;
        let run = self
            .repository
            .find_scoped_in(&transaction, project_id, run_id)
            .await?;
        transaction.commit().await?;
        Ok(run)
    }
    pub(crate) async fn with_log_usage(&self, run: AgentRunView) -> AgentRunView {
        self.artifacts.enrich(run).await
    }
}
