use super::{model::StateFields, repository::encode, service::StateService};
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::work_item_state,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum WorkItemStateCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for WorkItemStateCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for WorkItemStateCrudError {}
pub struct WorkItemStateCrudRepository {
    service: Arc<StateService>,
    fallback: SeaOrmRepo,
}
impl WorkItemStateCrudRepository {
    pub(crate) fn new(db: Arc<sea_orm::DatabaseConnection>, service: Arc<StateService>) -> Self {
        Self {
            service,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudWorkItemStateResource> for WorkItemStateCrudRepository {
    type Error = WorkItemStateCrudError;
    async fn insert(
        &self,
        model: work_item_state::CreateModel,
    ) -> Result<work_item_state::Model, Self::Error> {
        self.service
            .create(
                model.project_id,
                StateFields {
                    identifier: model.identifier,
                    name: model.name,
                    position: model.position,
                },
            )
            .await
            .and_then(encode)
            .map_err(|e| WorkItemStateCrudError::Internal(e.to_string()))
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item_state::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemStateResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemStateCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item_state::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<work_item_state::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemStateResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemStateCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item_state::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<work_item_state::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemStateResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemStateCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item_state::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<work_item_state::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemStateResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemStateCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item_state::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<work_item_state::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemStateResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemStateCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: work_item_state::Model,
        model: work_item_state::UpdateModel,
    ) -> Result<work_item_state::Model, Self::Error> {
        self.service
            .update(
                existing.project_id,
                existing.id,
                StateFields {
                    identifier: model.identifier,
                    name: model.name,
                    position: model.position,
                },
            )
            .await
            .and_then(encode)
            .map_err(|e| WorkItemStateCrudError::Internal(e.to_string()))
    }
    async fn delete(&self, model: work_item_state::Model) -> Result<DeleteResult, Self::Error> {
        self.service
            .delete(model.project_id, model.id)
            .await
            .map(|entities_affected| DeleteResult { entities_affected })
            .map_err(|e| WorkItemStateCrudError::Internal(e.to_string()))
    }
}
#[derive(Clone)]
pub struct WorkItemStateResourceContext;

impl fmt::Debug for WorkItemStateResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkItemStateResourceContext")
    }
}

impl CrudResourceContext for WorkItemStateResourceContext {}

#[derive(Debug, Clone)]
pub struct WorkItemStateHookError(String);

impl fmt::Display for WorkItemStateHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WorkItemStateHookError {}

#[derive(Debug)]
pub struct WorkItemStateLifetime;

impl CrudLifetime<CrudWorkItemStateResource> for WorkItemStateLifetime {
    type Error = WorkItemStateHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudWorkItemStateResource>,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudWorkItemStateResource>,
        _read_result: &mut ReadResult<CrudWorkItemStateResource>,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut work_item_state::CreateModel,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        create_model.identifier =
            super::policy::normalize_identifier(create_model.identifier.clone())
                .map_err(|err| work_item_state_unprocessable_error(err.to_string()))?;
        create_model.name = super::policy::normalize_name(create_model.name.clone())
            .map_err(|err| work_item_state_unprocessable_error(err.to_string()))?;
        Ok(data)
    }

    async fn after_create(
        _create_model: &work_item_state::CreateModel,
        _model: &work_item_state::Model,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        _existing: &work_item_state::Model,
        update_model: &mut work_item_state::UpdateModel,
        _update_request: &UpdateRequest,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        update_model.identifier =
            super::policy::normalize_identifier(update_model.identifier.clone())
                .map_err(|err| work_item_state_unprocessable_error(err.to_string()))?;
        update_model.name = super::policy::normalize_name(update_model.name.clone())
            .map_err(|err| work_item_state_unprocessable_error(err.to_string()))?;
        Ok(data)
    }

    async fn after_update(
        _update_model: &work_item_state::UpdateModel,
        _model: &work_item_state::Model,
        _update_request: &UpdateRequest,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        _model: &work_item_state::Model,
        _delete_request: &DeleteRequest<CrudWorkItemStateResource>,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_delete(
        _model: &work_item_state::Model,
        _delete_request: &DeleteRequest<CrudWorkItemStateResource>,
        _context: &WorkItemStateResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}

fn work_item_state_unprocessable_error(reason: String) -> HookError<WorkItemStateHookError> {
    HookError::UnprocessableEntity { reason }
}

#[derive(Debug, ToSchema)]
pub struct CrudWorkItemStateResource;

impl CrudResource for CrudWorkItemStateResource {
    type ReadModel = work_item_state::read_view::Model;
    type ReadModelId = work_item_state::read_view::ModelId;
    type ReadModelField = work_item_state::read_view::ModelField;

    type CreateModel = work_item_state::CreateModel;
    type CreateModelField = work_item_state::ModelField;

    type UpdateModel = work_item_state::UpdateModel;
    type UpdateModelField = work_item_state::ModelField;

    type Model = work_item_state::Model;
    type Id = work_item_state::WorkItemStateId;
    type ModelField = work_item_state::ModelField;

    type Repository = WorkItemStateCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = WorkItemStateResourceContext;
    type HookData = ();
    type Lifetime = WorkItemStateLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::WorkItemState;
}

impl SeaOrmResource for CrudWorkItemStateResource {
    type Entity = work_item_state::Entity;
    type SeaOrmModel = work_item_state::Model;
    type ActiveModel = work_item_state::ActiveModel;
    type Column = work_item_state::Column;
    type PrimaryKey = <work_item_state::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = work_item_state::read_view::Entity;
    type ReadViewSeaOrmModel = work_item_state::read_view::Model;
    type ReadViewActiveModel = work_item_state::read_view::ActiveModel;
    type ReadViewColumn = work_item_state::read_view::Column;
    type ReadViewPrimaryKey = <work_item_state::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <work_item_state::ModelField as CrudColumns<work_item_state::Column>>::to_sea_orm_column(
            field,
        )
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <work_item_state::read_view::ModelField as CrudColumns<
            work_item_state::read_view::Column,
        >>::to_sea_orm_column(field)
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::items::states::transport::CrudWorkItemStateResource,
    work_item_state
);
pub(crate) fn routes() -> axum::Router {
    axum_work_item_state_crud_routes::add_crud_routes("/api", axum::Router::new())
}
