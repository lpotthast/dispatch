use super::super::settings::{normalize_optional, parse_agent_extra_writable_roots_text};
use crate::backend::{
    entities::project::{ProjectActiveModel, ProjectModel},
    knowledge::normalize_knowledge_directory,
};
use dispatch_types::*;
use rootcause::{Result, prelude::*};
use sea_orm::ActiveValue::Set;
use std::str::FromStr;

pub(crate) fn decode(project: ProjectModel) -> Result<ProjectView> {
    let view = decode_for_edit(project)?;
    super::super::settings::validate_project_settings(&view)?;
    Ok(view)
}

/// Decode enums and structured values before an edit applies and validates the resulting policy.
pub(crate) fn decode_for_edit(project: ProjectModel) -> Result<ProjectView> {
    let name = project.name.clone();
    let view = (|| -> Result<ProjectView> {
        Ok(ProjectView {
            id: project.id,
            name: project.name,
            display_name: project.display_name,
            path: project.path,
            path_exists: project.path_exists,
            path_checked_at: project.path_checked_at,
            git_status: None,
            system_prompt: project.system_prompt,
            created_at: project.created_at,
            updated_at: project.updated_at,

            knowledge_directory: normalize_knowledge_directory(&project.knowledge_directory)
                .context("project has invalid knowledge directory")?,
            workspace_mode: WorkspaceMode::from_str(&project.workspace_mode)
                .context("project has invalid workspace mode")?,
            max_code_edit_agents: project.max_code_edit_agents,
            max_read_only_agents: project.max_read_only_agents,
            create_pr: project.create_pr,
            auto_commit: project.auto_commit,
            commit_standard: project.commit_standard.clone(),
            revert_strategy: RevertStrategy::from_str(&project.revert_strategy)
                .context("project has invalid revert strategy")?,
            stale_claim_minutes: project.stale_claim_minutes,
            worktree_cleanup_policy: WorktreeCleanupPolicy::from_str(
                &project.worktree_cleanup_policy,
            )
            .context("project has invalid worktree cleanup policy")?,
            default_agent_tool: AgentToolName::from_str(&project.default_agent_tool)
                .context("project has invalid default agent tool")?,
            default_agent_model: normalize_optional(project.default_agent_model.clone()),
            default_agent_reasoning_effort: project
                .default_agent_reasoning_effort
                .as_deref()
                .map(str::parse::<AgentReasoningEffort>)
                .transpose()
                .context("project has invalid default agent reasoning effort")?,
            agent_sandbox_mode: AgentSandboxMode::from_str(&project.agent_sandbox_mode)
                .context("project has invalid agent sandbox mode")?,
            agent_extra_writable_roots: parse_agent_extra_writable_roots_storage(
                &project.agent_extra_writable_roots,
            )
            .context("project has invalid extra writable roots")?,
            agent_git_command_policy: parse_agent_git_command_policy_storage(
                &project.agent_git_command_policy,
            )
            .context("project has invalid agent Git command policy")?,
        })
    })()
    .context_with(|| format!("project '{name}' has invalid settings"))?;
    Ok(view)
}

pub(crate) fn parse_agent_extra_writable_roots_storage(value: &str) -> Result<Vec<String>> {
    parse_agent_extra_writable_roots_text(value)
}

pub(crate) fn serialize_agent_extra_writable_roots(roots: &[String]) -> String {
    roots.join("\n")
}

pub(crate) fn default_agent_git_command_policy_json() -> String {
    serialize_agent_git_command_policy(&AgentGitCommandPolicy::default())
}

pub(crate) fn parse_agent_git_command_policy_storage(value: &str) -> Result<AgentGitCommandPolicy> {
    let value = value.trim();
    if value.is_empty() {
        return Ok(AgentGitCommandPolicy::default());
    }
    Ok(serde_json::from_str(value).context("failed to parse agent git command policy")?)
}

pub(crate) fn serialize_agent_git_command_policy(policy: &AgentGitCommandPolicy) -> String {
    serde_json::to_string(policy).expect("agent git command policy must serialize")
}

pub(super) fn encode(project: ProjectView) -> ProjectActiveModel {
    ProjectActiveModel {
        id: Set(project.id),
        name: Set(project.name),
        display_name: Set(project.display_name),
        path: Set(project.path),
        knowledge_directory: Set(project.knowledge_directory),
        path_exists: Set(project.path_exists),
        path_checked_at: Set(project.path_checked_at),
        system_prompt: Set(project.system_prompt),
        workspace_mode: Set(project.workspace_mode.as_storage().to_owned()),
        max_code_edit_agents: Set(project.max_code_edit_agents),
        max_read_only_agents: Set(project.max_read_only_agents),
        create_pr: Set(project.create_pr),
        auto_commit: Set(project.auto_commit),
        commit_standard: Set(project.commit_standard),
        revert_strategy: Set(project.revert_strategy.as_storage().to_owned()),
        stale_claim_minutes: Set(project.stale_claim_minutes),
        worktree_cleanup_policy: Set(project.worktree_cleanup_policy.as_storage().to_owned()),
        default_agent_tool: Set(project.default_agent_tool.as_storage().to_owned()),
        default_agent_model: Set(project.default_agent_model),
        default_agent_reasoning_effort: Set(project
            .default_agent_reasoning_effort
            .map(|effort| effort.as_storage().to_owned())),
        agent_sandbox_mode: Set(project.agent_sandbox_mode.as_storage().to_owned()),
        agent_extra_writable_roots: Set(serialize_agent_extra_writable_roots(
            &project.agent_extra_writable_roots,
        )),
        agent_git_command_policy: Set(serialize_agent_git_command_policy(
            &project.agent_git_command_policy,
        )),
        created_at: Set(project.created_at),
        updated_at: Set(project.updated_at),
        ..Default::default()
    }
}
