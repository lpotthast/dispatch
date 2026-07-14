//! Typed JSON contracts shared by the Dispatch server, API client, CLI, and hydrated frontend.
//!
//! Persistence-specific strings are converted at the server boundary. Enums in this crate retain
//! the canonical JSON and SQLite spelling so callers can use exhaustive Rust matches without
//! changing the wire protocol.

use std::{error::Error, fmt, str::FromStr};

use crudkit_core::condition::{
    Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
};
use serde::{Deserialize, Serialize};

mod optional_condition {
    use crudkit_core::condition::Condition;
    use serde::{Deserialize, Deserializer, Serialize, Serializer, de::Error as _};

    pub(super) fn serialize<S>(
        condition: &Option<Condition>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match condition {
            Some(condition) => serde_json::to_value(condition)
                .map_err(serde::ser::Error::custom)?
                .serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub(super) fn deserialize<'de, D>(deserializer: D) -> Result<Option<Condition>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<serde_json::Value>::deserialize(deserializer)?
            .map(|value| serde_json::from_value(value).map_err(D::Error::custom))
            .transpose()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiEvent {
    ProjectListChanged {
        sequence: u64,
        timestamp: String,
    },
    ProjectChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    SystemPromptChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    WorkItemChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        item_id: i64,
    },
    CommentChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        item_id: i64,
    },
    MemoryChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    SwimLaneChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    WorkItemStateChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    AgentToolChanged {
        sequence: u64,
        timestamp: String,
    },
    AutomationChanged {
        sequence: u64,
        timestamp: String,
        project: String,
    },
    AgentRunChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        run_id: i64,
        item_id: Option<i64>,
    },
    AgentOutputChanged {
        sequence: u64,
        timestamp: String,
        project: String,
        run_id: i64,
        item_id: Option<i64>,
    },
    CodexStatusChanged {
        sequence: u64,
        timestamp: String,
    },
}

#[derive(Debug, Clone)]
pub struct ParseEnumError(&'static str);

impl fmt::Display for ParseEnumError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl Error for ParseEnumError {}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ProjectView {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub path: Option<String>,
    pub path_exists: bool,
    pub path_checked_at: Option<String>,
    pub git_status: Option<ProjectGitStatusView>,
    pub system_prompt: String,
    pub memory: String,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectMemoryView {
    pub project_id: i64,
    pub project_name: String,
    pub memory: String,
    pub last_event: Option<ProjectMemoryEventView>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct ProjectMemoryEventView {
    pub id: i64,
    pub project_id: i64,
    pub project_name: String,
    pub operation: String,
    pub memory: String,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectMemoryUpdateView {
    pub project: ProjectView,
    pub event: ProjectMemoryEventView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectMemoryCompactionView {
    pub project_id: i64,
    pub project_name: String,
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectSystemPromptCompactionView {
    pub project_id: i64,
    pub project_name: String,
    pub deleted_events: u64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
pub struct PersonalityView {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub personality_description: String,
    #[serde(default)]
    pub current_revision_id: Option<i64>,
    #[serde(default)]
    pub managed_bundle_key: Option<String>,
    #[serde(default)]
    pub managed_object_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolName {
    Codex,
}

impl AgentToolName {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Codex => "codex",
        }
    }

    pub fn all() -> [Self; 1] {
        [Self::Codex]
    }
}

impl fmt::Display for AgentToolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentToolName {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "codex" => Ok(Self::Codex),
            _ => Err(ParseEnumError("agent tool must be codex")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodexAgentModel {
    Gpt56Sol,
    Gpt56Terra,
    Gpt56Luna,
    Gpt56,
    Gpt55,
    Gpt54,
    Gpt54Mini,
    Gpt53CodexSpark,
}

impl CodexAgentModel {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Gpt56Sol => "gpt-5.6-sol",
            Self::Gpt56Terra => "gpt-5.6-terra",
            Self::Gpt56Luna => "gpt-5.6-luna",
            Self::Gpt56 => "gpt-5.6",
            Self::Gpt55 => "gpt-5.5",
            Self::Gpt54 => "gpt-5.4",
            Self::Gpt54Mini => "gpt-5.4-mini",
            Self::Gpt53CodexSpark => "gpt-5.3-codex-spark",
        }
    }

    pub fn all() -> [Self; 8] {
        [
            Self::Gpt56Sol,
            Self::Gpt56Terra,
            Self::Gpt56Luna,
            Self::Gpt56,
            Self::Gpt55,
            Self::Gpt54,
            Self::Gpt54Mini,
            Self::Gpt53CodexSpark,
        ]
    }

    pub fn newest() -> Self {
        Self::Gpt56Sol
    }

    pub fn is_available_model(value: &str) -> bool {
        value.parse::<Self>().is_ok()
    }

    pub fn supported_reasoning_efforts(self) -> &'static [AgentReasoningEffort] {
        match self {
            Self::Gpt56Sol | Self::Gpt56Terra | Self::Gpt56Luna | Self::Gpt56 => &[
                AgentReasoningEffort::None,
                AgentReasoningEffort::Low,
                AgentReasoningEffort::Medium,
                AgentReasoningEffort::High,
                AgentReasoningEffort::XHigh,
                AgentReasoningEffort::Max,
            ],
            Self::Gpt55 | Self::Gpt54 | Self::Gpt54Mini | Self::Gpt53CodexSpark => &[
                AgentReasoningEffort::None,
                AgentReasoningEffort::Minimal,
                AgentReasoningEffort::Low,
                AgentReasoningEffort::Medium,
                AgentReasoningEffort::High,
                AgentReasoningEffort::XHigh,
            ],
        }
    }

    pub fn supports_reasoning_effort(self, effort: AgentReasoningEffort) -> bool {
        self.supported_reasoning_efforts().contains(&effort)
    }

    pub fn highest_reasoning_effort(self) -> AgentReasoningEffort {
        *self
            .supported_reasoning_efforts()
            .last()
            .expect("codex agent models must support at least one reasoning effort")
    }

    pub fn allowed_reasoning_effort_values(self) -> String {
        self.supported_reasoning_efforts()
            .iter()
            .map(|effort| effort.as_storage())
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn allowed_values() -> String {
        Self::all()
            .iter()
            .map(|model| model.as_storage())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for CodexAgentModel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for CodexAgentModel {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "gpt-5.6-sol" => Ok(Self::Gpt56Sol),
            "gpt-5.6-terra" => Ok(Self::Gpt56Terra),
            "gpt-5.6-luna" => Ok(Self::Gpt56Luna),
            "gpt-5.6" => Ok(Self::Gpt56),
            "gpt-5.5" => Ok(Self::Gpt55),
            "gpt-5.4" => Ok(Self::Gpt54),
            "gpt-5.4-mini" => Ok(Self::Gpt54Mini),
            "gpt-5.3-codex-spark" => Ok(Self::Gpt53CodexSpark),
            _ => Err(ParseEnumError("unknown codex agent model")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AgentToolView {
    pub id: i64,
    pub tool_name: AgentToolName,
    pub executable_path: Option<String>,
    pub discovered_path: Option<String>,
    pub effective_path: Option<String>,
    pub last_discovered_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexAppServerStatusView {
    pub available: bool,
    pub usable: bool,
    pub message: String,
    pub install_prompt: String,
    pub auth_setup: Option<CodexAuthSetupView>,
    pub checked_at: String,
    pub binary_path: Option<String>,
    pub requires_openai_auth: Option<bool>,
    pub signed_in: bool,
    pub auth_method: Option<String>,
    pub account_label: Option<String>,
    pub plan_type: Option<String>,
    pub payment_model: Option<String>,
    pub preconditions: Vec<CodexPreconditionView>,
    pub rate_limits: Vec<CodexRateLimitView>,
    pub usage_summary: Option<CodexUsageSummaryView>,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexAuthSetupView {
    pub codex_home_path: String,
    pub codex_config_path: String,
    pub login_command: String,
    pub refresh_instruction: String,
    pub api_key_instruction: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexPreconditionView {
    pub name: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexRateLimitView {
    pub label: String,
    pub plan_type: Option<String>,
    pub primary_used_percent: Option<i64>,
    pub primary_window_minutes: Option<i64>,
    pub primary_resets_at: Option<String>,
    pub secondary_used_percent: Option<i64>,
    pub secondary_window_minutes: Option<i64>,
    pub secondary_resets_at: Option<String>,
    pub individual_used: Option<String>,
    pub individual_limit: Option<String>,
    pub individual_remaining_percent: Option<i64>,
    pub individual_resets_at: Option<String>,
    pub credits_balance: Option<String>,
    pub credits_has_credits: Option<bool>,
    pub credits_unlimited: Option<bool>,
    pub reached_type: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexUsageSummaryView {
    pub lifetime_tokens: Option<i64>,
    pub peak_daily_tokens: Option<i64>,
    pub current_streak_days: Option<i64>,
    pub longest_streak_days: Option<i64>,
    pub longest_running_turn_seconds: Option<i64>,
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

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentSandboxMode {
    WorkspaceWrite,
    DangerFullAccess,
}

impl AgentSandboxMode {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::WorkspaceWrite => "workspace_write",
            Self::DangerFullAccess => "danger_full_access",
        }
    }
}

impl fmt::Display for AgentSandboxMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentSandboxMode {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "workspace_write" | "workspacewrite" => Ok(Self::WorkspaceWrite),
            "danger_full_access" | "dangerfullaccess" => Ok(Self::DangerFullAccess),
            _ => Err(ParseEnumError(
                "agent sandbox mode must be one of: workspace_write, danger_full_access",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
    Max,
}

impl AgentReasoningEffort {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
        }
    }

    pub fn all() -> [Self; 7] {
        [
            Self::None,
            Self::Minimal,
            Self::Low,
            Self::Medium,
            Self::High,
            Self::XHigh,
            Self::Max,
        ]
    }

    pub fn highest() -> Self {
        Self::Max
    }

    pub fn allowed_values() -> String {
        Self::all()
            .iter()
            .map(|effort| effort.as_storage())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

impl fmt::Display for AgentReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentReasoningEffort {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "none" => Ok(Self::None),
            "minimal" => Ok(Self::Minimal),
            "low" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" | "x_high" => Ok(Self::XHigh),
            "max" => Ok(Self::Max),
            _ => Err(ParseEnumError("unknown agent reasoning effort")),
        }
    }
}

pub const STATE_LABEL_KEY: &str = "state";
pub const DEFAULT_STATE_LABEL: &str = "open";
pub const CLAIMED_STATE_LABEL: &str = "in_progress";
pub const FINISHED_STATE_LABEL: &str = "done";
pub const CLAIMED_FROM_STATE_LABEL_KEY: &str = "dispatch:claimed-from-state";
pub const AUTOMATION_BLOCKED_LABEL_KEY: &str = "dispatch:automation-blocked";
pub const FEEDBACK_REQUESTED_LABEL_KEY: &str = "dispatch:feedback-requested";
pub const NEEDS_REFINEMENT_LABEL_KEY: &str = "needs-refinement";
pub const NEEDS_VERIFICATION_LABEL_KEY: &str = "needs-verification";

pub fn default_automation_work_item_selector() -> Condition {
    Condition::All(vec![
        ConditionElement::Clause(ConditionClause {
            column_name: STATE_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::String(DEFAULT_STATE_LABEL.to_owned()),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: NEEDS_REFINEMENT_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: NEEDS_VERIFICATION_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        }),
        ConditionElement::Clause(ConditionClause {
            column_name: FEEDBACK_REQUESTED_LABEL_KEY.to_owned(),
            operator: Operator::Equal,
            value: ConditionClauseValue::Bool(false),
        }),
    ])
}

pub fn needs_refinement_automation_work_item_selector() -> Condition {
    label_presence_selector(NEEDS_REFINEMENT_LABEL_KEY)
}

pub fn needs_verification_automation_work_item_selector() -> Condition {
    label_presence_selector(NEEDS_VERIFICATION_LABEL_KEY)
}

fn label_presence_selector(label_key: &str) -> Condition {
    Condition::All(vec![ConditionElement::Clause(ConditionClause {
        column_name: label_key.to_owned(),
        operator: Operator::Equal,
        value: ConditionClauseValue::Bool(true),
    })])
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemView {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub description: String,
    pub state: Option<String>,
    pub labels: Vec<WorkItemLabelView>,
    pub version: i64,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub claim_expires_at: Option<String>,
    pub claim_source: Option<WorkItemClaimSourceView>,
    pub finished_at: Option<String>,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    pub created_at: String,
    pub updated_at: String,
    pub comment_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_group: Option<WorkItemGroupSummaryView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<WorkItemOriginView>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemGroupSummaryView {
    pub id: i64,
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemGroupView {
    pub id: i64,
    pub project_id: i64,
    pub key: String,
    pub name: String,
    pub item_count: u64,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemClaimSourceView {
    pub run_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemLabelView {
    pub id: i64,
    pub project_id: i64,
    pub work_item_id: i64,
    pub key: String,
    pub value: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemRelationshipDirection {
    Outgoing,
    Incoming,
}

impl WorkItemRelationshipDirection {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Outgoing => "outgoing",
            Self::Incoming => "incoming",
        }
    }
}

impl fmt::Display for WorkItemRelationshipDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemRelationshipItemSummary {
    pub id: i64,
    pub title: String,
    pub state: Option<String>,
    pub version: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemRelationshipView {
    pub id: i64,
    pub project_id: i64,
    pub kind: String,
    pub source_work_item_id: i64,
    pub target_work_item_id: i64,
    pub source: WorkItemRelationshipItemSummary,
    pub target: WorkItemRelationshipItemSummary,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemRelationshipListEntry {
    pub relationship: WorkItemRelationshipView,
    pub direction: WorkItemRelationshipDirection,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectLabelView {
    pub key: String,
    pub value: Option<String>,
    pub usage_count: i64,
    pub last_used_at: String,
}

/// Supported ordering strategies for work items inside a swim-lane.
///
/// The enum is shared by the server and hydrated frontend so persisted lane configuration is
/// validated once and board rendering remains exhaustive when a new ordering mode is added.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SwimLaneItemOrder {
    #[default]
    UpdatedDesc,
    UpdatedAsc,
    CreatedDesc,
    CreatedAsc,
    IdDesc,
    IdAsc,
    TitleAsc,
    TitleDesc,
}

impl SwimLaneItemOrder {
    /// Returns the canonical SQLite and JSON representation.
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::UpdatedDesc => "updated_desc",
            Self::UpdatedAsc => "updated_asc",
            Self::CreatedDesc => "created_desc",
            Self::CreatedAsc => "created_asc",
            Self::IdDesc => "id_desc",
            Self::IdAsc => "id_asc",
            Self::TitleAsc => "title_asc",
            Self::TitleDesc => "title_desc",
        }
    }
}

impl fmt::Display for SwimLaneItemOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for SwimLaneItemOrder {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "updated_desc" => Ok(Self::UpdatedDesc),
            "updated_asc" => Ok(Self::UpdatedAsc),
            "created_desc" => Ok(Self::CreatedDesc),
            "created_asc" => Ok(Self::CreatedAsc),
            "id_desc" => Ok(Self::IdDesc),
            "id_asc" => Ok(Self::IdAsc),
            "title_asc" => Ok(Self::TitleAsc),
            "title_desc" => Ok(Self::TitleDesc),
            _ => Err(ParseEnumError(
                "swim-lane item order must be one of: updated_desc, updated_asc, created_desc, created_asc, id_desc, id_asc, title_asc, title_desc",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SwimLaneView {
    pub id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub filter: Condition,
    pub item_order: SwimLaneItemOrder,
    pub can_create_items: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkItemStateView {
    pub id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// A durable event kind in Dispatch's project-scoped workflow audit stream.
///
/// Storage uses historical spellings for the two project snapshot events and snake case for item
/// workflow events. Keeping those spellings behind this enum prevents producers from inventing
/// event names while preserving the existing API and database representation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemEventType {
    #[serde(rename = "SystemPromptChanged")]
    SystemPromptChanged,
    #[serde(rename = "MemoryChanged")]
    MemoryChanged,
    ItemCreated,
    ItemUpdated,
    ItemMoved,
    ItemDeleted,
    ItemClaimed,
    ProgressAdded,
    ItemFinished,
    ItemReleased,
    FeedbackRequested,
    CommentAdded,
    LabelAdded,
    LabelUpdated,
    LabelDeleted,
    RelationshipCreated,
    RelationshipUpdated,
    RelationshipDeleted,
}

impl WorkItemEventType {
    /// Returns the canonical SQLite and server-sent-event representation.
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::SystemPromptChanged => "SystemPromptChanged",
            Self::MemoryChanged => "MemoryChanged",
            Self::ItemCreated => "item_created",
            Self::ItemUpdated => "item_updated",
            Self::ItemMoved => "item_moved",
            Self::ItemDeleted => "item_deleted",
            Self::ItemClaimed => "item_claimed",
            Self::ProgressAdded => "progress_added",
            Self::ItemFinished => "item_finished",
            Self::ItemReleased => "item_released",
            Self::FeedbackRequested => "feedback_requested",
            Self::CommentAdded => "comment_added",
            Self::LabelAdded => "label_added",
            Self::LabelUpdated => "label_updated",
            Self::LabelDeleted => "label_deleted",
            Self::RelationshipCreated => "relationship_created",
            Self::RelationshipUpdated => "relationship_updated",
            Self::RelationshipDeleted => "relationship_deleted",
        }
    }
}

impl fmt::Display for WorkItemEventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for WorkItemEventType {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "SystemPromptChanged" => Ok(Self::SystemPromptChanged),
            "MemoryChanged" => Ok(Self::MemoryChanged),
            "item_created" => Ok(Self::ItemCreated),
            "item_updated" => Ok(Self::ItemUpdated),
            "item_moved" => Ok(Self::ItemMoved),
            "item_deleted" => Ok(Self::ItemDeleted),
            "item_claimed" => Ok(Self::ItemClaimed),
            "progress_added" => Ok(Self::ProgressAdded),
            "item_finished" => Ok(Self::ItemFinished),
            "item_released" => Ok(Self::ItemReleased),
            "feedback_requested" => Ok(Self::FeedbackRequested),
            "comment_added" => Ok(Self::CommentAdded),
            "label_added" => Ok(Self::LabelAdded),
            "label_updated" => Ok(Self::LabelUpdated),
            "label_deleted" => Ok(Self::LabelDeleted),
            "relationship_created" => Ok(Self::RelationshipCreated),
            "relationship_updated" => Ok(Self::RelationshipUpdated),
            "relationship_deleted" => Ok(Self::RelationshipDeleted),
            _ => Err(ParseEnumError("unknown work item event type")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkItemEventView {
    pub id: i64,
    pub project_id: i64,
    pub work_item_id: Option<i64>,
    pub event_type: WorkItemEventType,
    pub body: String,
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RecoveredClaimView {
    pub item_id: i64,
    pub agent_id: String,
    pub claimed_at: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorType {
    User,
    Agent,
    System,
}

impl AuthorType {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Agent => "agent",
            Self::System => "system",
        }
    }
}

impl fmt::Display for AuthorType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AuthorType {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "user" => Ok(Self::User),
            "agent" => Ok(Self::Agent),
            "system" => Ok(Self::System),
            _ => Err(ParseEnumError(
                "author type must be one of: user, agent, system",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CommentView {
    pub id: i64,
    pub work_item_id: i64,
    pub author_type: AuthorType,
    pub author_name: Option<String>,
    pub body: String,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl AgentRunStatus {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl fmt::Display for AgentRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentRunStatus {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().as_str() {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(ParseEnumError(
                "agent run status must be one of: running, completed, failed, cancelled",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunMutability {
    #[default]
    Mutating,
    ReadOnly,
}

impl AutomationRunMutability {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Mutating => "mutating",
            Self::ReadOnly => "read_only",
        }
    }

    pub fn all() -> [Self; 2] {
        [Self::Mutating, Self::ReadOnly]
    }
}

impl fmt::Display for AutomationRunMutability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AutomationRunMutability {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "mutating" | "mutable" => Ok(Self::Mutating),
            "read_only" | "readonly" => Ok(Self::ReadOnly),
            _ => Err(ParseEnumError(
                "automation run mutability must be one of: mutating, read_only",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentCommitOutcome {
    NotEvaluated,
    NotRequired,
    Committed,
    SkippedNoChanges,
    SkippedNoGitRepo,
    MissingRequired,
    Unknown,
}

impl AgentCommitOutcome {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::NotEvaluated => "not_evaluated",
            Self::NotRequired => "not_required",
            Self::Committed => "committed",
            Self::SkippedNoChanges => "skipped_no_changes",
            Self::SkippedNoGitRepo => "skipped_no_git_repo",
            Self::MissingRequired => "missing_required",
            Self::Unknown => "unknown",
        }
    }
}

impl fmt::Display for AgentCommitOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentCommitOutcome {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "not_evaluated" => Ok(Self::NotEvaluated),
            "not_required" => Ok(Self::NotRequired),
            "committed" => Ok(Self::Committed),
            "skipped_no_changes" => Ok(Self::SkippedNoChanges),
            "skipped_no_git_repo" => Ok(Self::SkippedNoGitRepo),
            "missing_required" => Ok(Self::MissingRequired),
            "unknown" => Ok(Self::Unknown),
            _ => Err(ParseEnumError(
                "commit outcome must be one of: not_evaluated, not_required, committed, skipped_no_changes, skipped_no_git_repo, missing_required, unknown",
            )),
        }
    }
}

/// Lifecycle of an isolated Git worktree after its agent run has ended.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunCleanupStatus {
    /// The run never created an isolated worktree.
    NotApplicable,
    /// Cleanup is required but has not completed yet.
    Pending,
    /// Dispatch removed the worktree successfully.
    Cleaned,
}

impl AgentRunCleanupStatus {
    /// Returns the canonical SQLite and JSON representation.
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::NotApplicable => "not_applicable",
            Self::Pending => "pending",
            Self::Cleaned => "cleaned",
        }
    }
}

impl fmt::Display for AgentRunCleanupStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AgentRunCleanupStatus {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "not_applicable" => Ok(Self::NotApplicable),
            "pending" => Ok(Self::Pending),
            "cleaned" => Ok(Self::Cleaned),
            _ => Err(ParseEnumError(
                "agent run cleanup status must be one of: not_applicable, pending, cleaned",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AgentRunView {
    pub id: i64,
    pub project_id: i64,
    pub work_item_id: Option<i64>,
    pub memory_event_id: Option<i64>,
    pub trigger_id: Option<i64>,
    pub trigger_name: Option<String>,
    #[serde(default)]
    pub trigger_revision_id: Option<i64>,
    #[serde(default)]
    pub personality_revision_id: Option<i64>,
    #[serde(default)]
    pub system_prompt_event_id: Option<i64>,
    pub tool_name: AgentToolName,
    pub mutability: AutomationRunMutability,
    pub status: AgentRunStatus,
    pub command: String,
    pub working_dir: String,
    pub worktree_path: Option<String>,
    pub branch_name: Option<String>,
    pub process_id: Option<i64>,
    pub exit_code: Option<i64>,
    pub log_path: Option<String>,
    pub developer_instructions_path: Option<String>,
    pub user_prompt_path: Option<String>,
    pub agent_model: Option<String>,
    pub agent_reasoning_effort: Option<AgentReasoningEffort>,
    #[serde(default)]
    pub effective_input_sha256: Option<String>,
    #[serde(default)]
    pub effective_timeout_seconds: Option<u64>,
    #[serde(default)]
    pub effective_concurrency_group: Option<String>,
    pub token_usage: Option<AgentRunTokenUsageView>,
    pub commit_required: bool,
    pub commit_outcome: AgentCommitOutcome,
    pub commit_shas: Vec<String>,
    pub pr_requested: bool,
    pub pr_url: Option<String>,
    pub cleanup_status: AgentRunCleanupStatus,
    pub worktree_cleaned_at: Option<String>,
    pub result_summary: String,
    #[serde(default)]
    pub semantic_postcondition_status: SemanticPostconditionStatus,
    #[serde(default)]
    pub semantic_postcondition_failures: Vec<PostconditionFailureView>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AgentRunTokenUsageView {
    pub input_tokens: i64,
    pub cached_input_tokens: i64,
    pub output_tokens: i64,
    pub total_tokens: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunLogView {
    pub run: AgentRunView,
    #[serde(default)]
    pub active: bool,
    pub memory_event: Option<ProjectMemoryEventRefView>,
    pub developer_instructions: Option<String>,
    pub user_prompt: Option<String>,
    pub output: Vec<AgentRunOutputPiece>,
    #[serde(default)]
    pub created_items: Vec<WorkItemSummaryView>,
    #[serde(default)]
    pub modified_items: Vec<WorkItemSummaryView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AgentRunOutputLog {
    pub schema_version: u32,
    pub pieces: Vec<AgentRunOutputPiece>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AgentRunOutputPiece {
    pub sequence: u64,
    pub timestamp: String,
    pub kind: AgentRunOutputKind,
    pub source: String,
    pub item_id: Option<String>,
    pub title: String,
    pub body: String,
    pub metadata: serde_json::Value,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunOutputKind {
    System,
    ModelMessage,
    Reasoning,
    ToolCall,
    FileChange,
    Error,
    Legacy,
}

impl AgentRunOutputKind {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::ModelMessage => "model_message",
            Self::Reasoning => "reasoning",
            Self::ToolCall => "tool_call",
            Self::FileChange => "file_change",
            Self::Error => "error",
            Self::Legacy => "legacy",
        }
    }
}

impl fmt::Display for AgentRunOutputKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectMemoryEventRefView {
    pub event_id: i64,
    pub available: bool,
    pub created_at: Option<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AutomationStatusView {
    pub project: String,
    pub settings: ProjectSettingsView,
    pub running_runs: i64,
    pub running_mutating_runs: i64,
    pub running_read_only_runs: i64,
    pub allowed_mutating_runs: i64,
    pub recent_runs: Vec<AgentRunView>,
    pub tools: Vec<AgentToolView>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationActivation {
    Manual,
    WorkItem,
    Cron,
    WorkItemCreated,
}

impl AutomationActivation {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::Manual => "manual",
            Self::WorkItem => "work_item",
            Self::Cron => "cron",
            Self::WorkItemCreated => "work_item_created",
        }
    }
}

impl fmt::Display for AutomationActivation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AutomationActivation {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "manual" => Ok(Self::Manual),
            "work_item" | "started" | "start" => Ok(Self::WorkItem),
            "cron" => Ok(Self::Cron),
            "work_item_created" => Ok(Self::WorkItemCreated),
            _ => Err(ParseEnumError(
                "automation activation must be one of: manual, work_item, cron, work_item_created",
            )),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEffect {
    ProduceWork,
    ConsumeWork,
}

impl AutomationEffect {
    pub fn as_storage(self) -> &'static str {
        match self {
            Self::ProduceWork => "produce_work",
            Self::ConsumeWork => "consume_work",
        }
    }
}

impl fmt::Display for AutomationEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_storage())
    }
}

impl FromStr for AutomationEffect {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_lowercase().replace('-', "_").as_str() {
            "produce_work" | "produce" | "producer" => Ok(Self::ProduceWork),
            "consume_work" | "consume" | "consumer" => Ok(Self::ConsumeWork),
            _ => Err(ParseEnumError(
                "automation effect must be one of: produce_work, consume_work",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationTriggerView {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub enabled: bool,
    #[serde(alias = "trigger_kind")]
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    pub tool_name: AgentToolName,
    pub mutability: AutomationRunMutability,
    pub personality_id: Option<i64>,
    pub personality_name: Option<String>,
    pub prompt: String,
    #[serde(default, with = "optional_condition")]
    pub work_item_selector: Option<Condition>,
    pub priority: i64,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub produced_work: Option<ProducedWorkSpec>,
    #[serde(default)]
    pub execution: AutomationExecutionPolicy,
    #[serde(default)]
    pub postconditions: Option<AutomationPostconditions>,
    #[serde(default)]
    pub current_revision_id: Option<i64>,
    #[serde(default)]
    pub managed_bundle_key: Option<String>,
    #[serde(default)]
    pub managed_object_key: Option<String>,
    pub evaluation_count: i64,
    pub pending_evaluation_count: i64,
    pub last_evaluation_queued_at: Option<String>,
    pub last_evaluated_at: Option<String>,
    pub next_evaluation_at: Option<String>,
    pub last_event_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

fn default_produced_work_state() -> String {
    DEFAULT_STATE_LABEL.to_owned()
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProducedWorkSpec {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default = "default_produced_work_state")]
    pub state: String,
    #[serde(default)]
    pub initial_labels: Vec<CreateWorkItemLabelRequest>,
    #[serde(default)]
    pub agent_model_override: Option<String>,
    #[serde(default)]
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    #[serde(default)]
    pub deduplication: ProduceDeduplication,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "policy", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProduceDeduplication {
    #[default]
    Always,
    WhileUnfinishedForTrigger,
    WhileUnfinishedForKey {
        key: String,
    },
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationExecutionPolicy {
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<AgentReasoningEffort>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub max_concurrent_runs: Option<u64>,
    #[serde(default)]
    pub concurrency_group: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPostconditions {
    #[serde(default)]
    pub any_of: Vec<AutomationOutcomeSet>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationOutcomeSet {
    #[serde(default)]
    pub disposition: Option<ExpectedDisposition>,
    #[serde(default)]
    pub attributed_events: Vec<WorkItemEventType>,
    #[serde(default)]
    pub labels: Vec<LabelAssertion>,
    #[serde(default)]
    pub created_items: Option<CreatedItemAssertion>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created_item_assertions: Vec<CreatedItemAssertion>,
    #[serde(default)]
    pub created_items_share_group: bool,
    #[serde(default)]
    pub workspace_changes: Option<WorkspaceAssertion>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedDisposition {
    Finished,
    Released,
    FeedbackRequested,
    SuccessfulNonterminal,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LabelAssertion {
    pub assertion: LabelAssertionKind,
    pub key: String,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelAssertionKind {
    Added,
    Removed,
    Present,
    Absent,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreatedItemAssertion {
    #[serde(default)]
    pub count: Option<u64>,
    #[serde(default)]
    pub at_least: Option<u64>,
    #[serde(default)]
    pub at_most: Option<u64>,
    #[serde(default, with = "optional_condition")]
    pub selector: Option<Condition>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkspaceAssertion {
    Any,
    None,
    Required,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticPostconditionStatus {
    #[default]
    NotConfigured,
    Passed,
    Failed,
}

impl SemanticPostconditionStatus {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::NotConfigured => "not_configured",
            Self::Passed => "passed",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for SemanticPostconditionStatus {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "not_configured" => Ok(Self::NotConfigured),
            "passed" => Ok(Self::Passed),
            "failed" => Ok(Self::Failed),
            _ => Err(ParseEnumError(
                "semantic postcondition status must be one of: not_configured, passed, failed",
            )),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PostconditionFailureView {
    pub outcome_index: usize,
    pub assertion: String,
    pub expected: String,
    pub actual: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkItemOriginKind {
    Historical,
    Operator,
    ProducingAutomation,
    AgentRun,
    System,
}

impl WorkItemOriginKind {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Historical => "historical",
            Self::Operator => "operator",
            Self::ProducingAutomation => "producing_automation",
            Self::AgentRun => "agent_run",
            Self::System => "system",
        }
    }
}

impl FromStr for WorkItemOriginKind {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "historical" => Ok(Self::Historical),
            "operator" => Ok(Self::Operator),
            "producing_automation" => Ok(Self::ProducingAutomation),
            "agent_run" => Ok(Self::AgentRun),
            "system" => Ok(Self::System),
            _ => Err(ParseEnumError("unknown work item origin kind")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemOriginView {
    pub kind: WorkItemOriginKind,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub producing_evaluation_id: Option<i64>,
    pub trigger_id: Option<i64>,
    pub trigger_revision_id: Option<i64>,
    pub trigger_name: Option<String>,
    pub bundle_key: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct WorkItemSummaryView {
    pub id: i64,
    pub title: String,
    pub state: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionChangeOperation {
    Create,
    Update,
    Restore,
    Detach,
    BundleApply,
    Migration,
}

impl RevisionChangeOperation {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Restore => "restore",
            Self::Detach => "detach",
            Self::BundleApply => "bundle_apply",
            Self::Migration => "migration",
        }
    }
}

impl FromStr for RevisionChangeOperation {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "create" => Ok(Self::Create),
            "update" => Ok(Self::Update),
            "restore" => Ok(Self::Restore),
            "detach" => Ok(Self::Detach),
            "bundle_apply" => Ok(Self::BundleApply),
            "migration" => Ok(Self::Migration),
            _ => Err(ParseEnumError("unknown revision change operation")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AutomationRevisionView {
    pub id: i64,
    pub trigger_id: Option<i64>,
    pub project_id: i64,
    pub revision_number: i64,
    pub configuration: serde_json::Value,
    pub sha256: String,
    pub operation: RevisionChangeOperation,
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PersonalityRevisionView {
    pub id: i64,
    pub personality_id: Option<i64>,
    pub project_id: i64,
    pub revision_number: i64,
    pub name: String,
    pub personality_description: String,
    pub sha256: String,
    pub operation: RevisionChangeOperation,
    pub actor_type: Option<AuthorType>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationEvaluationOutcome {
    CreatedWork,
    SkippedDuplicate,
    StartedRun,
    Failed,
}

impl AutomationEvaluationOutcome {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::CreatedWork => "created_work",
            Self::SkippedDuplicate => "skipped_duplicate",
            Self::StartedRun => "started_run",
            Self::Failed => "failed",
        }
    }
}

impl FromStr for AutomationEvaluationOutcome {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "created_work" => Ok(Self::CreatedWork),
            "skipped_duplicate" => Ok(Self::SkippedDuplicate),
            "started_run" => Ok(Self::StartedRun),
            "failed" => Ok(Self::Failed),
            _ => Err(ParseEnumError("unknown automation evaluation outcome")),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AutomationEvaluationView {
    pub id: i64,
    pub project_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_revision_id: Option<i64>,
    pub trigger_name: String,
    pub activation_cause: String,
    pub outcome: AutomationEvaluationOutcome,
    pub work_item_id: Option<i64>,
    pub run_id: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkItemSearchRequest {
    #[serde(default)]
    pub states: Vec<String>,
    #[serde(default, with = "optional_condition")]
    pub labels: Option<Condition>,
    #[serde(default, with = "optional_condition")]
    pub selector: Option<Condition>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub finished: Option<bool>,
    #[serde(default)]
    pub created_by_run: Option<i64>,
    #[serde(default)]
    pub produced_by_trigger: Option<i64>,
    #[serde(default)]
    pub relationship_kind: Option<String>,
    #[serde(default)]
    pub updated_since: Option<String>,
    #[serde(default)]
    pub limit: Option<u64>,
    #[serde(default)]
    pub cursor: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WorkItemPage {
    pub items: Vec<WorkItemView>,
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoutingExplainRequest {
    #[serde(default)]
    pub item_id: Option<i64>,
    #[serde(default)]
    pub rule: Option<AutomationRuleInput>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoutingExplanationView {
    pub item_id: Option<i64>,
    pub rules: Vec<RoutingRuleExplanationView>,
    pub winner_trigger_id: Option<i64>,
    pub matching_item_count: Option<u64>,
    pub example_items: Vec<WorkItemSummaryView>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RoutingRuleExplanationView {
    pub trigger_id: Option<i64>,
    pub trigger_name: String,
    pub selector_matches: bool,
    pub clause_results: Vec<SelectorClauseResultView>,
    pub due: bool,
    pub admission_allowed: bool,
    pub fairness_score: i64,
    pub priority: i64,
    pub exclusive: bool,
    pub suppressed_by_exclusive: bool,
    pub blockers: Vec<String>,
    pub would_win: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SelectorClauseResultView {
    pub path: String,
    pub column_name: Option<String>,
    pub matched: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationRuleInput {
    #[serde(default)]
    pub key: Option<String>,
    pub name: String,
    pub enabled: bool,
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    #[serde(default = "default_tool_name")]
    pub tool_name: AgentToolName,
    #[serde(default)]
    pub mutability: AutomationRunMutability,
    #[serde(default)]
    pub personality: Option<String>,
    #[serde(default)]
    pub prompt_markdown: String,
    #[serde(default, with = "optional_condition")]
    pub selector: Option<Condition>,
    #[serde(default)]
    pub priority: i64,
    #[serde(default)]
    pub exclusive: bool,
    #[serde(default)]
    pub produced_work: Option<ProducedWorkSpec>,
    #[serde(default)]
    pub execution: AutomationExecutionPolicy,
    #[serde(default)]
    pub postconditions: Option<AutomationPostconditions>,
}

fn default_tool_name() -> AgentToolName {
    AgentToolName::Codex
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPersonalityInput {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationBundleManifest {
    pub schema_version: u32,
    pub bundle_key: String,
    pub display_name: String,
    #[serde(default)]
    pub personalities: Vec<AutomationPersonalityInput>,
    #[serde(default)]
    pub automations: Vec<AutomationRuleInput>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleDiffOperation {
    Create,
    Update,
    Delete,
    Unchanged,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleObjectDiffView {
    pub object_type: String,
    pub key: String,
    pub name: String,
    pub operation: BundleDiffOperation,
    pub changes: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleDiffView {
    pub bundle_key: String,
    pub display_name: String,
    pub current_hash: Option<String>,
    pub manifest_hash: String,
    pub objects: Vec<BundleObjectDiffView>,
    pub has_deletions: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BundleYamlRequest {
    pub yaml: String,
    #[serde(default)]
    pub expected_current_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleApplyView {
    pub apply_id: i64,
    pub diff: AutomationBundleDiffView,
    pub status: String,
    pub applied_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InstalledAutomationBundleView {
    pub apply_id: i64,
    pub bundle_key: String,
    pub display_name: String,
    pub manifest_hash: String,
    pub automation_count: u64,
    pub personality_count: u64,
    pub installed_at: String,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveAutomationBundleRequest {
    #[serde(default)]
    pub expected_current_hash: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleValidationView {
    pub manifest: AutomationBundleManifest,
    pub manifest_hash: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationBundleExportView {
    pub yaml: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RestoreRevisionRequest {
    pub revision_id: i64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct RevisionAnalyticsView {
    pub revision_id: i64,
    pub run_count: u64,
    pub completed_count: u64,
    pub failed_count: u64,
    pub semantic_passed_count: u64,
    pub semantic_failed_count: u64,
    pub total_duration_seconds: u64,
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub created_item_count: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct TriggerRunOutcome {
    pub trigger_id: i64,
    pub trigger_name: String,
    pub work_item_id: Option<i64>,
    pub work_item: Option<WorkItemView>,
    pub run: Option<AgentRunView>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessSessionView {
    pub run_id: i64,
    pub project_name: String,
    pub tool_name: String,
    pub command: String,
    pub working_dir: String,
    pub process_id: Option<i64>,
    pub output: Vec<AgentRunOutputPiece>,
    pub started_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ApiError {
    pub error: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateWorkItemRequest {
    pub title: String,
    pub description: String,
    pub state: Option<String>,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "labels")]
    pub initial_labels: Vec<CreateWorkItemLabelRequest>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateWorkItemGroupRequest {
    pub key: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AssignWorkItemGroupRequest {
    pub item_ids: Vec<i64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UpdateWorkItemRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub state: Option<String>,
    pub agent_model_override: Option<Option<String>>,
    pub agent_reasoning_effort_override: Option<Option<AgentReasoningEffort>>,
    pub expect_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClaimWorkItemRequest {
    pub agent_id: String,
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ClaimWorkItemResponse {
    pub item: Option<WorkItemView>,
}

impl ClaimWorkItemResponse {
    pub fn claimed(&self) -> bool {
        self.item.is_some()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CreateWorkItemLabelRequest {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct UpdateWorkItemLabelRequest {
    pub key: Option<String>,
    pub value: Option<Option<String>>,
    pub expect_version: Option<i64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteWorkItemLabelResponse {
    pub deleted: bool,
    pub label_id: i64,
    pub work_item: WorkItemView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CreateWorkItemRelationshipRequest {
    pub target_work_item_id: i64,
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateWorkItemRelationshipRequest {
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeleteWorkItemRelationshipResponse {
    pub deleted: bool,
    pub relationship: WorkItemRelationshipView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProgressWorkItemRequest {
    pub agent_id: String,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct FinishWorkItemRequest {
    pub agent_id: String,
    pub report: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReleaseWorkItemRequest {
    pub agent_id: String,
    pub comment: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct RequestFeedbackWorkItemRequest {
    pub agent_id: String,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct UpdateProjectMemoryRequest {
    pub agent_id: String,
    pub agent_run_id: Option<i64>,
    pub body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AddCommentRequest {
    pub author_type: AuthorType,
    pub author_name: Option<String>,
    pub body: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automation_run_mutability_parses_displays_and_serializes() {
        assert_eq!(
            "mutating".parse::<AutomationRunMutability>().unwrap(),
            AutomationRunMutability::Mutating
        );
        assert_eq!(
            "read-only".parse::<AutomationRunMutability>().unwrap(),
            AutomationRunMutability::ReadOnly
        );
        assert_eq!(AutomationRunMutability::ReadOnly.to_string(), "read_only");
        assert_eq!(
            serde_json::to_string(&AutomationRunMutability::ReadOnly).unwrap(),
            r#""read_only""#
        );
        assert!("readonly-ish".parse::<AutomationRunMutability>().is_err());
    }

    #[test]
    fn swim_lane_item_order_has_one_canonical_wire_value() {
        assert_eq!(
            "title-desc".parse::<SwimLaneItemOrder>().unwrap(),
            SwimLaneItemOrder::TitleDesc
        );
        assert_eq!(
            serde_json::to_string(&SwimLaneItemOrder::UpdatedDesc).unwrap(),
            r#""updated_desc""#
        );
        assert!("newest-ish".parse::<SwimLaneItemOrder>().is_err());
    }

    #[test]
    fn work_item_event_type_preserves_historical_storage_names() {
        for (storage, event_type) in [
            (
                "SystemPromptChanged",
                WorkItemEventType::SystemPromptChanged,
            ),
            ("MemoryChanged", WorkItemEventType::MemoryChanged),
            ("item_claimed", WorkItemEventType::ItemClaimed),
            (
                "relationship_deleted",
                WorkItemEventType::RelationshipDeleted,
            ),
        ] {
            assert_eq!(storage.parse::<WorkItemEventType>().unwrap(), event_type);
            assert_eq!(event_type.as_storage(), storage);
            assert_eq!(
                serde_json::to_string(&event_type).unwrap(),
                format!(r#""{storage}""#)
            );
        }
        assert!("item_claimedd".parse::<WorkItemEventType>().is_err());
    }

    #[test]
    fn agent_run_cleanup_status_rejects_unknown_states() {
        assert_eq!(
            "not-applicable".parse::<AgentRunCleanupStatus>().unwrap(),
            AgentRunCleanupStatus::NotApplicable
        );
        assert_eq!(AgentRunCleanupStatus::Cleaned.to_string(), "cleaned");
        assert!("cleanup_failed".parse::<AgentRunCleanupStatus>().is_err());
    }

    #[test]
    fn codex_agent_models_include_gpt_56_matrix() {
        assert_eq!(CodexAgentModel::newest().as_storage(), "gpt-5.6-sol");
        assert_eq!(
            "gpt-5.6-terra".parse::<CodexAgentModel>().unwrap(),
            CodexAgentModel::Gpt56Terra
        );
        assert_eq!(
            "gpt-5.6".parse::<CodexAgentModel>().unwrap(),
            CodexAgentModel::Gpt56
        );
        assert!(CodexAgentModel::Gpt56Sol.supports_reasoning_effort(AgentReasoningEffort::Max));
        assert!(
            !CodexAgentModel::Gpt56Sol.supports_reasoning_effort(AgentReasoningEffort::Minimal)
        );
        assert!(CodexAgentModel::Gpt55.supports_reasoning_effort(AgentReasoningEffort::Minimal));
        assert!(!CodexAgentModel::Gpt55.supports_reasoning_effort(AgentReasoningEffort::Max));
    }

    #[test]
    fn agent_reasoning_effort_accepts_max() {
        assert_eq!(
            "max".parse::<AgentReasoningEffort>().unwrap(),
            AgentReasoningEffort::Max
        );
        assert_eq!(AgentReasoningEffort::highest(), AgentReasoningEffort::Max);
        assert!(
            AgentReasoningEffort::allowed_values()
                .contains("none, minimal, low, medium, high, xhigh, max")
        );
    }

    #[test]
    fn create_work_item_request_defaults_missing_initial_labels() {
        let request: CreateWorkItemRequest = serde_json::from_value(serde_json::json!({
            "title": "Backwards compatible",
            "description": "Older callers do not send labels",
            "state": "open",
            "agent_model_override": null,
            "agent_reasoning_effort_override": null
        }))
        .unwrap();

        assert!(request.initial_labels.is_empty());
    }

    #[test]
    fn create_work_item_request_accepts_labels_alias() {
        let request: CreateWorkItemRequest = serde_json::from_value(serde_json::json!({
            "title": "Alias",
            "description": "Accepts labels as an alias",
            "state": "open",
            "agent_model_override": null,
            "agent_reasoning_effort_override": null,
            "labels": [
                { "key": "type", "value": "feature" },
                { "key": "needs-verification", "value": null }
            ]
        }))
        .unwrap();

        assert_eq!(request.initial_labels.len(), 2);
        assert_eq!(request.initial_labels[0].key, "type");
        assert_eq!(request.initial_labels[0].value.as_deref(), Some("feature"));
        assert_eq!(request.initial_labels[1].key, "needs-verification");
        assert!(request.initial_labels[1].value.is_none());
    }
}
