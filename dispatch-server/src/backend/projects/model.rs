use dispatch_types::AgentReasoningEffort;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct CreateProject {
    pub name: String,
    pub display_name: Option<String>,
    pub path: PathBuf,
    pub default_agent_model: Option<String>,
    pub default_agent_reasoning_effort: Option<AgentReasoningEffort>,
    pub system_prompt: Option<String>,
    pub memory: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct UpdateProject {
    pub display_name: Option<String>,
    pub path: Option<ProjectPathUpdate>,
}

#[derive(Clone, Debug)]
pub enum ProjectChangeSource {
    User,
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectScope {
    pub(crate) id: i64,
    pub(crate) name: String,
    pub(crate) path: Option<String>,
}

#[derive(Clone, Debug)]
pub enum ProjectPathUpdate {
    Set(PathBuf),
    Clear,
}

#[derive(Clone, Copy)]
pub(crate) enum ProjectReference<'a> {
    Name(&'a str),
    Id(i64),
}
