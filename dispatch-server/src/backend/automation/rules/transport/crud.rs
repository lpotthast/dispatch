use super::super::{repository::encoding::encode, service::RuleService};
use crate::backend::projects::ProjectReference;
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::automation_trigger,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};

use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Debug)]
pub enum AutomationTriggerCrudError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}
impl fmt::Display for AutomationTriggerCrudError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl RepositoryError for AutomationTriggerCrudError {}
pub struct AutomationTriggerCrudRepository {
    service: Arc<RuleService>,
    fallback: SeaOrmRepo,
}
impl AutomationTriggerCrudRepository {
    pub(crate) fn new(db: Arc<sea_orm::DatabaseConnection>, service: Arc<RuleService>) -> Self {
        Self {
            service,
            fallback: SeaOrmRepo::new(db),
        }
    }
}
impl Repository<CrudAutomationTriggerResource> for AutomationTriggerCrudRepository {
    type Error = AutomationTriggerCrudError;
    async fn insert(
        &self,
        model: automation_trigger::CreateModel,
    ) -> Result<automation_trigger::Model, Self::Error> {
        let fields = create_fields(&model)
            .map_err(|error| AutomationTriggerCrudError::Internal(error.to_string()))?;
        self.service
            .create_configuration(ProjectReference::Id(model.project_id), fields)
            .await
            .and_then(encode)
            .map_err(|error| AutomationTriggerCrudError::Internal(error.to_string()))
    }
    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<automation_trigger::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudAutomationTriggerResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AutomationTriggerCrudError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<automation_trigger::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<automation_trigger::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAutomationTriggerResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AutomationTriggerCrudError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<automation_trigger::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<automation_trigger::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAutomationTriggerResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AutomationTriggerCrudError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<automation_trigger::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<automation_trigger::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAutomationTriggerResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AutomationTriggerCrudError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<automation_trigger::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<automation_trigger::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudAutomationTriggerResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(AutomationTriggerCrudError::SeaOrm)
    }

    async fn update(
        &self,
        existing: automation_trigger::Model,
        model: automation_trigger::UpdateModel,
    ) -> Result<automation_trigger::Model, Self::Error> {
        let fields = update_fields(&model)
            .map_err(|error| AutomationTriggerCrudError::Internal(error.to_string()))?;
        self.service
            .update_configuration(
                ProjectReference::Id(existing.project_id),
                existing.id,
                fields,
            )
            .await
            .and_then(encode)
            .map_err(|error| AutomationTriggerCrudError::Internal(error.to_string()))
    }
    async fn delete(&self, model: automation_trigger::Model) -> Result<DeleteResult, Self::Error> {
        self.service
            .delete(ProjectReference::Id(model.project_id), model.id)
            .await
            .map(|entities_affected| DeleteResult { entities_affected })
            .map_err(|e| AutomationTriggerCrudError::Internal(e.to_string()))
    }
}
#[derive(Clone)]
pub struct AutomationTriggerResourceContext {
    pub(crate) service: Arc<RuleService>,
}

impl fmt::Debug for AutomationTriggerResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AutomationTriggerResourceContext")
    }
}

impl CrudResourceContext for AutomationTriggerResourceContext {}

#[derive(Debug, Clone)]
pub struct AutomationTriggerHookError(String);

impl fmt::Display for AutomationTriggerHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AutomationTriggerHookError {}

#[derive(Debug)]
pub struct AutomationTriggerLifetime;

impl CrudLifetime<CrudAutomationTriggerResource> for AutomationTriggerLifetime {
    type Error = AutomationTriggerHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudAutomationTriggerResource>,
        _context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudAutomationTriggerResource>,
        _read_result: &mut ReadResult<CrudAutomationTriggerResource>,
        _context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut automation_trigger::CreateModel,
        context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        let fields = create_fields(create_model)
            .map_err(|error| automation_trigger_unprocessable_error(error.to_string()))?;
        context
            .service
            .validate_edit(create_model.project_id, None, fields)
            .await
            .map_err(|error| automation_trigger_unprocessable_error(error.to_string()))?;
        Ok(data)
    }

    async fn after_create(
        _create_model: &automation_trigger::CreateModel,
        _model: &automation_trigger::Model,
        _context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        existing: &automation_trigger::Model,
        update_model: &mut automation_trigger::UpdateModel,
        _update_request: &UpdateRequest,
        context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        let fields = update_fields(update_model)
            .map_err(|error| automation_trigger_unprocessable_error(error.to_string()))?;
        context
            .service
            .validate_edit(existing.project_id, Some(existing.id), fields)
            .await
            .map_err(|error| automation_trigger_unprocessable_error(error.to_string()))?;
        Ok(data)
    }

    async fn after_update(
        _update_model: &automation_trigger::UpdateModel,
        _model: &automation_trigger::Model,
        _update_request: &UpdateRequest,
        _context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        model: &automation_trigger::Model,
        _delete_request: &DeleteRequest<CrudAutomationTriggerResource>,
        context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        context
            .service
            .validate_delete(model.project_id, model.id)
            .await
            .map_err(|e| automation_trigger_unprocessable_error(e.to_string()))?;
        Ok(data)
    }

    async fn after_delete(
        _model: &automation_trigger::Model,
        _delete_request: &DeleteRequest<CrudAutomationTriggerResource>,
        _context: &AutomationTriggerResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}

fn automation_trigger_unprocessable_error(reason: String) -> HookError<AutomationTriggerHookError> {
    HookError::UnprocessableEntity { reason }
}

#[derive(Debug, ToSchema)]
pub struct CrudAutomationTriggerResource;

impl CrudResource for CrudAutomationTriggerResource {
    type ReadModel = automation_trigger::read_view::Model;
    type ReadModelId = automation_trigger::read_view::ModelId;
    type ReadModelField = automation_trigger::read_view::ModelField;

    type CreateModel = automation_trigger::CreateModel;
    type CreateModelField = automation_trigger::ModelField;

    type UpdateModel = automation_trigger::UpdateModel;
    type UpdateModelField = automation_trigger::ModelField;

    type Model = automation_trigger::Model;
    type Id = automation_trigger::AutomationTriggerId;
    type ModelField = automation_trigger::ModelField;

    type Repository = AutomationTriggerCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = AutomationTriggerResourceContext;
    type HookData = ();
    type Lifetime = AutomationTriggerLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::AutomationTrigger;
}

impl SeaOrmResource for CrudAutomationTriggerResource {
    type Entity = automation_trigger::Entity;
    type SeaOrmModel = automation_trigger::Model;
    type ActiveModel = automation_trigger::ActiveModel;
    type Column = automation_trigger::Column;
    type PrimaryKey = <automation_trigger::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = automation_trigger::read_view::Entity;
    type ReadViewSeaOrmModel = automation_trigger::read_view::Model;
    type ReadViewActiveModel = automation_trigger::read_view::ActiveModel;
    type ReadViewColumn = automation_trigger::read_view::Column;
    type ReadViewPrimaryKey = <automation_trigger::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <automation_trigger::ModelField as CrudColumns<automation_trigger::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <automation_trigger::read_view::ModelField as CrudColumns<
            automation_trigger::read_view::Column,
        >>::to_sea_orm_column(field)
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::automation::rules::transport::crud::CrudAutomationTriggerResource,
    automation_trigger
);
pub(crate) fn routes() -> axum::Router {
    axum_automation_trigger_crud_routes::add_crud_routes("/api", axum::Router::new())
}

use super::super::{
    model::{PersonalityReference, RuleFields},
    repository::encoding::{decode_trigger_policy, selector_from_storage},
};

fn create_fields(model: &automation_trigger::CreateModel) -> rootcause::Result<RuleFields> {
    let effect = model.effect.parse()?;
    let policy = decode_trigger_policy(
        effect,
        model.produced_work_spec_json.as_deref(),
        model.postconditions_json.as_deref(),
        model.model_override.as_deref(),
        model.reasoning_effort_override.as_deref(),
        model.timeout_seconds,
        model.max_concurrent_runs,
        model.concurrency_group.as_deref(),
    )?;
    Ok(RuleFields {
        name: model.name.clone(),
        enabled: model.enabled,
        activation: model.activation.parse()?,
        effect,
        schedule: model.schedule.clone(),
        tool_name: None,
        mutability: model.mutability.parse()?,
        personality: model.personality_id.map(PersonalityReference::Id),
        prompt: model.prompt.clone(),
        selector: selector_from_storage(model.work_item_selector.as_deref())?,
        priority: model.priority,
        exclusive: model.exclusive,
        produced_work: policy.produced_work,
        execution: policy.execution,
        postconditions: policy.postconditions,
    })
}

fn update_fields(model: &automation_trigger::UpdateModel) -> rootcause::Result<RuleFields> {
    let effect = model.effect.parse()?;
    let policy = decode_trigger_policy(
        effect,
        model.produced_work_spec_json.as_deref(),
        model.postconditions_json.as_deref(),
        model.model_override.as_deref(),
        model.reasoning_effort_override.as_deref(),
        model.timeout_seconds,
        model.max_concurrent_runs,
        model.concurrency_group.as_deref(),
    )?;
    Ok(RuleFields {
        name: model.name.clone(),
        enabled: model.enabled,
        activation: model.activation.parse()?,
        effect,
        schedule: model.schedule.clone(),
        tool_name: None,
        mutability: model.mutability.parse()?,
        personality: model.personality_id.map(PersonalityReference::Id),
        prompt: model.prompt.clone(),
        selector: selector_from_storage(model.work_item_selector.as_deref())?,
        priority: model.priority,
        exclusive: model.exclusive,
        produced_work: policy.produced_work,
        execution: policy.execution,
        postconditions: policy.postconditions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::crudkit_resources::tests::test_store;
    use assertr::prelude::*;
    use sea_orm::{ColumnTrait, QueryFilter};
    #[tokio::test]
    async fn automation_crud_hooks_reject_incompatible_policy_on_create_and_update() {
        let (_temp, store, project_id) = test_store().await;
        let existing = automation_trigger::Entity::find()
            .filter(automation_trigger::Column::ProjectId.eq(project_id))
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap();
        let mut fields = serde_json::to_value(&existing).unwrap();
        fields["produced_work_spec_json"] = serde_json::Value::String("{}".to_owned());
        let context = AutomationTriggerResourceContext {
            service: crate::backend::automation::rules::tests::service(&store),
        };
        let mut create = serde_json::from_value(fields.clone()).unwrap();
        let error = AutomationTriggerLifetime::before_create(
            &mut create,
            &context,
            RequestContext::unauthenticated(),
            (),
        )
        .await
        .unwrap_err();
        match error {
            HookError::UnprocessableEntity { reason } => {
                assert_that!(&reason).contains("only valid for produce_work");
            }
            other => panic!("expected invalid automation policy, got {other:?}"),
        }
        let mut update = serde_json::from_value(fields).unwrap();
        let error = AutomationTriggerLifetime::before_update(
            &existing,
            &mut update,
            &UpdateRequest { condition: None },
            &context,
            RequestContext::unauthenticated(),
            (),
        )
        .await
        .unwrap_err();
        match error {
            HookError::UnprocessableEntity { reason } => {
                assert_that!(&reason).contains("only valid for produce_work");
            }
            other => panic!("expected invalid automation policy, got {other:?}"),
        }
    }
}
