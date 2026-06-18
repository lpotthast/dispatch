use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type WorkItemRelationship = Entity;
pub type WorkItemRelationshipModel = Model;
pub type WorkItemRelationshipActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "work_item_relationships")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,

    pub project_id: i64,

    pub source_work_item_id: i64,

    pub target_work_item_id: i64,

    pub kind: String,

    pub created_at: String,

    pub updated_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
