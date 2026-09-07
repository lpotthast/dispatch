use crate::backend::{
    comments::{AddCommentRequest, CommentTarget, service::CommentService},
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::comment,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::{DatabaseConnection, EntityTrait};
use std::{fmt, sync::Arc};
use utoipa::ToSchema;

#[derive(Debug, CkResourceContext)]
pub struct CommentResourceContext;

#[derive(Debug, ToSchema)]
pub struct CrudCommentResource;

impl CrudResource for CrudCommentResource {
    type ReadModel = comment::read_view::Model;
    type ReadModelId = comment::read_view::ModelId;
    type ReadModelField = comment::read_view::ModelField;

    type CreateModel = comment::CreateModel;
    type CreateModelField = comment::ModelField;

    type UpdateModel = comment::UpdateModel;
    type UpdateModelField = comment::ModelField;

    type Model = comment::Model;
    type Id = comment::CommentId;
    type ModelField = comment::ModelField;

    type Repository = CommentCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = CommentResourceContext;
    type HookData = ();
    type Lifetime = CommentLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::Comment;
}

impl SeaOrmResource for CrudCommentResource {
    type Entity = comment::Entity;
    type SeaOrmModel = comment::Model;
    type ActiveModel = comment::ActiveModel;
    type Column = comment::Column;
    type PrimaryKey = <comment::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = comment::read_view::Entity;
    type ReadViewSeaOrmModel = comment::read_view::Model;
    type ReadViewActiveModel = comment::read_view::ActiveModel;
    type ReadViewColumn = comment::read_view::Column;
    type ReadViewPrimaryKey = <comment::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <comment::ModelField as CrudColumns<comment::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <comment::read_view::ModelField as CrudColumns<comment::read_view::Column>>::to_sea_orm_column(field)
    }
}

#[derive(Debug)]
pub enum CommentCrudRepositoryError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Service(String),
}
impl fmt::Display for CommentCrudRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SeaOrm(error) => write!(f, "comment CRUD repository error: {error:?}"),
            Self::Service(error) => write!(f, "comment operation failed: {error}"),
        }
    }
}
impl RepositoryError for CommentCrudRepositoryError {}

pub struct CommentCrudRepository {
    fallback: SeaOrmRepo,
    service: Arc<CommentService>,
}
impl CommentCrudRepository {
    pub(crate) fn new(db: Arc<DatabaseConnection>, service: Arc<CommentService>) -> Self {
        Self {
            fallback: SeaOrmRepo::new(db),
            service,
        }
    }
}
impl Repository<CrudCommentResource> for CommentCrudRepository {
    type Error = CommentCrudRepositoryError;
    async fn insert(&self, model: comment::CreateModel) -> Result<comment::Model, Self::Error> {
        let input = create_input(&model).map_err(service_error)?;
        self.service
            .add(
                CommentTarget::Item(model.work_item_id),
                input,
                Default::default(),
            )
            .await
            .map(to_model)
            .map_err(service_error)
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<comment::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudCommentResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(CommentCrudRepositoryError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<comment::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<comment::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudCommentResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(CommentCrudRepositoryError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<comment::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<comment::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudCommentResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(CommentCrudRepositoryError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<comment::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<comment::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudCommentResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(CommentCrudRepositoryError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<comment::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<comment::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudCommentResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(CommentCrudRepositoryError::SeaOrm)
    }

    async fn update(
        &self,
        existing: comment::Model,
        model: comment::UpdateModel,
    ) -> Result<comment::Model, Self::Error> {
        let input = update_input(&model).map_err(service_error)?;
        self.service
            .update(existing.work_item_id, existing.id, input)
            .await
            .map(to_model)
            .map_err(service_error)
    }
    async fn delete(&self, model: comment::Model) -> Result<DeleteResult, Self::Error> {
        self.service
            .delete(model.work_item_id, model.id)
            .await
            .map_err(service_error)?;
        Ok(DeleteResult {
            entities_affected: 1,
        })
    }
}
fn service_error(error: impl fmt::Display) -> CommentCrudRepositoryError {
    CommentCrudRepositoryError::Service(error.to_string())
}
fn create_input(model: &comment::CreateModel) -> rootcause::Result<AddCommentRequest> {
    Ok(AddCommentRequest {
        author_type: model.author_type.parse()?,
        author_name: model.author_name.clone(),
        body: model.body.clone(),
    })
}
fn update_input(model: &comment::UpdateModel) -> rootcause::Result<AddCommentRequest> {
    Ok(AddCommentRequest {
        author_type: model.author_type.parse()?,
        author_name: model.author_name.clone(),
        body: model.body.clone(),
    })
}
fn to_model(view: dispatch_types::CommentView) -> comment::Model {
    comment::Model {
        id: view.id,
        work_item_id: view.work_item_id,
        author_type: view.author_type.as_storage().to_owned(),
        author_name: view.author_name,
        body: view.body,
        created_at: view.created_at,
    }
}
crudkit_rs::impl_add_crud_routes!(
    crate::backend::comments::transport::crud::CrudCommentResource,
    comment
);
pub(crate) fn routes() -> axum::Router {
    axum_comment_crud_routes::add_crud_routes("/api", axum::Router::new())
}

#[derive(Debug)]
pub struct CommentHookError;
impl fmt::Display for CommentHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("comment validation failed")
    }
}
impl std::error::Error for CommentHookError {}
fn comment_unprocessable_error(reason: String) -> HookError<CommentHookError> {
    HookError::UnprocessableEntity { reason }
}
#[derive(Debug)]
pub struct CommentLifetime;

impl CrudLifetime<CrudCommentResource> for CommentLifetime {
    type Error = CommentHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudCommentResource>,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudCommentResource>,
        _read_result: &mut ReadResult<CrudCommentResource>,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut comment::CreateModel,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        let input = create_input(create_model)
            .map_err(|error| comment_unprocessable_error(error.to_string()))?;
        CommentService::validate(&input)
            .map_err(|error| comment_unprocessable_error(error.to_string()))?;
        Ok(data)
    }

    async fn after_create(
        _create_model: &comment::CreateModel,
        _model: &comment::Model,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        _existing: &comment::Model,
        update_model: &mut comment::UpdateModel,
        _update_request: &UpdateRequest,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        let input = update_input(update_model)
            .map_err(|error| comment_unprocessable_error(error.to_string()))?;
        CommentService::validate(&input)
            .map_err(|error| comment_unprocessable_error(error.to_string()))?;
        Ok(data)
    }

    async fn after_update(
        _update_model: &comment::UpdateModel,
        _model: &comment::Model,
        _update_request: &UpdateRequest,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        _model: &comment::Model,
        _delete_request: &DeleteRequest<CrudCommentResource>,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_delete(
        _model: &comment::Model,
        _delete_request: &DeleteRequest<CrudCommentResource>,
        _context: &CommentResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}
