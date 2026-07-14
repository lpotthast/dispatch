use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type PersonalityRevision = Entity;
pub type PersonalityRevisionActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "personality_revisions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub personality_id: Option<i64>,
    pub project_id: i64,
    pub personality_name: String,
    pub revision_number: i64,
    pub personality_description: String,
    pub sha256: String,
    pub change_operation: String,
    pub actor_type: Option<String>,
    pub actor_id: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
