use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "swim_lanes")]
#[ck_field(model = ModelType::Update)]
pub struct SwimLane {
    pub id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub filter: String,
    pub item_order: String,
    pub can_create_items: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateSwimLane {
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub filter: String,
    pub item_order: String,
    pub can_create_items: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadSwimLane {
    pub id: i64,
    pub project_id: i64,
    pub identifier: String,
    pub name: String,
    pub position: i64,
    pub filter: String,
    pub item_order: String,
    pub can_create_items: bool,
    pub created_at: String,
    pub updated_at: String,
    pub has_validation_errors: bool,
}

impl From<ReadSwimLane> for SwimLane {
    fn from(read: ReadSwimLane) -> Self {
        Self {
            id: read.id,
            identifier: read.identifier,
            name: read.name,
            position: read.position,
            filter: read.filter,
            item_order: read.item_order,
            can_create_items: read.can_create_items,
        }
    }
}

impl ErasedIdentifiable for CreateSwimLane {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
