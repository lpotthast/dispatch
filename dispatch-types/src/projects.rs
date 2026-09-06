use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{AgentReasoningEffort, AgentSandboxMode, AgentToolName, ParseEnumError};

fn legacy_knowledge_directory() -> String {
    "design".to_owned()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectView {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub path: Option<String>,
    #[serde(default = "legacy_knowledge_directory")]
    pub knowledge_directory: String,
    pub path_exists: bool,
    pub path_checked_at: Option<String>,
    pub git_status: Option<ProjectGitStatusView>,
    pub system_prompt: String,
    pub workspace_mode: WorkspaceMode,
    pub max_code_edit_agents: i64,
    pub max_read_only_agents: i64,
    pub create_pr: bool,
    pub auto_commit: bool,
    pub commit_standard: String,
    pub revert_strategy: RevertStrategy,
    pub stale_claim_minutes: i64,
    pub worktree_cleanup_policy: WorktreeCleanupPolicy,
    pub default_agent_tool: AgentToolName,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub agent_sandbox_mode: AgentSandboxMode,
    pub agent_extra_writable_roots: Vec<String>,
    pub agent_git_command_policy: AgentGitCommandPolicy,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectGitStatusView {
    pub is_repository: bool,
    pub branch: Option<String>,
    pub added_lines: u64,
    pub deleted_lines: u64,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct WorkspaceEditorView {
    pub target: String,
    pub label: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HistoryClearResult {
    pub deleted_events: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectSystemPromptView {
    pub project_id: i64,
    pub project_name: String,
    pub system_prompt: String,
    pub last_event: Option<ProjectSystemPromptEventView>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ProjectSystemPromptEventView {
    pub id: i64,
    pub project_id: i64,
    pub project_name: String,
    pub operation: String,
    pub system_prompt: String,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectSystemPromptUpdateView {
    pub project: ProjectView,
    pub event: ProjectSystemPromptEventView,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceMode {
    CurrentBranch,
    GitWorktree,
    GitBranch,
}

impl WorkspaceMode {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::CurrentBranch => "current_branch",
            Self::GitWorktree => "git_worktree",
            Self::GitBranch => "git_branch",
        }
    }
}

impl fmt::Display for WorkspaceMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for WorkspaceMode {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "current_branch" => Ok(Self::CurrentBranch),
            "git_worktree" => Ok(Self::GitWorktree),
            "git_branch" => Ok(Self::GitBranch),
            _ => Err(ParseEnumError(
                "workspace mode must be one of: current_branch, git_worktree, git_branch",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentGitHardResetPolicy {
    Never,
    #[default]
    IsolatedWorkspaces,
}

impl AgentGitHardResetPolicy {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Never => "never",
            Self::IsolatedWorkspaces => "isolated_workspaces",
        }
    }
}

impl fmt::Display for AgentGitHardResetPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentGitHardResetPolicy {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "never" => Ok(Self::Never),
            "isolated" | "isolated_workspace" | "isolated_workspaces" => {
                Ok(Self::IsolatedWorkspaces)
            }
            _ => Err(ParseEnumError(
                "agent git hard-reset policy must be one of: never, isolated_workspaces",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentGitCommandPolicy {
    pub add: bool,
    pub commit: bool,
    pub push: bool,
    pub reset: bool,
    pub hard_reset: AgentGitHardResetPolicy,
}

impl AgentGitCommandPolicy {
    pub fn allows_hard_reset(&self, workspace_mode: WorkspaceMode) -> bool {
        self.reset
            && match self.hard_reset {
                AgentGitHardResetPolicy::Never => false,
                AgentGitHardResetPolicy::IsolatedWorkspaces => {
                    workspace_mode != WorkspaceMode::CurrentBranch
                }
            }
    }
}

impl Default for AgentGitCommandPolicy {
    fn default() -> Self {
        Self {
            add: true,
            commit: true,
            push: true,
            reset: true,
            hard_reset: AgentGitHardResetPolicy::IsolatedWorkspaces,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentGitRuntimePolicy {
    pub policy: AgentGitCommandPolicy,
    pub workspace_mode: WorkspaceMode,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorktreeCleanupPolicy {
    Manual,
    AfterSuccess,
}

impl WorktreeCleanupPolicy {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::AfterSuccess => "after_success",
        }
    }
}

impl fmt::Display for WorktreeCleanupPolicy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for WorktreeCleanupPolicy {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "manual" => Ok(Self::Manual),
            "after_success" => Ok(Self::AfterSuccess),
            _ => Err(ParseEnumError(
                "worktree cleanup policy must be one of: manual, after_success",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevertStrategy {
    Manual,
    GitReset,
}

impl RevertStrategy {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::GitReset => "git_reset",
        }
    }
}

impl fmt::Display for RevertStrategy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for RevertStrategy {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "manual" => Ok(Self::Manual),
            "git_reset" => Ok(Self::GitReset),
            _ => Err(ParseEnumError(
                "revert strategy must be one of: manual, git_reset",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectSettingsView {
    pub id: i64,
    pub project_id: i64,
    #[serde(default = "legacy_knowledge_directory")]
    pub knowledge_directory: String,
    pub workspace_mode: WorkspaceMode,
    pub max_code_edit_agents: i64,
    pub max_read_only_agents: i64,
    pub create_pr: bool,
    pub auto_commit: bool,
    pub commit_standard: String,
    pub revert_strategy: RevertStrategy,
    pub stale_claim_minutes: i64,
    pub worktree_cleanup_policy: WorktreeCleanupPolicy,
    pub default_agent_tool: AgentToolName,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub agent_sandbox_mode: AgentSandboxMode,
    pub agent_extra_writable_roots: Vec<String>,
    pub agent_git_command_policy: AgentGitCommandPolicy,
    pub created_at: String,
    pub updated_at: String,
}
