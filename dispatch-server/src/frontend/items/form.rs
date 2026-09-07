use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "work_items")]
#[ck_field(model = ModelType::Update)]
pub struct WorkItem {
    pub id: i64,
    pub title: String,
    pub description: String,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateWorkItem {
    pub project_id: i64,
    pub title: String,
    pub description: String,
    pub state: String,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<String>,
    pub initial_labels: String,
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadWorkItem {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub description: String,
    pub claimed_by: Option<String>,
    pub claimed_at: Option<String>,
    pub claim_expires_at: Option<String>,
    pub finished_at: Option<String>,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<String>,
    pub version: i64,
    pub created_at: String,
    pub updated_at: String,
    pub state_label: Option<String>,
    pub has_validation_errors: bool,
}

impl From<ReadWorkItem> for WorkItem {
    fn from(read: ReadWorkItem) -> Self {
        Self {
            id: read.id,
            title: read.title,
            description: read.description,
            agent_model_override: read.agent_model_override,
            agent_reasoning_effort_override: read.agent_reasoning_effort_override,
        }
    }
}

impl ErasedIdentifiable for CreateWorkItem {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
