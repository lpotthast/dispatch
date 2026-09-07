use crate::backend::items::labels::policy as item_labels;
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::work_item,
    items::labels::workflow as workflow_labels,
    items::{self, CreateWorkItem},
    projects,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use dispatch_types::{AgentReasoningEffort, DEFAULT_STATE_LABEL};
use indexmap::IndexMap;
use sea_orm::{ActiveValue::Set, DbErr, EntityTrait};
use std::{fmt, sync::Arc};
use utoipa::ToSchema;
#[derive(Clone)]
pub struct WorkItemResourceContext;

impl fmt::Debug for WorkItemResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("WorkItemResourceContext")
    }
}

impl CrudResourceContext for WorkItemResourceContext {}

#[derive(Debug, Clone)]
pub struct WorkItemHookError(String);

impl fmt::Display for WorkItemHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for WorkItemHookError {}

#[derive(Clone, Debug, PartialEq, Eq, ToSchema, serde::Deserialize, serde::Serialize)]
pub struct CrudInitialWorkItemLabel {
    pub key: String,
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, crudkit_sea_orm::CkField, ToSchema, serde::Deserialize)]
pub struct CrudCreateWorkItem {
    pub project_id: i64,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub state: Option<String>,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<String>,
    #[serde(default)]
    pub initial_labels: serde_json::Value,
}

impl crudkit_rs::data::CreateModel for CrudCreateWorkItem {}

impl crudkit_sea_orm::SeaOrmCreateModel<work_item::ActiveModel> for CrudCreateWorkItem {
    async fn into_active_model(self) -> work_item::ActiveModel {
        work_item::ActiveModel {
            project_id: Set(self.project_id),
            title: Set(self.title),
            description: Set(self.description),
            agent_model_override: Set(self.agent_model_override),
            agent_reasoning_effort_override: Set(self.agent_reasoning_effort_override),
            ..Default::default()
        }
    }
}

#[derive(Debug)]
pub enum WorkItemRepositoryError {
    Db(DbErr),
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Internal(String),
}

impl fmt::Display for WorkItemRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Db(err) => write!(f, "work item repository database error: {err}"),
            Self::SeaOrm(err) => write!(f, "work item repository SeaORM error: {err:?}"),
            Self::Internal(err) => write!(f, "work item repository internal error: {err}"),
        }
    }
}

impl RepositoryError for WorkItemRepositoryError {}

pub struct WorkItemRepository {
    items: Arc<crate::backend::items::service::ItemService>,
    creation: Arc<crate::backend::items::creation::service::ItemCreationService>,
    fallback: SeaOrmRepo,
}

impl WorkItemRepository {
    pub(crate) fn new(
        db: Arc<sea_orm::DatabaseConnection>,
        creation: Arc<crate::backend::items::creation::service::ItemCreationService>,
        items: Arc<crate::backend::items::service::ItemService>,
    ) -> Self {
        Self {
            items,
            creation,
            fallback: SeaOrmRepo::new(db.clone()),
        }
    }
}

struct PlannedCrudWorkItemCreate {
    project_id: i64,
    input: CreateWorkItem,
}

fn crud_create_work_item_plan(
    create_model: CrudCreateWorkItem,
) -> Result<PlannedCrudWorkItemCreate, String> {
    let project_id = create_model.project_id;
    let state = projects::normalize_optional(create_model.state)
        .unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned());
    let agent_reasoning_effort_override =
        parse_crud_agent_reasoning_effort(create_model.agent_reasoning_effort_override)?;
    let initial_labels = normalized_work_item_initial_label_requests(create_model.initial_labels)?;

    let input = CreateWorkItem {
        title: create_model.title,
        description: create_model.description,
        state,
        agent_model_override: create_model.agent_model_override,
        agent_reasoning_effort_override,
        initial_labels,
    };

    Ok(PlannedCrudWorkItemCreate { project_id, input })
}

fn crud_update_work_item_plan(
    update_model: work_item::UpdateModel,
) -> Result<dispatch_types::UpdateWorkItemRequest, String> {
    let agent_reasoning_effort_override =
        parse_crud_agent_reasoning_effort(update_model.agent_reasoning_effort_override)?;

    Ok(dispatch_types::UpdateWorkItemRequest {
        title: Some(update_model.title),
        description: Some(update_model.description),
        state: None,
        agent_model_override: Some(update_model.agent_model_override),
        agent_reasoning_effort_override: Some(agent_reasoning_effort_override),
        expect_version: None,
    })
}

fn parse_crud_agent_reasoning_effort(
    value: Option<String>,
) -> Result<Option<AgentReasoningEffort>, String> {
    projects::normalize_optional(value)
        .map(|effort| {
            effort
                .parse::<AgentReasoningEffort>()
                .map_err(|err| err.to_string())
        })
        .transpose()
}

impl Repository<CrudWorkItemResource> for WorkItemRepository {
    type Error = WorkItemRepositoryError;

    async fn insert(
        &self,
        create_model: CrudCreateWorkItem,
    ) -> Result<work_item::Model, Self::Error> {
        let create =
            crud_create_work_item_plan(create_model).map_err(WorkItemRepositoryError::Internal)?;
        let item = self
            .creation
            .create(
                crate::backend::projects::ProjectReference::Id(create.project_id),
                create.input,
                Default::default(),
            )
            .await
            .map_err(|error| WorkItemRepositoryError::Internal(error.to_string()))?;
        Ok(work_item_model(item))
    }

    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemRepositoryError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<work_item::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemRepositoryError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<work_item::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemRepositoryError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<work_item::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemRepositoryError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<work_item::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<work_item::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudWorkItemResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(WorkItemRepositoryError::SeaOrm)
    }

    async fn update(
        &self,
        existing: work_item::Model,
        update_model: work_item::UpdateModel,
    ) -> Result<work_item::Model, Self::Error> {
        let update =
            crud_update_work_item_plan(update_model).map_err(WorkItemRepositoryError::Internal)?;
        self.items
            .update(
                crate::backend::projects::ProjectReference::Id(existing.project_id),
                existing.id,
                update,
                Default::default(),
            )
            .await
            .map(work_item_model)
            .map_err(|err| WorkItemRepositoryError::Internal(err.to_string()))
    }

    async fn delete(&self, model: work_item::Model) -> Result<DeleteResult, Self::Error> {
        let rows_affected = self
            .items
            .delete(
                crate::backend::projects::ProjectReference::Id(model.project_id),
                model.id,
            )
            .await
            .map_err(|err| WorkItemRepositoryError::Internal(err.to_string()))?;
        Ok(DeleteResult {
            entities_affected: rows_affected,
        })
    }
}

#[derive(Debug)]
pub struct WorkItemLifetime;

impl CrudLifetime<CrudWorkItemResource> for WorkItemLifetime {
    type Error = WorkItemHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudWorkItemResource>,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudWorkItemResource>,
        _read_result: &mut ReadResult<CrudWorkItemResource>,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut CrudCreateWorkItem,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        validate_work_item_text(&create_model.title, &create_model.description)?;
        let normalized_state = normalize_work_item_create_state(create_model.state.take())?;
        create_model.state = Some(normalized_state);
        create_model.initial_labels = serde_json::to_value(normalize_work_item_initial_labels(
            std::mem::take(&mut create_model.initial_labels),
        )?)
        .map_err(|err| work_item_unprocessable_error(err.to_string()))?;
        create_model.agent_model_override =
            projects::normalize_optional(create_model.agent_model_override.take());
        create_model.agent_reasoning_effort_override =
            parse_crud_agent_reasoning_effort(create_model.agent_reasoning_effort_override.take())
                .map_err(work_item_unprocessable_error)?
                .map(|effort| effort.as_storage().to_owned());
        Ok(data)
    }

    async fn after_create(
        _create_model: &CrudCreateWorkItem,
        _model: &work_item::Model,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        _existing: &work_item::Model,
        update_model: &mut work_item::UpdateModel,
        _update_request: &UpdateRequest,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        validate_work_item_text(&update_model.title, &update_model.description)?;
        update_model.agent_model_override =
            projects::normalize_optional(update_model.agent_model_override.take());
        update_model.agent_reasoning_effort_override =
            parse_crud_agent_reasoning_effort(update_model.agent_reasoning_effort_override.take())
                .map_err(work_item_unprocessable_error)?
                .map(|effort| effort.as_storage().to_owned());
        Ok(data)
    }

    async fn after_update(
        _update_model: &work_item::UpdateModel,
        _model: &work_item::Model,
        _update_request: &UpdateRequest,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        _model: &work_item::Model,
        _delete_request: &DeleteRequest<CrudWorkItemResource>,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_delete(
        _model: &work_item::Model,
        _delete_request: &DeleteRequest<CrudWorkItemResource>,
        _context: &WorkItemResourceContext,
        _request: RequestContext<NoAuth>,
        data: (),
    ) -> Result<(), HookError<Self::Error>> {
        Ok(data)
    }
}

fn validate_work_item_text(
    title: &str,
    description: &str,
) -> Result<(), HookError<WorkItemHookError>> {
    items::policy::validate_item_text(title, description)
        .map_err(|err| work_item_unprocessable_error(err.to_string()))
}

fn normalize_work_item_create_state(
    state: Option<String>,
) -> Result<String, HookError<WorkItemHookError>> {
    workflow_labels::normalize_state_value(
        projects::normalize_optional(state).unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned()),
    )
    .map_err(|err| work_item_unprocessable_error(err.to_string()))
}

fn normalize_work_item_initial_labels(
    labels: serde_json::Value,
) -> Result<Vec<CrudInitialWorkItemLabel>, HookError<WorkItemHookError>> {
    normalized_work_item_initial_labels(labels).map_err(work_item_unprocessable_error)
}

fn normalized_work_item_initial_labels(
    labels: serde_json::Value,
) -> Result<Vec<CrudInitialWorkItemLabel>, String> {
    normalized_work_item_initial_label_requests(labels).map(|labels| {
        labels
            .into_iter()
            .map(|label| CrudInitialWorkItemLabel {
                key: label.key,
                value: label.value,
            })
            .collect()
    })
}

fn normalized_work_item_initial_label_requests(
    labels: serde_json::Value,
) -> Result<Vec<dispatch_types::CreateWorkItemLabelRequest>, String> {
    let labels = match labels {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(raw) if raw.trim().is_empty() => Vec::new(),
        serde_json::Value::String(raw) => {
            serde_json::from_str::<Vec<CrudInitialWorkItemLabel>>(&raw)
                .map_err(|err| format!("invalid initial labels JSON: {err}"))?
        }
        value => serde_json::from_value::<Vec<CrudInitialWorkItemLabel>>(value)
            .map_err(|err| format!("invalid initial labels: {err}"))?,
    };
    item_labels::normalize_initial_labels(labels.into_iter().map(|label| (label.key, label.value)))
        .map(|labels| {
            labels
                .into_iter()
                .map(|label| dispatch_types::CreateWorkItemLabelRequest {
                    key: label.key,
                    value: label.value,
                })
                .collect()
        })
        .map_err(|err| err.to_string())
}

fn work_item_unprocessable_error(reason: String) -> HookError<WorkItemHookError> {
    HookError::UnprocessableEntity { reason }
}

#[derive(Debug, ToSchema)]
pub struct CrudWorkItemResource;

impl CrudResource for CrudWorkItemResource {
    type ReadModel = work_item::read_view::Model;
    type ReadModelId = work_item::read_view::ModelId;
    type ReadModelField = work_item::read_view::ModelField;

    type CreateModel = CrudCreateWorkItem;
    type CreateModelField = CrudCreateWorkItemField;

    type UpdateModel = work_item::UpdateModel;
    type UpdateModelField = work_item::ModelField;

    type Model = work_item::Model;
    type Id = work_item::WorkItemId;
    type ModelField = work_item::ModelField;

    type Repository = WorkItemRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = WorkItemResourceContext;
    type HookData = ();
    type Lifetime = WorkItemLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::WorkItem;
}

impl SeaOrmResource for CrudWorkItemResource {
    type Entity = work_item::Entity;
    type SeaOrmModel = work_item::Model;
    type ActiveModel = work_item::ActiveModel;
    type Column = work_item::Column;
    type PrimaryKey = <work_item::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = work_item::read_view::Entity;
    type ReadViewSeaOrmModel = work_item::read_view::Model;
    type ReadViewActiveModel = work_item::read_view::ActiveModel;
    type ReadViewColumn = work_item::read_view::Column;
    type ReadViewPrimaryKey = <work_item::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <work_item::ModelField as CrudColumns<work_item::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <work_item::read_view::ModelField as CrudColumns<work_item::read_view::Column>>::to_sea_orm_column(field)
    }
}

fn work_item_model(item: crate::shared::view_models::WorkItemView) -> work_item::Model {
    work_item::Model {
        id: item.id,
        project_id: item.project_id,
        work_group_id: item.work_group.map(|group| group.id),
        title: item.title,
        description: item.description,
        claimed_by: item.claimed_by,
        claimed_at: item.claimed_at,
        claim_expires_at: item.claim_expires_at,
        finished_at: item.finished_at,
        agent_model_override: item.agent_model_override,
        agent_reasoning_effort_override: item
            .agent_reasoning_effort_override
            .map(|effort| effort.as_storage().to_owned()),
        version: item.version,
        created_at: item.created_at,
        updated_at: item.updated_at,
    }
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::items::transport::crud::CrudWorkItemResource,
    work_item
);
pub(crate) fn routes() -> axum::Router {
    axum_work_item_crud_routes::add_crud_routes("/api", axum::Router::new())
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{
        crudkit_resources::tests::test_store,
        items::labels::repository::records as work_item_labels,
    };
    use assertr::prelude::*;
    use dispatch_types::{STATE_LABEL_KEY, WorkItemEventType};
    #[test]
    fn work_item_create_state_uses_explicit_state() {
        let state = normalize_work_item_create_state(Some(" review ".to_owned()))
            .expect("state should normalize");

        assert_that!(&(state)).is_equal_to("review");
    }

    #[test]
    fn work_item_create_state_defaults_to_open() {
        let missing = normalize_work_item_create_state(None).expect("missing state should default");
        let blank = normalize_work_item_create_state(Some("  ".to_owned()))
            .expect("blank state should default");

        assert_that!(&(missing)).is_equal_to(DEFAULT_STATE_LABEL);
        assert_that!(&(blank)).is_equal_to(DEFAULT_STATE_LABEL);
    }

    #[test]
    fn work_item_create_state_rejects_invalid_state() {
        let err = normalize_work_item_create_state(Some("bad=value".to_owned()))
            .expect_err("state containing '=' should be rejected");

        match err {
            HookError::UnprocessableEntity { reason } => {
                assert_that!(&(reason.contains("cannot contain '='"))).is_true();
            }
            other => panic!("expected unprocessable state error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn crud_work_item_create_uses_domain_create_plan() {
        let (_temp, store, project_id) = test_store().await;
        let repository = WorkItemRepository::new(
            store.db(),
            crate::backend::items::creation::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
        );

        let created = repository
            .insert(CrudCreateWorkItem {
                project_id,
                title: "Repository create".to_owned(),
                description: "Created through the CrudKit repository".to_owned(),
                state: Some(" review ".to_owned()),
                agent_model_override: Some("  ".to_owned()),
                agent_reasoning_effort_override: Some(" medium ".to_owned()),
                initial_labels: serde_json::json!([
                    { "key": " priority ", "value": " high " },
                    { "key": "needs-verification", "value": "  " }
                ]),
            })
            .await
            .unwrap();

        assert_that!(&(created.version)).is_equal_to(1);
        assert_that!(&(created.agent_model_override.is_none())).is_true();
        assert_that!(&(created.agent_reasoning_effort_override.as_deref()))
            .is_equal_to(Some("medium"));

        let labels = work_item_labels::for_item(store.db().as_ref(), project_id, created.id)
            .await
            .unwrap();
        assert_that!(
            &(labels.iter().any(|label| {
                label.key == STATE_LABEL_KEY && label.value.as_deref() == Some("review")
            }))
        )
        .is_true();
        assert_that!(
            &(labels.iter().any(|label| {
                label.key == "priority" && label.value.as_deref() == Some("high")
            }))
        )
        .is_true();
        assert_that!(
            &(labels
                .iter()
                .any(|label| label.key == "needs-verification" && label.value.is_none()))
        )
        .is_true();

        let events = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .events("demo", Some(created.id), None)
        .await
        .unwrap();
        assert_that!(
            &(events
                .iter()
                .any(|event| event.event_type == WorkItemEventType::ItemCreated))
        )
        .is_true();
    }

    #[tokio::test]
    async fn crud_work_item_update_uses_domain_update_plan() {
        let (_temp, store, project_id) = test_store().await;
        let repository = WorkItemRepository::new(
            store.db(),
            crate::backend::items::creation::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
        );
        let created = repository
            .insert(CrudCreateWorkItem {
                project_id,
                title: "Before update".to_owned(),
                description: "Existing CrudKit item".to_owned(),
                state: Some("open".to_owned()),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: serde_json::Value::Null,
            })
            .await
            .unwrap();

        let updated = repository
            .update(
                created.clone(),
                work_item::UpdateModel {
                    title: "After update".to_owned(),
                    description: "Updated through the CrudKit repository".to_owned(),
                    agent_model_override: Some("gpt-5.5".to_owned()),
                    agent_reasoning_effort_override: Some(" high ".to_owned()),
                },
            )
            .await
            .unwrap();

        assert_that!(&(updated.version)).is_equal_to(created.version + 1);
        assert_that!(&(updated.title)).is_equal_to("After update");
        assert_that!(&(updated.agent_model_override.as_deref())).is_equal_to(Some("gpt-5.5"));
        assert_that!(&(updated.agent_reasoning_effort_override.as_deref()))
            .is_equal_to(Some("high"));

        let events = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .events("demo", Some(created.id), None)
        .await
        .unwrap();
        assert_that!(
            &(events
                .iter()
                .any(|event| event.event_type == WorkItemEventType::ItemUpdated))
        )
        .is_true();
    }

    #[tokio::test]
    async fn crud_work_item_delete_uses_domain_delete_path() {
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, store, project_id) = test_store().await;
        let repository = WorkItemRepository::new(
            store.db(),
            crate::backend::items::creation::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
        );
        let source = repository
            .insert(CrudCreateWorkItem {
                project_id,
                title: "Source item".to_owned(),
                description: "Deleted through the CrudKit repository".to_owned(),
                state: Some("open".to_owned()),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: serde_json::Value::Null,
            })
            .await
            .unwrap();
        let target = repository
            .insert(CrudCreateWorkItem {
                project_id,
                title: "Target item".to_owned(),
                description: "Keeps relationship cleanup events".to_owned(),
                state: Some("open".to_owned()),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: serde_json::Value::Null,
            })
            .await
            .unwrap();
        crate::backend::relationships::tests::service(&store, event_bus.clone())
            .create(
                "demo",
                source.id,
                target.id,
                "blocks".to_owned(),
                Default::default(),
            )
            .await
            .unwrap();

        let deleted = repository.delete(source.clone()).await.unwrap();

        assert_that!(&(deleted.entities_affected)).is_equal_to(1);
        assert_that!(
            &(crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new()
            )
            .get("demo", source.id)
            .await
            .is_err())
        )
        .is_true();
        assert_that!(
            &(crate::backend::relationships::tests::service(
                &store,
                crate::backend::events::UiEventBus::new()
            )
            .list("demo", target.id)
            .await
            .unwrap()
            .is_empty())
        )
        .is_true();

        let project_events = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .events("demo", None, None)
        .await
        .unwrap();
        assert_that!(
            &(project_events.iter().any(|event| {
                event.event_type == WorkItemEventType::ItemDeleted && event.body == "Deleted item"
            }))
        )
        .is_true();
        let target_events = crate::backend::items::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .events("demo", Some(target.id), None)
        .await
        .unwrap();
        assert_that!(
            &(target_events.iter().any(|event| {
                event.event_type == WorkItemEventType::RelationshipDeleted
                    && event.body
                        == format!("Deleted relationships touching removed item #{}", source.id)
            }))
        )
        .is_true();
    }
}
