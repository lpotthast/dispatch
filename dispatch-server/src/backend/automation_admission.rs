use std::str::FromStr;

use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    backend::{
        entities::agent_run::{self, AgentRun},
        projects,
        storage::Store,
    },
    shared::view_models::{
        AgentRunStatus, AutomationExecutionPolicy, AutomationRunMutability, ProjectSettingsView,
        WorkspaceMode,
    },
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RunningRunCounts {
    pub(crate) mutating: i64,
    pub(crate) read_only: i64,
}

impl RunningRunCounts {
    pub(crate) fn total(self) -> i64 {
        self.mutating.saturating_add(self.read_only)
    }

    fn for_mutability(self, mutability: AutomationRunMutability) -> i64 {
        match mutability {
            AutomationRunMutability::Mutating => self.mutating,
            AutomationRunMutability::ReadOnly => self.read_only,
        }
    }
}

pub(crate) async fn enforce_start_allowed(
    store: &Store,
    project_name: &str,
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> Result<()> {
    ensure_supported_launch_settings(settings, mutability)?;

    let allowed = allowed_runs_for_mutability(settings, mutability);
    let running = running_counts(store, project_name)
        .await?
        .for_mutability(mutability);
    if running >= allowed {
        match mutability {
            AutomationRunMutability::Mutating => {
                bail!(
                    "project already has {running} running mutating agent run(s); limit is {allowed}"
                );
            }
            AutomationRunMutability::ReadOnly => {
                bail!(
                    "project already has {running} running read-only agent run(s); limit is {allowed}"
                );
            }
        }
    }
    Ok(())
}

pub(crate) async fn enforce_rule_start_allowed(
    store: &Store,
    project_name: &str,
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
    trigger_id: Option<i64>,
    execution: &AutomationExecutionPolicy,
) -> Result<()> {
    enforce_start_allowed(store, project_name, settings, mutability).await?;
    validate_execution_policy(execution)?;
    let project_id = projects::project_id(store, project_name).await?;
    if let (Some(trigger_id), Some(limit)) = (trigger_id, execution.max_concurrent_runs) {
        let running = AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
            .filter(agent_run::Column::TriggerId.eq(trigger_id))
            .all(store.db().as_ref())
            .await
            .context("failed to count running automation rule runs")?
            .len() as u64;
        if running >= limit {
            bail!("automation rule already has {running} running run(s); limit is {limit}");
        }
    }
    if let Some(group) = execution.concurrency_group.as_deref() {
        let running = AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
            .filter(agent_run::Column::EffectiveConcurrencyGroup.eq(group))
            .all(store.db().as_ref())
            .await
            .context("failed to inspect automation concurrency group")?;
        if !running.is_empty() {
            bail!("automation concurrency group '{group}' already has an active run");
        }
    }
    Ok(())
}

pub(crate) fn validate_execution_policy(execution: &AutomationExecutionPolicy) -> Result<()> {
    if execution.timeout_seconds == Some(0) {
        bail!("automation timeout must be positive");
    }
    if execution
        .timeout_seconds
        .is_some_and(|value| value > i64::MAX as u64)
    {
        bail!("automation timeout is too large");
    }
    if execution.max_concurrent_runs == Some(0) {
        bail!("automation concurrent-run limit must be positive");
    }
    if execution
        .max_concurrent_runs
        .is_some_and(|value| value > i64::MAX as u64)
    {
        bail!("automation concurrent-run limit is too large");
    }
    if let Some(group) = execution.concurrency_group.as_deref() {
        crate::backend::automation_triggers::validate_stable_key(
            "automation concurrency group",
            group,
        )?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) async fn can_start_run(
    store: &Store,
    project_name: &str,
    mutability: AutomationRunMutability,
) -> Result<bool> {
    let settings = projects::get_settings(store, project_name).await?;
    let allowed = allowed_runs_for_mutability(&settings, mutability);
    let running = running_counts(store, project_name)
        .await?
        .for_mutability(mutability);
    Ok(running < allowed)
}

pub(crate) async fn running_counts(store: &Store, project_name: &str) -> Result<RunningRunCounts> {
    let project_id = projects::project_id(store, project_name).await?;
    let runs = AgentRun::find()
        .filter(agent_run::Column::ProjectId.eq(project_id))
        .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
        .all(store.db().as_ref())
        .await
        .context("failed to load running agent runs")?;
    let mut counts = RunningRunCounts::default();
    for run in runs {
        match AutomationRunMutability::from_str(&run.mutability)? {
            AutomationRunMutability::Mutating => counts.mutating += 1,
            AutomationRunMutability::ReadOnly => counts.read_only += 1,
        }
    }
    Ok(counts)
}

fn ensure_supported_launch_settings(
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> Result<()> {
    if mutability == AutomationRunMutability::Mutating
        && settings.create_pr
        && settings.workspace_mode == WorkspaceMode::CurrentBranch
    {
        bail!("pull requests can only be created for git_worktree or git_branch strategies");
    }
    Ok(())
}

fn allowed_runs_for_mutability(
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> i64 {
    match mutability {
        AutomationRunMutability::Mutating => projects::allowed_code_edit_agents(settings),
        AutomationRunMutability::ReadOnly => settings.max_read_only_agents,
    }
}
