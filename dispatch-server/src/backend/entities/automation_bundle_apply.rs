use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type AutomationBundleApply = Entity;
pub type AutomationBundleApplyActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "automation_bundle_applies")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i64,
    pub bundle_key: String,
    pub display_name: String,
    pub manifest_hash: String,
    pub applied_diff_json: String,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
