use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "work_item_states")]
#[ck_field(model = ModelType::Update)]
pub struct WorkItemState {
    pub id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateWorkItemState {
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadWorkItemState {
    pub id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub created_at: String,
    pub updated_at: String,
    pub has_validation_errors: bool,
}

impl From<ReadWorkItemState> for WorkItemState {
    fn from(read: ReadWorkItemState) -> Self {
        Self {
            id: read.id,
            identifier: read.identifier,
            name: read.name,
            position: read.position,
        }
    }
}

impl ErasedIdentifiable for CreateWorkItemState {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
