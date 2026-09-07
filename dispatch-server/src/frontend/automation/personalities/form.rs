use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "personalities")]
#[ck_field(model = ModelType::Update)]
pub struct Personality {
    pub id: i64,
    pub name: String,
    pub personality_description: String,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreatePersonality {
    pub project_id: i64,
    pub name: String,
    pub personality_description: String,
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadPersonality {
    pub id: i64,
    pub project_id: i64,
    pub name: String,
    pub personality_description: String,
    pub current_revision_id: Option<i64>,
    pub managed_bundle_key: Option<String>,
    pub managed_object_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub has_validation_errors: bool,
}

impl From<ReadPersonality> for Personality {
    fn from(read: ReadPersonality) -> Self {
        Self {
            id: read.id,
            name: read.name,
            personality_description: read.personality_description,
        }
    }
}

impl ErasedIdentifiable for CreatePersonality {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
