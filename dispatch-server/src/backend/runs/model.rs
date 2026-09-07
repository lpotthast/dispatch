use crate::backend::{
    execution::workspaces::runs::WorkspacePlan, runs::launch::model::AgentLaunchTargetV1,
};
use dispatch_types::*;
#[derive(Clone, Debug)]
pub(crate) struct AutomationTriggerOrigin {
    pub trigger_id: i64,
    pub trigger_name: String,
    pub trigger_revision_id: Option<i64>,
}
pub(crate) struct LaunchDetails {
    pub(crate) work_item_id: Option<i64>,
    pub(crate) command: String,
    pub(crate) workspace: WorkspacePlan,
    pub(crate) developer_instructions_path: Option<String>,
    pub(crate) user_prompt_path: Option<String>,
    pub(crate) log_path: Option<String>,
    pub(crate) agent_model: Option<String>,
    pub(crate) agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub(crate) commit_required: bool,
    pub(crate) pr_requested: bool,
    pub(crate) system_prompt_event_id: Option<i64>,
    pub(crate) effective_input_sha256: String,
    pub(crate) effective_timeout_seconds: u64,
}
pub(crate) struct CreateRunConfig<'a> {
    pub(crate) tool: AgentToolName,
    pub(crate) mutability: AutomationRunMutability,
    pub(crate) trigger: Option<&'a AutomationTriggerOrigin>,
    pub(crate) personality_revision_id: Option<i64>,
    pub(crate) effective_timeout_seconds: u64,
    pub(crate) effective_concurrency_group: Option<&'a str>,
    pub(crate) run_kind: AgentRunKind,
    pub(crate) purpose: AgentRunPurposeV1,
    pub(crate) knowledge_job_id: Option<i64>,
    pub(crate) launch_target: &'a AgentLaunchTargetV1,
}
pub(crate) enum RunChange<'a> {
    Launch(Box<LaunchDetails>),
    Finish {
        status: AgentRunStatus,
        exit_code: Option<i64>,
        result_summary: String,
    },
    ProcessId(Option<i64>),
    TokenUsage(AgentRunTokenUsageView),
    Commit(&'a crate::backend::automation::launch::commit::CommitOutcomeEvaluation),
    Semantics(&'a crate::backend::automation::postconditions::model::SemanticEvaluation),
    PullRequest(Option<String>),
    Cleanup {
        cleanup_status: AgentRunCleanupStatus,
        worktree_cleaned_at: Option<String>,
    },
}

pub(crate) struct RunArtifactLocations {
    pub(crate) id: i64,
    pub(crate) branch_name: Option<String>,
    pub(crate) worktree_path: Option<String>,
}
impl From<&dispatch_types::AgentRunView> for RunArtifactLocations {
    fn from(run: &dispatch_types::AgentRunView) -> Self {
        Self {
            id: run.id,
            branch_name: run.branch_name.clone(),
            worktree_path: run.worktree_path.clone(),
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct RunMetadata {
    pub(crate) work_item_id: Option<i64>,
    pub(crate) tool: dispatch_types::AgentToolName,
}
