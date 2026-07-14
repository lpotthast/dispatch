use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type WorkItemOrigin = Entity;
pub type WorkItemOriginActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "work_item_origins")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub work_item_id: i64,
    pub project_id: i64,
    pub origin_kind: String,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub producing_evaluation_id: Option<i64>,
    pub trigger_id: Option<i64>,
    pub trigger_revision_id: Option<i64>,
    pub trigger_name: Option<String>,
    pub bundle_key: Option<String>,
    pub deduplication_key: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
