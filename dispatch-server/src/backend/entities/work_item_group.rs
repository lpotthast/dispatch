use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type WorkItemGroup = Entity;
pub type WorkItemGroupActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "work_item_groups")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i64,
    pub group_key: String,
    pub name: String,
    pub actor_id: Option<String>,
    pub agent_run_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
