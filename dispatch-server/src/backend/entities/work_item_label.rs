use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CkField, CkId, CkSeaOrmBridge, CkSeaOrmCreateModel, CkSeaOrmUpdateModel};
use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub type WorkItemLabel = Entity;
pub type WorkItemLabelModel = Model;
pub type WorkItemLabelActiveModel = ActiveModel;

#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    sea_orm::DeriveEntityModel,
    CkId,
    CkField,
    CkSeaOrmBridge,
    CkSeaOrmCreateModel,
    CkSeaOrmUpdateModel,
    crudkit_sea_orm::ReadView,
    ToSchema,
    Serialize,
    Deserialize,
)]
#[sea_orm(table_name = "work_item_labels")]
#[read_view(table_name = "work_item_labels_read_view")]
pub struct Model {
    #[sea_orm(primary_key)]
    #[serde(skip_deserializing)]
    #[ck_create_model(exclude)]
    #[ck_update_model(exclude)]
    pub id: i64,

    #[ck_update_model(exclude)]
    pub project_id: i64,

    #[ck_update_model(exclude)]
    pub work_item_id: i64,

    #[sea_orm(column_name = "label_key")]
    pub key: String,

    #[sea_orm(column_name = "label_value")]
    pub value: Option<String>,

    #[ck_create_model(exclude)]
    #[ck_update_model(exclude)]
    pub created_at: String,

    #[ck_create_model(exclude)]
    #[ck_update_model(exclude)]
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
