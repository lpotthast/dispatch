use crate::backend::projects;
use dispatch_types::{
    AutomationExecutionPolicy, AutomationRunMutability, ProjectSettingsView, WorkspaceMode,
};
use rootcause::{Result, prelude::*};
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
        crate::backend::automation::rules::policy::validate_stable_key(
            "automation concurrency group",
            group,
        )?;
    }
    Ok(())
}

pub(super) fn ensure_supported_launch_settings(
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

pub(super) fn allowed_runs_for_mutability(
    settings: &ProjectSettingsView,
    mutability: AutomationRunMutability,
) -> i64 {
    match mutability {
        AutomationRunMutability::Mutating => projects::allowed_code_edit_agents(settings),
        AutomationRunMutability::ReadOnly => settings.max_read_only_agents,
    }
}
