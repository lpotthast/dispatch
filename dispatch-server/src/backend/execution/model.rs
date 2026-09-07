use crate::backend::{
    automation::launch::prompt::AutomationPrompt, execution::git::GitRuntimeFiles,
};
use dispatch_types::{
    AgentReasoningEffort, AgentRunOutputPiece, AgentRunTokenUsageView, AgentSandboxMode,
    AgentToolName, AutomationRunMutability,
};
use std::{collections::HashMap, path::PathBuf, time::Duration};
#[derive(Debug)]
pub(crate) struct AgentProcessOutput {
    pub(crate) process_id: Option<i64>,
    pub(crate) output: Vec<AgentRunOutputPiece>,
    pub(crate) final_response: String,
    pub(crate) token_usage: Option<AgentRunTokenUsageView>,
}
pub(crate) struct AgentProcessStart {
    pub(crate) process_record: Option<PathBuf>,
    pub(crate) incremental_log_dir: Option<PathBuf>,
    pub(crate) run_id: i64,
    pub(crate) project_id: i64,
    pub(crate) project_name: String,
    pub(crate) tool_name: AgentToolName,
    pub(crate) codex_binary: PathBuf,
    pub(crate) codex_home: PathBuf,
    pub(crate) codex_stderr_path: PathBuf,
    pub(crate) dispatch_binary: PathBuf,
    pub(crate) prompt: AutomationPrompt,
    pub(crate) working_dir: PathBuf,
    pub(crate) git_runtime: GitRuntimeFiles,
    pub(crate) real_git_path: PathBuf,
    pub(crate) agent_id: String,
    pub(crate) claimed_item_id: Option<i64>,
    pub(crate) agent_model: Option<String>,
    pub(crate) agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub(crate) agent_sandbox_mode: AgentSandboxMode,
    pub(crate) agent_extra_writable_roots: Vec<String>,
    pub(crate) mutability: AutomationRunMutability,
    pub(crate) timeout: Duration,
    pub(crate) environment: Option<HashMap<String, String>>,
}
