use crate::backend::runs::{launch::model::AgentLaunchTargetV1, model::AutomationTriggerOrigin};
use crudkit_core::condition::Condition;
use dispatch_types::{
    AgentToolName, AutomationExecutionPolicy, AutomationPostconditions, AutomationRunMutability,
};
#[derive(Clone, Debug)]
pub struct StartAutomation {
    pub tool: Option<AgentToolName>,
    /// Immutable persisted authority for item selection. There is deliberately no implicit
    /// meaning attached to a missing compatibility `agent_runs.work_item_id`.
    pub launch_target: AgentLaunchTargetV1,
    pub work_item_selector: Option<Condition>,
    pub extra_prompt: Option<String>,
    pub mutability: Option<AutomationRunMutability>,
    pub personality_id: Option<i64>,
    pub trigger: Option<AutomationTriggerOrigin>,
    pub execution: AutomationExecutionPolicy,
    pub postconditions: Option<AutomationPostconditions>,
}
