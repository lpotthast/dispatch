use super::{model::LaneFields, repository::encode, service::LaneService};
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::swim_lane,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum SwimLaneCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for SwimLaneCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for SwimLaneCrudError {}
pub struct SwimLaneCrudRepository {
    service: Arc<LaneService>,
    fallback: SeaOrmRepo,
}
impl SwimLaneCrudRepository {
    pub(crate) fn new(db: Arc<sea_orm::DatabaseConnection>, service: Arc<LaneService>) -> Self {
        Self {
            service,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudSwimLaneResource> for SwimLaneCrudRepository {
    type Error = SwimLaneCrudError;
    async fn insert(&self, model: swim_lane::CreateModel) -> Result<swim_lane::Model, Self::Error> {
        self.service
            .create(
                model.project_id,
                LaneFields {
                    identifier: model.identifier,
                    name: model.name,
                    position: model.position,
                    filter: parse_filter(&model.filter)
                        .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))?,
                    item_order: super::policy::parse_item_order(&model.item_order)
                        .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))?,
                    can_create_items: model.can_create_items,
                },
            )
            .await
            .and_then(encode)
            .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<swim_lane::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudSwimLaneResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(SwimLaneCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<swim_lane::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<swim_lane::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudSwimLaneResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(SwimLaneCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<swim_lane::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<swim_lane::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudSwimLaneResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(SwimLaneCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<swim_lane::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<swim_lane::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudSwimLaneResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(SwimLaneCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<swim_lane::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<swim_lane::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudSwimLaneResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(SwimLaneCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: swim_lane::Model,
        model: swim_lane::UpdateModel,
    ) -> Result<swim_lane::Model, Self::Error> {
        self.service
            .update(
                existing.project_id,
                existing.id,
                LaneFields {
                    identifier: model.identifier,
                    name: model.name,
                    position: model.position,
                    filter: parse_filter(&model.filter)
                        .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))?,
                    item_order: super::policy::parse_item_order(&model.item_order)
                        .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))?,
                    can_create_items: model.can_create_items,
                },
            )
            .await
            .and_then(encode)
            .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))
    }
    async fn delete(&self, model: swim_lane::Model) -> Result<DeleteResult, Self::Error> {
        self.service
            .delete(model.project_id, model.id)
            .await
            .map(|entities_affected| DeleteResult { entities_affected })
            .map_err(|e| SwimLaneCrudError::Internal(e.to_string()))
    }
}
#[derive(Clone)]
pub struct SwimLaneResourceContext;

impl fmt::Debug for SwimLaneResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SwimLaneResourceContext")
    }
}

impl CrudResourceContext for SwimLaneResourceContext {}

#[derive(Debug, Clone)]
pub struct SwimLaneHookError(String);

impl fmt::Display for SwimLaneHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for SwimLaneHookError {}

#[derive(Debug)]
pub struct SwimLaneLifetime;

impl CrudLifetime<CrudSwimLaneResource> for SwimLaneLifetime {
    type Error = SwimLaneHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudSwimLaneResource>,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudSwimLaneResource>,
        _read_result: &mut ReadResult<CrudSwimLaneResource>,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut swim_lane::CreateModel,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        create_model.identifier =
            super::policy::normalize_identifier(create_model.identifier.clone())
                .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?;
        create_model.name = super::policy::normalize_name(create_model.name.clone())
            .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?;
        create_model.filter = normalize_filter_json(create_model.filter.clone())
            .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?;
        create_model.item_order = super::policy::parse_item_order(&create_model.item_order)
            .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?
            .as_storage()
            .to_owned();
        Ok(data)
    }

    async fn after_create(
        _create_model: &swim_lane::CreateModel,
        _model: &swim_lane::Model,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        _existing: &swim_lane::Model,
        update_model: &mut swim_lane::UpdateModel,
        _update_request: &UpdateRequest,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        update_model.identifier =
            super::policy::normalize_identifier(update_model.identifier.clone())
                .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?;
        update_model.name = super::policy::normalize_name(update_model.name.clone())
            .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?;
        update_model.filter = normalize_filter_json(update_model.filter.clone())
            .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?;
        update_model.item_order = super::policy::parse_item_order(&update_model.item_order)
            .map_err(|err| swim_lane_unprocessable_error(err.to_string()))?
            .as_storage()
            .to_owned();
        Ok(data)
    }

    async fn after_update(
        _update_model: &swim_lane::UpdateModel,
        _model: &swim_lane::Model,
        _update_request: &UpdateRequest,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        _model: &swim_lane::Model,
        _delete_request: &DeleteRequest<CrudSwimLaneResource>,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_delete(
        _model: &swim_lane::Model,
        _delete_request: &DeleteRequest<CrudSwimLaneResource>,
        _context: &SwimLaneResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}

fn swim_lane_unprocessable_error(reason: String) -> HookError<SwimLaneHookError> {
    HookError::UnprocessableEntity { reason }
}

#[derive(Debug, ToSchema)]
pub struct CrudSwimLaneResource;

impl CrudResource for CrudSwimLaneResource {
    type ReadModel = swim_lane::read_view::Model;
    type ReadModelId = swim_lane::read_view::ModelId;
    type ReadModelField = swim_lane::read_view::ModelField;

    type CreateModel = swim_lane::CreateModel;
    type CreateModelField = swim_lane::ModelField;

    type UpdateModel = swim_lane::UpdateModel;
    type UpdateModelField = swim_lane::ModelField;

    type Model = swim_lane::Model;
    type Id = swim_lane::SwimLaneId;
    type ModelField = swim_lane::ModelField;

    type Repository = SwimLaneCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = SwimLaneResourceContext;
    type HookData = ();
    type Lifetime = SwimLaneLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::SwimLane;
}

impl SeaOrmResource for CrudSwimLaneResource {
    type Entity = swim_lane::Entity;
    type SeaOrmModel = swim_lane::Model;
    type ActiveModel = swim_lane::ActiveModel;
    type Column = swim_lane::Column;
    type PrimaryKey = <swim_lane::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = swim_lane::read_view::Entity;
    type ReadViewSeaOrmModel = swim_lane::read_view::Model;
    type ReadViewActiveModel = swim_lane::read_view::ActiveModel;
    type ReadViewColumn = swim_lane::read_view::Column;
    type ReadViewPrimaryKey = <swim_lane::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <swim_lane::ModelField as CrudColumns<swim_lane::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <swim_lane::read_view::ModelField as CrudColumns<
            swim_lane::read_view::Column,
        >>::to_sea_orm_column(field)
    }
}

fn parse_filter(filter: &str) -> rootcause::Result<Condition> {
    use rootcause::prelude::*;
    let filter = filter.trim();
    let condition = if filter.is_empty() {
        Condition::all()
    } else {
        serde_json::from_str(filter)
            .context("swim-lane filter must be a CrudKit Condition JSON object")?
    };
    crate::backend::items::labels::conditions::validate_condition(&condition)?;
    Ok(condition)
}
fn normalize_filter_json(filter: impl Into<String>) -> rootcause::Result<String> {
    use rootcause::prelude::*;
    Ok(serde_json::to_string(&parse_filter(&filter.into())?)
        .context("failed to serialize swim-lane filter")?)
}
crudkit_rs::impl_add_crud_routes!(
    crate::backend::board::lanes::transport::CrudSwimLaneResource,
    swim_lane
);
pub(crate) fn routes() -> axum::Router {
    axum_swim_lane_crud_routes::add_crud_routes("/api", axum::Router::new())
}
