use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "agent_tools")]
#[ck_field(model = ModelType::Update)]
pub struct AgentTool {
    pub id: i64,
    pub executable_path: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateAgentTool {
    pub tool_name: String,
    pub executable_path: Option<String>,
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadAgentTool {
    pub id: i64,
    pub tool_name: String,
    pub executable_path: Option<String>,
    pub discovered_path: Option<String>,
    pub last_discovered_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl From<ReadAgentTool> for AgentTool {
    fn from(read: ReadAgentTool) -> Self {
        Self {
            id: read.id,
            executable_path: read.executable_path,
        }
    }
}

impl ErasedIdentifiable for CreateAgentTool {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
