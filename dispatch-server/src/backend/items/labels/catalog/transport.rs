use super::{
    model::{CreateLabelKey, UpdateLabelKey},
    repository::encode,
    service::CatalogService,
};
use crate::backend::items::labels::policy as item_labels;
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::label_key,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum LabelKeyCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for LabelKeyCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for LabelKeyCrudError {}
pub struct LabelKeyCrudRepository {
    service: Arc<CatalogService>,
    fallback: SeaOrmRepo,
}
impl LabelKeyCrudRepository {
    pub(crate) fn new(db: Arc<sea_orm::DatabaseConnection>, service: Arc<CatalogService>) -> Self {
        Self {
            service,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudLabelKeyResource> for LabelKeyCrudRepository {
    type Error = LabelKeyCrudError;
    async fn insert(&self, model: label_key::CreateModel) -> Result<label_key::Model, Self::Error> {
        self.service
            .create(
                model.project_id,
                CreateLabelKey {
                    key: model.key,
                    accent_color: model.accent_color,
                    persistent: model.persistent,
                },
            )
            .await
            .map(encode)
            .map_err(|e| LabelKeyCrudError::Internal(e.to_string()))
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<label_key::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudLabelKeyResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(LabelKeyCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<label_key::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<label_key::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudLabelKeyResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(LabelKeyCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<label_key::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<label_key::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudLabelKeyResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(LabelKeyCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<label_key::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<label_key::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudLabelKeyResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(LabelKeyCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<label_key::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<label_key::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudLabelKeyResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(LabelKeyCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: label_key::Model,
        model: label_key::UpdateModel,
    ) -> Result<label_key::Model, Self::Error> {
        self.service
            .update(
                existing.project_id,
                existing.id,
                UpdateLabelKey {
                    accent_color: model.accent_color,
                    persistent: model.persistent,
                },
            )
            .await
            .map(encode)
            .map_err(|e| LabelKeyCrudError::Internal(e.to_string()))
    }
    async fn delete(&self, _model: label_key::Model) -> Result<DeleteResult, Self::Error> {
        Err(LabelKeyCrudError::Internal(
            "label keys are forgotten by clearing persistent or removing their final usage".into(),
        ))
    }
}
#[derive(Clone)]
pub struct LabelKeyResourceContext {
    pub(crate) service: Arc<CatalogService>,
}

impl fmt::Debug for LabelKeyResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LabelKeyResourceContext")
    }
}

impl CrudResourceContext for LabelKeyResourceContext {}

#[derive(Debug, Clone)]
pub struct LabelKeyHookError(String);

impl fmt::Display for LabelKeyHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for LabelKeyHookError {}

#[derive(Debug)]
pub struct LabelKeyLifetime;

impl CrudLifetime<CrudLabelKeyResource> for LabelKeyLifetime {
    type Error = LabelKeyHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudLabelKeyResource>,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudLabelKeyResource>,
        _read_result: &mut ReadResult<CrudLabelKeyResource>,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut label_key::CreateModel,
        context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        create_model.key = item_labels::normalize_key(std::mem::take(&mut create_model.key))
            .map_err(|err| label_key_unprocessable_error(err.to_string()))?;
        create_model.accent_color =
            super::policy::normalize_accent_color(create_model.accent_color.take())
                .map_err(|err| label_key_unprocessable_error(err.to_string()))?;
        if !create_model.persistent {
            return Err(label_key_unprocessable_error(
                "an unused label key must be created as persistent".to_owned(),
            ));
        }
        let already_exists = context
            .service
            .exists(create_model.project_id, &create_model.key)
            .await
            .map_err(|err| HookError::Internal(LabelKeyHookError(err.to_string())))?;
        if already_exists {
            return Err(label_key_unprocessable_error(format!(
                "label key '{}' already exists",
                create_model.key
            )));
        }
        Ok(data)
    }

    async fn after_create(
        _create_model: &label_key::CreateModel,
        _model: &label_key::Model,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        existing: &label_key::Model,
        update_model: &mut label_key::UpdateModel,
        _update_request: &UpdateRequest,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        update_model.accent_color =
            super::policy::normalize_accent_color(update_model.accent_color.take())
                .map_err(|err| label_key_unprocessable_error(err.to_string()))?;
        if existing.built_in && !update_model.persistent {
            return Err(label_key_unprocessable_error(format!(
                "built-in label key '{}' must remain persistent",
                existing.key
            )));
        }
        Ok(data)
    }

    async fn after_update(
        _update_model: &label_key::UpdateModel,
        _model: &label_key::Model,
        _update_request: &UpdateRequest,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        _model: &label_key::Model,
        _delete_request: &DeleteRequest<CrudLabelKeyResource>,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        _data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Err(label_key_unprocessable_error(
            "label keys are forgotten by clearing persistent or removing their final usage"
                .to_owned(),
        ))
    }

    async fn after_delete(
        _model: &label_key::Model,
        _delete_request: &DeleteRequest<CrudLabelKeyResource>,
        _context: &LabelKeyResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}

fn label_key_unprocessable_error(reason: String) -> HookError<LabelKeyHookError> {
    HookError::UnprocessableEntity { reason }
}

#[derive(Debug, ToSchema)]
pub struct CrudLabelKeyResource;

impl CrudResource for CrudLabelKeyResource {
    type ReadModel = label_key::read_view::Model;
    type ReadModelId = label_key::read_view::ModelId;
    type ReadModelField = label_key::read_view::ModelField;

    type CreateModel = label_key::CreateModel;
    type CreateModelField = label_key::ModelField;

    type UpdateModel = label_key::UpdateModel;
    type UpdateModelField = label_key::ModelField;

    type Model = label_key::Model;
    type Id = label_key::LabelKeyId;
    type ModelField = label_key::ModelField;

    type Repository = LabelKeyCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = LabelKeyResourceContext;
    type HookData = ();
    type Lifetime = LabelKeyLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::LabelKey;
}

impl SeaOrmResource for CrudLabelKeyResource {
    type Entity = label_key::Entity;
    type SeaOrmModel = label_key::Model;
    type ActiveModel = label_key::ActiveModel;
    type Column = label_key::Column;
    type PrimaryKey = <label_key::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = label_key::read_view::Entity;
    type ReadViewSeaOrmModel = label_key::read_view::Model;
    type ReadViewActiveModel = label_key::read_view::ActiveModel;
    type ReadViewColumn = label_key::read_view::Column;
    type ReadViewPrimaryKey = <label_key::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <label_key::ModelField as CrudColumns<label_key::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <label_key::read_view::ModelField as CrudColumns<
            label_key::read_view::Column,
        >>::to_sea_orm_column(field)
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::items::labels::catalog::transport::CrudLabelKeyResource,
    label_key
);
pub(crate) fn routes() -> axum::Router {
    axum_label_key_crud_routes::add_crud_routes("/api", axum::Router::new())
}
