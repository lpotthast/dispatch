use super::{repository::encode, service::ToolService};
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::agent_tool,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum AgentToolCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for AgentToolCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for AgentToolCrudError {}
pub struct AgentToolCrudRepository {
    service: Arc<ToolService>,
    fallback: SeaOrmRepo,
}
impl AgentToolCrudRepository {
    pub(crate) fn new(db: Arc<sea_orm::DatabaseConnection>, service: Arc<ToolService>) -> Self {
        Self {
            service,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudAgentToolResource> for AgentToolCrudRepository {
    type Error = AgentToolCrudError;
    async fn insert(
        &self,
        model: agent_tool::CreateModel,
    ) -> Result<agent_tool::Model, Self::Error> {
        let tool = model
            .tool_name
            .parse::<dispatch_types::AgentToolName>()
            .map_err(|error| AgentToolCrudError::Internal(error.to_string()))?;
        self.service
            .create(tool, model.executable_path)
            .await
            .map(encode)
            .map_err(|e| AgentToolCrudError::Internal(e.to_string()))
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_tool::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentToolResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentToolCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_tool::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<agent_tool::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentToolResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentToolCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_tool::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<agent_tool::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentToolResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentToolCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_tool::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<agent_tool::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentToolResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentToolCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<agent_tool::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<agent_tool::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAgentToolResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AgentToolCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: agent_tool::Model,
        model: agent_tool::UpdateModel,
    ) -> Result<agent_tool::Model, Self::Error> {
        self.service
            .update(existing.id, model.executable_path)
            .await
            .map(encode)
            .map_err(|e| AgentToolCrudError::Internal(e.to_string()))
    }
    async fn delete(&self, model: agent_tool::Model) -> Result<DeleteResult, Self::Error> {
        self.service
            .delete(model.id)
            .await
            .map(|entities_affected| DeleteResult { entities_affected })
            .map_err(|e| AgentToolCrudError::Internal(e.to_string()))
    }
}
#[derive(Debug, CkResourceContext)]
pub struct AgentToolResourceContext;

#[derive(Debug, ToSchema)]
pub struct CrudAgentToolResource;

impl CrudResource for CrudAgentToolResource {
    type ReadModel = agent_tool::read_view::Model;
    type ReadModelId = agent_tool::read_view::ModelId;
    type ReadModelField = agent_tool::read_view::ModelField;

    type CreateModel = agent_tool::CreateModel;
    type CreateModelField = agent_tool::ModelField;

    type UpdateModel = agent_tool::UpdateModel;
    type UpdateModelField = agent_tool::ModelField;

    type Model = agent_tool::Model;
    type Id = agent_tool::AgentToolId;
    type ModelField = agent_tool::ModelField;

    type Repository = AgentToolCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = AgentToolResourceContext;
    type HookData = ();
    type Lifetime = NoopLifetimeHooks;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::AgentTool;
}

impl SeaOrmResource for CrudAgentToolResource {
    type Entity = agent_tool::Entity;
    type SeaOrmModel = agent_tool::Model;
    type ActiveModel = agent_tool::ActiveModel;
    type Column = agent_tool::Column;
    type PrimaryKey = <agent_tool::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = agent_tool::read_view::Entity;
    type ReadViewSeaOrmModel = agent_tool::read_view::Model;
    type ReadViewActiveModel = agent_tool::read_view::ActiveModel;
    type ReadViewColumn = agent_tool::read_view::Column;
    type ReadViewPrimaryKey = <agent_tool::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <agent_tool::ModelField as CrudColumns<agent_tool::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <agent_tool::read_view::ModelField as CrudColumns<agent_tool::read_view::Column>>::to_sea_orm_column(field)
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::execution::tools::transport::CrudAgentToolResource,
    agent_tool
);
pub(crate) fn routes() -> axum::Router {
    axum_agent_tool_crud_routes::add_crud_routes("/api", axum::Router::new())
}
