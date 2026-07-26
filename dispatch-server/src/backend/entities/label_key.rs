use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CkField, CkId, CkSeaOrmBridge, CkSeaOrmCreateModel, CkSeaOrmUpdateModel};
use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub type LabelKey = Entity;
pub type LabelKeyModel = Model;
pub type LabelKeyActiveModel = ActiveModel;
pub type LabelKeyId = ModelId;

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
    ToSchema,
    Serialize,
    Deserialize,
)]
#[sea_orm(table_name = "label_keys")]
pub struct Model {
    #[sea_orm(primary_key)]
    #[serde(skip_deserializing)]
    #[ck_create_model(exclude)]
    #[ck_update_model(exclude)]
    pub id: i64,

    #[ck_update_model(exclude)]
    pub project_id: i64,

    #[sea_orm(column_name = "label_key")]
    #[ck_update_model(exclude)]
    pub key: String,

    pub accent_color: Option<String>,

    pub persistent: bool,

    #[ck_create_model(exclude)]
    #[ck_update_model(exclude)]
    pub built_in: bool,

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

pub mod read_view {
    use crudkit_rs::prelude::*;
    use crudkit_sea_orm::{CkField, CkId, CkSeaOrmBridge};
    use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};
    use serde::{Deserialize, Serialize};
    use utoipa::ToSchema;

    #[derive(
        Clone,
        Debug,
        PartialEq,
        Eq,
        sea_orm::DeriveEntityModel,
        CkId,
        CkField,
        CkSeaOrmBridge,
        ToSchema,
        Serialize,
        Deserialize,
    )]
    #[sea_orm(table_name = "label_keys_read_view")]
    pub struct Model {
        #[sea_orm(primary_key)]
        #[serde(skip_deserializing)]
        pub id: i64,

        pub project_id: i64,

        #[sea_orm(column_name = "label_key")]
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

    #[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
    pub enum Relation {}

    impl sea_orm::ActiveModelBehavior for ActiveModel {}
}
