use crudkit_core::condition::Condition;
use dispatch_types::*;
#[derive(Clone, Debug)]
pub struct CreateAutomationTrigger {
    pub name: String,
    pub enabled: bool,
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    pub tool_name: Option<AgentToolName>,
    pub mutability: AutomationRunMutability,
    pub personality_id: Option<i64>,
    pub prompt: String,
    pub work_item_selector: Option<Condition>,
    pub priority: i64,
}

#[derive(Clone, Debug)]
pub struct UpdateAutomationTrigger {
    pub name: String,
    pub enabled: bool,
    pub activation: AutomationActivation,
    pub effect: AutomationEffect,
    pub schedule: String,
    pub mutability: AutomationRunMutability,
    pub personality_id: Option<i64>,
    pub prompt: String,
    pub work_item_selector: Option<Condition>,
    pub priority: Option<i64>,
}

#[derive(Clone, Debug)]
pub(crate) enum PersonalityReference {
    Id(i64),
    Name(String),
}
#[derive(Clone, Debug)]
pub(crate) struct RuleFields {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    pub(crate) activation: AutomationActivation,
    pub(crate) effect: AutomationEffect,
    pub(crate) schedule: String,
    pub(crate) tool_name: Option<AgentToolName>,
    pub(crate) mutability: AutomationRunMutability,
    pub(crate) personality: Option<PersonalityReference>,
    pub(crate) prompt: String,
    pub(crate) selector: Option<Condition>,
    pub(crate) priority: i64,
    pub(crate) exclusive: bool,
    pub(crate) produced_work: Option<ProducedWorkSpec>,
    pub(crate) execution: AutomationExecutionPolicy,
    pub(crate) postconditions: Option<AutomationPostconditions>,
}
impl From<CreateAutomationTrigger> for RuleFields {
    fn from(value: CreateAutomationTrigger) -> Self {
        Self {
            name: value.name,
            enabled: value.enabled,
            activation: value.activation,
            effect: value.effect,
            schedule: value.schedule,
            tool_name: value.tool_name,
            mutability: value.mutability,
            personality: value.personality_id.map(PersonalityReference::Id),
            prompt: value.prompt,
            selector: value.work_item_selector,
            priority: value.priority,
            exclusive: false,
            produced_work: None,
            execution: Default::default(),
            postconditions: None,
        }
    }
}
impl From<&AutomationTriggerView> for RuleFields {
    fn from(value: &AutomationTriggerView) -> Self {
        Self {
            name: value.name.clone(),
            enabled: value.enabled,
            activation: value.activation,
            effect: value.effect,
            schedule: value.schedule.clone(),
            tool_name: Some(value.tool_name),
            mutability: value.mutability,
            personality: value.personality_id.map(PersonalityReference::Id),
            prompt: value.prompt.clone(),
            selector: value.work_item_selector.clone(),
            priority: value.priority,
            exclusive: value.exclusive,
            produced_work: value.produced_work.clone(),
            execution: value.execution.clone(),
            postconditions: value.postconditions.clone(),
        }
    }
}
