use sea_orm::{DerivePrimaryKey, EntityTrait, EnumIter, PrimaryKeyTrait};

pub type AutomationEvaluation = Entity;
pub type AutomationEvaluationActiveModel = ActiveModel;

#[derive(Clone, Debug, PartialEq, Eq, sea_orm::DeriveEntityModel)]
#[sea_orm(table_name = "automation_evaluations")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub project_id: i64,
    pub trigger_id: Option<i64>,
    pub trigger_revision_id: Option<i64>,
    pub trigger_name: String,
    pub activation_cause: String,
    pub outcome: String,
    pub work_item_id: Option<i64>,
    pub run_id: Option<i64>,
    pub error: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, sea_orm::DeriveRelation)]
pub enum Relation {}

impl sea_orm::ActiveModelBehavior for ActiveModel {}
