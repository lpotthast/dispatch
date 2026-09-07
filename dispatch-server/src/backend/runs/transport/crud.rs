use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::agent_run,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::{lifetime::NoopLifetimeHooks, prelude::*};
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum AgentRunCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for AgentRunCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for AgentRunCrudError {}
pub struct AgentRunCrudRepository {
    service: Arc<crate::backend::runs::service::RunService>,
    administration: Arc<crate::backend::runs::administration::RunAdministrationService>,
    fallback: SeaOrmRepo,
}
impl AgentRunCrudRepository {
    pub(crate) fn new(
        db: Arc<sea_orm::DatabaseConnection>,
        service: Arc<crate::backend::runs::service::RunService>,
        administration: Arc<crate::backend::runs::administration::RunAdministrationService>,
    ) -> Self {
        Self {
            service,
            administration,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudAgentRunResource> for AgentRunCrudRepository {
    type Error = AgentRunCrudError;
    async fn insert(&self, model: agent_run::CreateModel) -> Result<agent_run::Model, Self::Error> {
        let tool = model
            .tool_name
            .parse::<dispatch_types::AgentToolName>()
            .map_err(|error| AgentRunCrudError::Internal(error.to_string()))?;
        match self
            .service
            .create_administrative(
                model.project_id,
                crate::backend::runs::model::RunMetadata {
                    work_item_id: model.work_item_id,
                    tool,
                },
            )
            .await
            .map_err(|e| AgentRunCrudError::Internal(e.to_string()))? {}
    }

    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_run::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentRunResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentRunCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_run::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<agent_run::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentRunResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentRunCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_run::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<agent_run::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentRunResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentRunCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_run::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<agent_run::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentRunResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentRunCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_run::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<agent_run::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentRunResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentRunCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: agent_run::Model,
        model: agent_run::UpdateModel,
    ) -> Result<agent_run::Model, Self::Error> {
        let tool = model
            .tool_name
            .parse::<dispatch_types::AgentToolName>()
            .map_err(|e| AgentRunCrudError::Internal(e.to_string()))?;
        self.service
            .update_metadata(
                existing.project_id,
                existing.id,
                crate::backend::runs::model::RunMetadata {
                    work_item_id: model.work_item_id,
                    tool,
                },
            )
            .await
            .map_err(|e| AgentRunCrudError::Internal(e.to_string()))?;
        use crudkit_core::condition::{
            ConditionClause, ConditionClauseValue, ConditionElement, Operator,
        };
        let condition = Condition::All(vec![ConditionElement::Clause(ConditionClause {
            column_name: "id".into(),
            operator: Operator::Equal,
            value: ConditionClauseValue::I64(existing.id),
        })]);
        self.fetch_one(None, None, None, Some(&condition))
            .await?
            .ok_or_else(|| AgentRunCrudError::Internal("run no longer exists".into()))
    }
    async fn delete(&self, model: agent_run::Model) -> Result<DeleteResult, Self::Error> {
        self.administration
            .delete(model.project_id, model.id)
            .await
            .map(|entities_affected| DeleteResult { entities_affected })
            .map_err(|e| AgentRunCrudError::Internal(e.to_string()))
    }
}

#[derive(Debug, CkResourceContext)]
pub struct AgentRunResourceContext;

#[derive(Debug, ToSchema)]
pub struct CrudAgentRunResource;

impl CrudResource for CrudAgentRunResource {
    type ReadModel = agent_run::read_view::Model;
    type ReadModelId = agent_run::read_view::ModelId;
    type ReadModelField = agent_run::read_view::ModelField;

    type CreateModel = agent_run::CreateModel;
    type CreateModelField = agent_run::ModelField;

    type UpdateModel = agent_run::UpdateModel;
    type UpdateModelField = agent_run::ModelField;

    type Model = agent_run::Model;
    type Id = agent_run::AgentRunId;
    type ModelField = agent_run::ModelField;

    type Repository = AgentRunCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = AgentRunResourceContext;
    type HookData = ();
    type Lifetime = NoopLifetimeHooks;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::AgentRun;
}

impl SeaOrmResource for CrudAgentRunResource {
    type Entity = agent_run::Entity;
    type SeaOrmModel = agent_run::Model;
    type ActiveModel = agent_run::ActiveModel;
    type Column = agent_run::Column;
    type PrimaryKey = <agent_run::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = agent_run::read_view::Entity;
    type ReadViewSeaOrmModel = agent_run::read_view::Model;
    type ReadViewActiveModel = agent_run::read_view::ActiveModel;
    type ReadViewColumn = agent_run::read_view::Column;
    type ReadViewPrimaryKey = <agent_run::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <agent_run::ModelField as CrudColumns<agent_run::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <agent_run::read_view::ModelField as CrudColumns<agent_run::read_view::Column>>::to_sea_orm_column(field)
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::runs::transport::crud::CrudAgentRunResource,
    agent_run
);
pub(crate) fn routes() -> axum::Router {
    axum_agent_run_crud_routes::add_crud_routes("/api", axum::Router::new())
}
