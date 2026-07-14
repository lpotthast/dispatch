use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type AutomationTriggerRevision = Entity;
pub type AutomationTriggerRevisionActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "automation_trigger_revisions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub trigger_id: Option<i64>,
    pub project_id: i64,
    pub trigger_name: String,
    pub revision_number: i64,
    pub configuration_json: String,
    pub sha256: String,
    pub change_operation: String,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
