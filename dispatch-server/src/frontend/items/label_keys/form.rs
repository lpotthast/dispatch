use crudkit_leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, CkResource, Serialize, Deserialize)]
#[ck_resource(resource_name = "label_keys")]
#[ck_field(model = ModelType::Update)]
pub struct LabelKey {
    pub id: i64,
    pub key: String,
    pub accent_color: Option<String>,
    pub persistent: bool,
    pub built_in: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, Default, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Create)]
pub struct CreateLabelKey {
    pub project_id: i64,
    pub key: String,
    pub accent_color: Option<String>,
    pub persistent: bool,
}

#[derive(Clone, PartialEq, Eq, Debug, CkId, CkField, Serialize, Deserialize)]
#[ck_field(model = ModelType::Read)]
pub struct ReadLabelKey {
    pub id: i64,
    pub project_id: i64,
    pub key: String,
    pub accent_color: Option<String>,
    pub persistent: bool,
    pub built_in: bool,
    pub usage_count: i64,
    pub last_used_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub has_validation_errors: bool,
}

impl From<ReadLabelKey> for LabelKey {
    fn from(read: ReadLabelKey) -> Self {
        Self {
            id: read.id,
            key: read.key,
            accent_color: read.accent_color,
            persistent: read.persistent,
            built_in: read.built_in,
        }
    }
}

impl ErasedIdentifiable for CreateLabelKey {
    fn id(&self) -> SerializableId {
        panic!("create models are not identifiable")
    }
}
