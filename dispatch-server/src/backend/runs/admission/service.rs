use super::{
    RunningRunCounts,
    policy::{
        allowed_runs_for_mutability, ensure_supported_launch_settings, validate_execution_policy,
    },
    repository::RunAdmissionRepository,
};
use crate::backend::{
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{AutomationExecutionPolicy, AutomationRunMutability, ProjectSettingsView};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

/// Serializes runtime admission and filesystem publication for this application instance.
/// The permit spans the short admission check and durable run allocation, or bounded publication.
pub(crate) struct RuntimeAdmissionPermit<'a> {
    _guard: MutexGuard<'a, ()>,
}

pub(crate) struct RunAdmissionService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<RunAdmissionRepository>,
    coordination: Mutex<()>,
}
impl RunAdmissionService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<RunAdmissionRepository>,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            coordination: Mutex::new(()),
        }
    }
    pub(crate) async fn acquire(&self) -> RuntimeAdmissionPermit<'_> {
        RuntimeAdmissionPermit {
            _guard: self.coordination.lock().await,
        }
    }
    pub(crate) async fn enforce_start_allowed(
        &self,
        project: &str,
        settings: &ProjectSettingsView,
        mutability: AutomationRunMutability,
    ) -> Result<()> {
        self.enforce_rule_start_allowed(
            project,
            settings,
            mutability,
            None,
            &AutomationExecutionPolicy::default(),
        )
        .await
    }
    pub(crate) async fn enforce_rule_start_allowed(
        &self,
        project: &str,
        settings: &ProjectSettingsView,
        mutability: AutomationRunMutability,
        trigger_id: Option<i64>,
        execution: &AutomationExecutionPolicy,
    ) -> Result<()> {
        let transaction = self.transactions.begin().await?;
        self.enforce_in(
            &transaction,
            project,
            settings,
            mutability,
            trigger_id,
            execution,
        )
        .await?;
        transaction.commit().await
    }
    pub(crate) async fn enforce_in(
        &self,
        transaction: &Transaction,
        project: &str,
        settings: &ProjectSettingsView,
        mutability: AutomationRunMutability,
        trigger_id: Option<i64>,
        execution: &AutomationExecutionPolicy,
    ) -> Result<()> {
        let current = self.projects.settings_in(transaction, project).await?;
        let project_id = current.project_id;
        if project_id != settings.project_id {
            bail!("project settings no longer belong to this project");
        }
        if current != *settings {
            bail!("project settings changed during admission; retry the operation");
        }
        ensure_supported_launch_settings(settings, mutability)?;
        let allowed = allowed_runs_for_mutability(settings, mutability);
        let running = self
            .repository
            .counts_in(transaction, project_id)
            .await?
            .for_mutability(mutability);
        if running >= allowed {
            match mutability {
                AutomationRunMutability::Mutating => bail!(
                    "project already has {running} running mutating agent run(s); limit is {allowed}"
                ),
                AutomationRunMutability::ReadOnly => bail!(
                    "project already has {running} running read-only agent run(s); limit is {allowed}"
                ),
            }
        }
        validate_execution_policy(execution)?;
        if let (Some(trigger_id), Some(limit)) = (trigger_id, execution.max_concurrent_runs) {
            let running = self
                .repository
                .rule_count_in(transaction, project_id, trigger_id)
                .await?;
            if running >= limit {
                bail!("automation rule already has {running} running run(s); limit is {limit}");
            }
        }
        if let Some(group) = execution.concurrency_group.as_deref()
            && self
                .repository
                .group_count_in(transaction, project_id, group)
                .await?
                > 0
        {
            bail!("automation concurrency group '{group}' already has an active run");
        }
        Ok(())
    }
    pub(crate) async fn running_counts_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<RunningRunCounts> {
        self.repository.counts_in(transaction, project_id).await
    }
    pub(crate) async fn running_counts_for_project_id(
        &self,
        project_id: i64,
    ) -> Result<RunningRunCounts> {
        let transaction = self.transactions.begin().await?;
        let counts = self.repository.counts_in(&transaction, project_id).await?;
        transaction.commit().await?;
        Ok(counts)
    }
    #[cfg(test)]
    pub(crate) async fn can_start_run(
        &self,
        project: &str,
        mutability: AutomationRunMutability,
    ) -> Result<bool> {
        let transaction = self.transactions.begin().await?;
        let settings = self.projects.settings_in(&transaction, project).await?;
        let running = self
            .repository
            .counts_in(&transaction, settings.project_id)
            .await?
            .for_mutability(mutability);
        transaction.commit().await?;
        Ok(running < allowed_runs_for_mutability(&settings, mutability))
    }
}
