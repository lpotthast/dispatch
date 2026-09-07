use crate::{CodexAppServerStatusView, ProjectView};
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{
    AgentReasoningEffort, AgentToolName, ParseEnumError, PostconditionFailureView,
    SemanticPostconditionStatus, WorkItemSummaryView,
};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunKind {
    #[default]
    Task,
    KnowledgeAnswer,
}

/// Optional, forward-compatible classification of why an agent run was launched.
///
/// `run_kind` remains the stable compatibility projection. Rows created before the launch
/// contract migration have no purpose, while knowledge-cycle runs project as `task` and expose
/// this field as `knowledge_cycle`.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunPurposeV1 {
    Ordinary,
    KnowledgeAnswer,
    KnowledgeCycle,
}

/// Immutable item-selection authority persisted before a post-migration run can start.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentRunLaunchTargetView {
    None {
        schema_version: u32,
    },
    NextOpen {
        schema_version: u32,
        state: String,
    },
    Selector {
        schema_version: u32,
        selector_sha256: String,
    },
    Specific {
        schema_version: u32,
        work_item_id: i64,
        expected_version: i64,
    },
}

/// Durable result of resolving an [`AgentRunLaunchTargetView`].
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AgentRunLaunchResolutionView {
    Pending {
        schema_version: u32,
    },
    None {
        schema_version: u32,
    },
    Claimed {
        schema_version: u32,
        work_item_id: i64,
        claimed_version: i64,
    },
    Unavailable {
        schema_version: u32,
        reason: String,
    },
}

impl AgentRunPurposeV1 {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::KnowledgeAnswer => "knowledge_answer",
            Self::KnowledgeCycle => "knowledge_cycle",
        }
    }
}

impl FromStr for AgentRunPurposeV1 {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "ordinary" => Ok(Self::Ordinary),
            "knowledge_answer" => Ok(Self::KnowledgeAnswer),
            "knowledge_cycle" => Ok(Self::KnowledgeCycle),
            _ => Err(ParseEnumError(
                "agent run purpose must be one of: ordinary, knowledge_answer, knowledge_cycle",
            )),
        }
    }
}

impl AgentRunKind {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Task => "task",
            Self::KnowledgeAnswer => "knowledge_answer",
        }
    }
}

impl FromStr for AgentRunKind {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "task" => Ok(Self::Task),
            "knowledge_answer" => Ok(Self::KnowledgeAnswer),
            _ => Err(ParseEnumError(
                "agent run kind must be one of: task, knowledge_answer",
            )),
        }
    }
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
    #[serde(default)]
    pub knowledge_job_id: Option<i64>,
    pub id: i64,
    pub project_id: i64,
    pub work_item_id: Option<i64>,
    #[serde(default)]
    pub run_kind: AgentRunKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<AgentRunPurposeV1>,
    /// Missing only for pre-049 legacy rows. A present `none` differs from a pending target even
    /// though both have a null compatibility `work_item_id`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_target: Option<AgentRunLaunchTargetView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_resolution: Option<AgentRunLaunchResolutionView>,
    #[serde(default)]
    pub knowledge_revision: Option<String>,
    #[serde(default)]
    pub source_baseline_id: Option<i64>,
    #[serde(default)]
    pub source_snapshot_id: Option<String>,
    #[serde(default)]
    pub knowledge_view_sha256: Option<String>,
    #[serde(default)]
    pub input_overlay_sha256: Option<String>,
    #[serde(default)]
    pub source_authority_kind: Option<AgentRunSourceAuthority>,
    #[serde(default)]
    pub source_ref_name: Option<String>,
    #[serde(default)]
    pub source_raw_head: Option<String>,
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
#[serde(rename_all = "snake_case")]
pub enum AgentRunSourceAuthority {
    Canonical,
    ProposalOnly,
}

impl AgentRunSourceAuthority {
    pub const fn as_storage(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::ProposalOnly => "proposal_only",
        }
    }
}

impl FromStr for AgentRunSourceAuthority {
    type Err = ParseEnumError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "canonical" => Ok(Self::Canonical),
            "proposal_only" => Ok(Self::ProposalOnly),
            _ => Err(ParseEnumError(
                "agent run source authority must be one of: canonical, proposal_only",
            )),
        }
    }
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
pub struct RunSummaryView {
    pub run: AgentRunView,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunsSection {
    pub automation_running: bool,
    pub running_runs: i64,
    pub running_mutating_runs: i64,
    pub running_read_only_runs: i64,
    pub runs: Vec<RunSummaryView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunLogPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub project: String,
    pub run_log: RunLogView,
    pub codex_status: CodexAppServerStatusView,
}
