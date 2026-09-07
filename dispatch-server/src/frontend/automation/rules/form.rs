use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "automation_triggers")]
#[ck_field(model = ModelType::Update)]
pub struct AutomationTrigger {
    pub id: i64,
    pub name: String,
    pub enabled: bool,
    pub activation: String,
    pub effect: String,
    pub schedule: String,
    pub tool_name: String,
    pub mutability: String,
    pub personality_id: Option<i64>,
    pub prompt: String,
    pub work_item_selector: Option<String>,
    pub priority: i64,
    pub exclusive: bool,
    pub produced_work_spec_json: Option<String>,
    pub postconditions_json: Option<String>,
    pub model_override: Option<String>,
    pub reasoning_effort_override: Option<String>,
    pub timeout_seconds: Option<i64>,
    pub max_concurrent_runs: Option<i64>,
    pub concurrency_group: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateAutomationTrigger {
    pub project_id: i64,
    pub name: String,
    pub enabled: bool,
    pub activation: String,
    pub effect: String,
    pub schedule: String,
    pub tool_name: String,
    pub mutability: String,
    pub personality_id: Option<i64>,
    pub prompt: String,
    pub work_item_selector: Option<String>,
    pub priority: i64,
    pub exclusive: bool,
    pub produced_work_spec_json: Option<String>,
    pub postconditions_json: Option<String>,
    pub model_override: Option<String>,
    pub reasoning_effort_override: Option<String>,
    pub timeout_seconds: Option<i64>,
    pub max_concurrent_runs: Option<i64>,
    pub concurrency_group: Option<String>,
}

impl Default for CreateAutomationTrigger {
    fn default() -> Self {
        Self {
                project_id: 0,
                name: String::new(),
                enabled: true,
                activation: "work_item".to_owned(),
                effect: "consume_work".to_owned(),
                schedule: "@every 10s".to_owned(),
                tool_name: "codex".to_owned(),
                mutability: "mutating".to_owned(),
                personality_id: None,
                prompt: String::new(),
                work_item_selector: Some(
                    r#"{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}},{"column_name":"needs-refinement","operator":"=","value":{"Bool":false}},{"column_name":"needs-verification","operator":"=","value":{"Bool":false}}]}"#
                        .to_owned(),
                ),
                priority: 0,
                exclusive: false,
                produced_work_spec_json: None,
                postconditions_json: None,
                model_override: None,
                reasoning_effort_override: None,
                timeout_seconds: None,
                max_concurrent_runs: None,
                concurrency_group: None,
            }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadAutomationTrigger {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub enabled: bool,
    pub activation: String,
    pub effect: String,
    pub schedule: String,
    pub tool_name: String,
    pub mutability: String,
    pub personality_id: Option<i64>,
    pub personality_name: Option<String>,
    pub prompt: String,
    pub work_item_selector: Option<String>,
    pub priority: i64,
    pub exclusive: bool,
    pub produced_work_spec_json: Option<String>,
    pub postconditions_json: Option<String>,
    pub model_override: Option<String>,
    pub reasoning_effort_override: Option<String>,
    pub timeout_seconds: Option<i64>,
    pub max_concurrent_runs: Option<i64>,
    pub concurrency_group: Option<String>,
    pub current_revision_id: Option<i64>,
    pub managed_bundle_key: Option<String>,
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

impl From<ReadAutomationTrigger> for AutomationTrigger {
    fn from(read: ReadAutomationTrigger) -> Self {
        Self {
            id: read.id,
            name: read.name,
            enabled: read.enabled,
            activation: read.activation,
            effect: read.effect,
            schedule: read.schedule,
            tool_name: read.tool_name,
            mutability: read.mutability,
            personality_id: read.personality_id,
            prompt: read.prompt,
            work_item_selector: read.work_item_selector,
            priority: read.priority,
            exclusive: read.exclusive,
            produced_work_spec_json: read.produced_work_spec_json,
            postconditions_json: read.postconditions_json,
            model_override: read.model_override,
            reasoning_effort_override: read.reasoning_effort_override,
            timeout_seconds: read.timeout_seconds,
            max_concurrent_runs: read.max_concurrent_runs,
            concurrency_group: read.concurrency_group,
        }
    }
}

impl ErasedIdentifiable for CreateAutomationTrigger {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
