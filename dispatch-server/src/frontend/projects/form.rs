use crate::shared::view_models::{AgentReasoningEffort, CodexAgentModel};

use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "projects")]
#[ck_field(model = ModelType::Update)]
pub struct Project {
    pub id: i64,
    pub display_name: String,
    pub path: String,
    pub knowledge_directory: String,
    pub workspace_mode: String,
    pub max_code_edit_agents: i64,
    pub max_read_only_agents: i64,
    pub create_pr: bool,
    pub auto_commit: bool,
    pub commit_standard: String,
    pub revert_strategy: String,
    pub stale_claim_minutes: i64,
    pub worktree_cleanup_policy: String,
    pub default_agent_tool: String,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<String>,
    pub agent_sandbox_mode: String,
    pub agent_extra_writable_roots: String,
    pub agent_git_command_policy: String,
}

#[derive(Clone, PartialEq, Eq, Debug, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateProject {
    pub name: String,
    pub display_name: String,
    pub path: String,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<String>,
}

impl Default for CreateProject {
    fn default() -> Self {
        Self {
            name: String::new(),
            display_name: String::new(),
            path: String::new(),
            default_agent_model: Some(CodexAgentModel::newest().as_storage().to_owned()),
            default_agent_reasoning_effort: Some(
                AgentReasoningEffort::highest().as_storage().to_owned(),
            ),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadProject {
    pub id: i64,
    pub name: String,
    pub display_name: String,
    pub path: Option<String>,
    pub knowledge_directory: String,
    pub path_exists: bool,
    pub path_checked_at: Option<String>,
    pub system_prompt: String,
    pub workspace_mode: String,
    pub max_code_edit_agents: i64,
    pub max_read_only_agents: i64,
    pub create_pr: bool,
    pub auto_commit: bool,
    pub commit_standard: String,
    pub revert_strategy: String,
    pub stale_claim_minutes: i64,
    pub worktree_cleanup_policy: String,
    pub default_agent_tool: String,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<String>,
    pub agent_sandbox_mode: String,
    pub agent_extra_writable_roots: String,
    pub agent_git_command_policy: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ReadProject> for Project {
    fn from(read: ReadProject) -> Self {
        Self {
            id: read.id,
            display_name: read.display_name,
            path: read.path.unwrap_or_default(),
            knowledge_directory: read.knowledge_directory,
            workspace_mode: read.workspace_mode,
            max_code_edit_agents: read.max_code_edit_agents,
            max_read_only_agents: read.max_read_only_agents,
            create_pr: read.create_pr,
            auto_commit: read.auto_commit,
            commit_standard: read.commit_standard,
            revert_strategy: read.revert_strategy,
            stale_claim_minutes: read.stale_claim_minutes,
            worktree_cleanup_policy: read.worktree_cleanup_policy,
            default_agent_tool: read.default_agent_tool,
            default_agent_model: read.default_agent_model,
            default_agent_reasoning_effort: read.default_agent_reasoning_effort,
            agent_sandbox_mode: read.agent_sandbox_mode,
            agent_extra_writable_roots: read.agent_extra_writable_roots,
            agent_git_command_policy: read.agent_git_command_policy,
        }
    }
}

impl ErasedIdentifiable for CreateProject {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
