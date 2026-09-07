use super::super::{repository::encode, service::PersonalityService};
use crate::backend::projects::ProjectReference;
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::personality,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use dispatch_types::AutomationPersonalityInput;
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum PersonalityCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for PersonalityCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for PersonalityCrudError {}
pub struct PersonalityCrudRepository {
    service: Arc<PersonalityService>,
    fallback: SeaOrmRepo,
}
impl PersonalityCrudRepository {
    pub(crate) fn new(
        db: Arc<sea_orm::DatabaseConnection>,
        service: Arc<PersonalityService>,
    ) -> Self {
        Self {
            service,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudPersonalityResource> for PersonalityCrudRepository {
    type Error = PersonalityCrudError;
    async fn insert(
        &self,
        model: personality::CreateModel,
    ) -> Result<personality::Model, Self::Error> {
        self.service
            .create(
                ProjectReference::Id(model.project_id),
                AutomationPersonalityInput {
                    key: String::new(),
                    name: model.name,
                    description: model.personality_description,
                },
            )
            .await
            .map(encode)
            .map_err(|e| PersonalityCrudError::Internal(e.to_string()))
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<personality::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudPersonalityResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(PersonalityCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<personality::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<personality::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudPersonalityResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(PersonalityCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<personality::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<personality::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudPersonalityResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(PersonalityCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<personality::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<personality::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudPersonalityResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(PersonalityCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<personality::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<personality::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudPersonalityResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(PersonalityCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: personality::Model,
        model: personality::UpdateModel,
    ) -> Result<personality::Model, Self::Error> {
        self.service
            .update(
                ProjectReference::Id(existing.project_id),
                existing.id,
                AutomationPersonalityInput {
                    key: String::new(),
                    name: model.name,
                    description: model.personality_description,
                },
            )
            .await
            .map(encode)
            .map_err(|e| PersonalityCrudError::Internal(e.to_string()))
    }
    async fn delete(&self, model: personality::Model) -> Result<DeleteResult, Self::Error> {
        self.service
            .delete(ProjectReference::Id(model.project_id), model.id)
            .await
            .map(|entities_affected| DeleteResult { entities_affected })
            .map_err(|e| PersonalityCrudError::Internal(e.to_string()))
    }
}
#[derive(Clone)]
pub struct PersonalityResourceContext {
    pub(crate) service: Arc<PersonalityService>,
}

impl fmt::Debug for PersonalityResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PersonalityResourceContext")
    }
}

impl CrudResourceContext for PersonalityResourceContext {}

#[derive(Debug, Clone)]
pub struct PersonalityHookError(String);

impl fmt::Display for PersonalityHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PersonalityHookError {}

#[derive(Debug)]
pub struct PersonalityLifetime;

impl CrudLifetime<CrudPersonalityResource> for PersonalityLifetime {
    type Error = PersonalityHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudPersonalityResource>,
        _context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudPersonalityResource>,
        _read_result: &mut ReadResult<CrudPersonalityResource>,
        _context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut personality::CreateModel,
        context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        context
            .service
            .validate_edit(
                create_model.project_id,
                None,
                &AutomationPersonalityInput {
                    key: String::new(),
                    name: create_model.name.clone(),
                    description: create_model.personality_description.clone(),
                },
            )
            .await
            .map_err(|e| personality_unprocessable_error(e.to_string()))?;
        Ok(data)
    }

    async fn after_create(
        _create_model: &personality::CreateModel,
        _model: &personality::Model,
        _context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        existing: &personality::Model,
        update_model: &mut personality::UpdateModel,
        _update_request: &UpdateRequest,
        context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        context
            .service
            .validate_edit(
                existing.project_id,
                Some(existing.id),
                &AutomationPersonalityInput {
                    key: String::new(),
                    name: update_model.name.clone(),
                    description: update_model.personality_description.clone(),
                },
            )
            .await
            .map_err(|e| personality_unprocessable_error(e.to_string()))?;
        Ok(data)
    }

    async fn after_update(
        _update_model: &personality::UpdateModel,
        _model: &personality::Model,
        _update_request: &UpdateRequest,
        _context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        model: &personality::Model,
        _delete_request: &DeleteRequest<CrudPersonalityResource>,
        context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        context
            .service
            .validate_delete(model.project_id, model.id)
            .await
            .map_err(|e| personality_unprocessable_error(e.to_string()))?;
        Ok(data)
    }

    async fn after_delete(
        _model: &personality::Model,
        _delete_request: &DeleteRequest<CrudPersonalityResource>,
        _context: &PersonalityResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}

fn personality_unprocessable_error(reason: String) -> HookError<PersonalityHookError> {
    HookError::UnprocessableEntity { reason }
}

#[derive(Debug, ToSchema)]
pub struct CrudPersonalityResource;

impl CrudResource for CrudPersonalityResource {
    type ReadModel = personality::read_view::Model;
    type ReadModelId = personality::read_view::ModelId;
    type ReadModelField = personality::read_view::ModelField;

    type CreateModel = personality::CreateModel;
    type CreateModelField = personality::ModelField;

    type UpdateModel = personality::UpdateModel;
    type UpdateModelField = personality::ModelField;

    type Model = personality::Model;
    type Id = personality::PersonalityId;
    type ModelField = personality::ModelField;

    type Repository = PersonalityCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = PersonalityResourceContext;
    type HookData = ();
    type Lifetime = PersonalityLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::Personality;
}

impl SeaOrmResource for CrudPersonalityResource {
    type Entity = personality::Entity;
    type SeaOrmModel = personality::Model;
    type ActiveModel = personality::ActiveModel;
    type Column = personality::Column;
    type PrimaryKey = <personality::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = personality::read_view::Entity;
    type ReadViewSeaOrmModel = personality::read_view::Model;
    type ReadViewActiveModel = personality::read_view::ActiveModel;
    type ReadViewColumn = personality::read_view::Column;
    type ReadViewPrimaryKey = <personality::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <personality::ModelField as CrudColumns<personality::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <personality::read_view::ModelField as CrudColumns<personality::read_view::Column>>::to_sea_orm_column(field)
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::automation::personalities::transport::crud::CrudPersonalityResource,
    personality
);
pub(crate) fn routes() -> axum::Router {
    axum_personality_crud_routes::add_crud_routes("/api", axum::Router::new())
}
