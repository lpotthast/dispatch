use dispatch_types::{AgentReasoningEffort, CreateWorkItemLabelRequest, WorkItemOriginKind};
#[derive(Clone, Debug)]
pub(crate) struct InsertWorkItemOrigin {
    pub(crate) kind: WorkItemOriginKind,
    pub(crate) actor_id: Option<String>,
    pub(crate) agent_run_id: Option<i64>,
    pub(crate) producing_evaluation_id: Option<i64>,
    pub(crate) trigger_id: Option<i64>,
    pub(crate) trigger_revision_id: Option<i64>,
    pub(crate) trigger_name: Option<String>,
    pub(crate) bundle_key: Option<String>,
    pub(crate) deduplication_key: Option<String>,
}

impl Default for InsertWorkItemOrigin {
    fn default() -> Self {
        Self {
            kind: WorkItemOriginKind::Operator,
            actor_id: None,
            agent_run_id: None,
            producing_evaluation_id: None,
            trigger_id: None,
            trigger_revision_id: None,
            trigger_name: None,
            bundle_key: None,
            deduplication_key: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreateWorkItem {
    pub title: String,
    pub description: String,
    pub state: String,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    pub initial_labels: Vec<CreateWorkItemLabelRequest>,
}
