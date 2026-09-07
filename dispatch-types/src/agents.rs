use crate::ProjectView;
use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::ParseEnumError;

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
    #[serde(default)]
    pub log_storage: CodexLogStorageStatusView,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexLogStorageStatusView {
    pub threshold_bytes: u64,
    pub oversized_databases: Vec<CodexLogDatabaseView>,
    pub scan_errors: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexLogDatabaseView {
    pub relative_path: String,
    pub size_bytes: u64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CodexLogPurgeResultView {
    pub removed_database_count: u64,
    pub reclaimed_bytes: u64,
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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CodexStatusPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
}
