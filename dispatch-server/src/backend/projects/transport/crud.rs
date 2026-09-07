use super::super::{
    CreateProject, UpdateProject, UpdateProjectSettings, service::ProjectService, settings,
};
use crate::backend::{
    crudkit_resources::{CrudResources, NoopCollaborationService},
    entities::project,
    projects::deletion::ProjectDeletionService,
    storage::Store,
};
use crudkit_core::{Order, condition::Condition};
use crudkit_rs::prelude::*;
use crudkit_sea_orm::{CrudColumns, SeaOrmResource, repo::SeaOrmRepo};
use indexmap::IndexMap;
use sea_orm::EntityTrait;
use std::{fmt, path::PathBuf, sync::Arc};
use utoipa::ToSchema;

#[derive(Clone)]
pub struct ProjectResourceContext {
    pub(crate) service: Arc<ProjectService>,
}

impl fmt::Debug for ProjectResourceContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ProjectResourceContext")
    }
}

impl CrudResourceContext for ProjectResourceContext {}

#[derive(Debug, Clone)]
pub struct ProjectHookError(String);

impl fmt::Display for ProjectHookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ProjectHookError {}

#[derive(Debug, Default)]
pub struct ProjectHookData;

#[derive(Debug)]
pub enum ProjectCrudRepositoryError {
    SeaOrm(crudkit_sea_orm::repo::SeaOrmRepoError),
    Deletion(String),
    Service(String),
}

impl fmt::Display for ProjectCrudRepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SeaOrm(err) => write!(f, "project CRUD repository SeaORM error: {err:?}"),
            Self::Service(err) => write!(f, "project operation failed: {err}"),
            Self::Deletion(err) => write!(f, "project deletion failed: {err}"),
        }
    }
}

impl RepositoryError for ProjectCrudRepositoryError {}

pub struct ProjectCrudRepository {
    fallback: SeaOrmRepo,
    deletion: ProjectDeletionService,
    service: Arc<ProjectService>,
}

impl ProjectCrudRepository {
    pub(crate) fn new(
        store: &Store,
        deletion: ProjectDeletionService,
        service: Arc<ProjectService>,
    ) -> Self {
        Self {
            fallback: SeaOrmRepo::new(store.db()),
            deletion,
            service,
        }
    }
}

impl ProjectCrudRepository {
    async fn load_model(&self, id: i64) -> Result<project::Model, ProjectCrudRepositoryError> {
        let condition = crudkit_core::condition::Condition::All(vec![
            crudkit_core::condition::ConditionElement::Clause(
                crudkit_core::condition::ConditionClause {
                    column_name: "id".to_owned(),
                    operator: crudkit_core::condition::Operator::Equal,
                    value: crudkit_core::condition::ConditionClauseValue::I64(id),
                },
            ),
        ]);
        self.fetch_one(None, None, None, Some(&condition))
            .await?
            .ok_or_else(|| {
                ProjectCrudRepositoryError::Service(format!(
                    "project {id} disappeared after its mutation"
                ))
            })
    }
}

impl Repository<CrudProjectResource> for ProjectCrudRepository {
    type Error = ProjectCrudRepositoryError;

    async fn insert(
        &self,
        create_model: project::CreateModel,
    ) -> Result<project::Model, Self::Error> {
        let input = create_input(&create_model)
            .map_err(|error| ProjectCrudRepositoryError::Service(error.to_string()))?;
        let project = self
            .service
            .create(input)
            .await
            .map_err(|error| ProjectCrudRepositoryError::Service(error.to_string()))?;
        self.load_model(project.id).await
    }

    async fn count(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<project::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<u64, Self::Error> {
        <SeaOrmRepo as Repository<CrudProjectResource>>::count(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(ProjectCrudRepositoryError::SeaOrm)
    }

    async fn fetch_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<project::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<project::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudProjectResource>>::fetch_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(ProjectCrudRepositoryError::SeaOrm)
    }

    async fn fetch_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<project::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<project::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudProjectResource>>::fetch_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(ProjectCrudRepositoryError::SeaOrm)
    }

    async fn read_one(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<project::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Option<project::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudProjectResource>>::read_one(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(ProjectCrudRepositoryError::SeaOrm)
    }

    async fn read_many(
        &self,
        limit: Option<u64>,
        skip: Option<u64>,
        order_by: Option<IndexMap<project::read_view::ModelField, Order>>,
        condition: Option<&Condition>,
    ) -> Result<Vec<project::read_view::Model>, Self::Error> {
        <SeaOrmRepo as Repository<CrudProjectResource>>::read_many(
            &self.fallback,
            limit,
            skip,
            order_by,
            condition,
        )
        .await
        .map_err(ProjectCrudRepositoryError::SeaOrm)
    }

    async fn update(
        &self,
        existing: project::Model,
        update_model: project::UpdateModel,
    ) -> Result<project::Model, Self::Error> {
        let (update, settings) = update_input(&update_model)
            .map_err(|error| ProjectCrudRepositoryError::Service(error.to_string()))?;
        let project = self
            .service
            .edit(&existing.name, Some(existing.id), update, Some(settings))
            .await
            .map_err(|error| ProjectCrudRepositoryError::Service(error.to_string()))?;
        self.load_model(project.id).await
    }

    async fn delete(&self, model: project::Model) -> Result<DeleteResult, Self::Error> {
        self.deletion
            .delete_by_id(model.id)
            .await
            .map_err(|err| ProjectCrudRepositoryError::Deletion(err.to_string()))?;
        Ok(DeleteResult {
            entities_affected: 1,
        })
    }
}

#[derive(Debug)]
pub struct ProjectLifetime;

impl CrudLifetime<CrudProjectResource> for ProjectLifetime {
    type Error = ProjectHookError;

    async fn before_read(
        _read_request: &mut ReadRequest<CrudProjectResource>,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_read(
        _read_request: &ReadRequest<CrudProjectResource>,
        _read_result: &mut ReadResult<CrudProjectResource>,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_create(
        create_model: &mut project::CreateModel,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        let input = create_input(create_model)
            .map_err(|error| project_unprocessable_error(error.to_string()))?;
        _context
            .service
            .validate_create(input)
            .map_err(|error| project_unprocessable_error(error.to_string()))?;
        Ok(data)
    }

    async fn after_create(
        _create_model: &project::CreateModel,
        _model: &project::Model,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_update(
        existing: &project::Model,
        update_model: &mut project::UpdateModel,
        _update_request: &UpdateRequest,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        let (update, settings) = update_input(update_model)
            .map_err(|error| project_unprocessable_error(error.to_string()))?;
        let project = super::super::repository::encoding::decode_for_edit(existing.clone())
            .map_err(|error| project_unprocessable_error(error.to_string()))?;
        _context
            .service
            .validate_edit(&project, update, settings)
            .map_err(|error| project_unprocessable_error(error.to_string()))?;
        Ok(data)
    }

    async fn after_update(
        _update_model: &project::UpdateModel,
        _model: &project::Model,
        _update_request: &UpdateRequest,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        Ok(data)
    }

    async fn before_delete(
        _model: &project::Model,
        _delete_request: &DeleteRequest<CrudProjectResource>,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        Ok(data)
    }

    async fn after_delete(
        _model: &project::Model,
        _delete_request: &DeleteRequest<CrudProjectResource>,
        _context: &ProjectResourceContext,
        _request: RequestContext<NoAuth>,
        data: ProjectHookData,
    ) -> Result<ProjectHookData, HookError<Self::Error>> {
        Ok(data)
    }
}

#[derive(Debug, ToSchema)]
pub struct CrudProjectResource;

impl CrudResource for CrudProjectResource {
    type ReadModel = project::read_view::Model;
    type ReadModelId = project::read_view::ModelId;
    type ReadModelField = project::read_view::ModelField;

    type CreateModel = project::CreateModel;
    type CreateModelField = project::ModelField;

    type UpdateModel = project::UpdateModel;
    type UpdateModelField = project::ModelField;

    type Model = project::Model;
    type Id = project::ProjectId;
    type ModelField = project::ModelField;

    type Repository = ProjectCrudRepository;
    type ValidationResultRepository =
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository;
    type CollaborationService = NoopCollaborationService;
    type Context = ProjectResourceContext;
    type HookData = ProjectHookData;
    type Lifetime = ProjectLifetime;
    type Auth = NoAuth;
    type AuthPolicy = OpenAuthPolicy;
    type ResourceType = CrudResources;
    const TYPE: CrudResources = CrudResources::Project;
}

impl SeaOrmResource for CrudProjectResource {
    type Entity = project::Entity;
    type SeaOrmModel = project::Model;
    type ActiveModel = project::ActiveModel;
    type Column = project::Column;
    type PrimaryKey = <project::Entity as EntityTrait>::PrimaryKey;

    type ReadViewEntity = project::read_view::Entity;
    type ReadViewSeaOrmModel = project::read_view::Model;
    type ReadViewActiveModel = project::read_view::ActiveModel;
    type ReadViewColumn = project::read_view::Column;
    type ReadViewPrimaryKey = <project::read_view::Entity as EntityTrait>::PrimaryKey;

    fn model_field_to_column(field: &Self::ModelField) -> Self::Column {
        <project::ModelField as CrudColumns<project::Column>>::to_sea_orm_column(field)
    }

    fn read_model_field_to_column(field: &Self::ReadModelField) -> Self::ReadViewColumn {
        <project::read_view::ModelField as CrudColumns<project::read_view::Column>>::to_sea_orm_column(field)
    }
}

fn project_unprocessable_error(reason: String) -> HookError<ProjectHookError> {
    HookError::UnprocessableEntity { reason }
}

fn create_input(model: &project::CreateModel) -> rootcause::Result<CreateProject> {
    Ok(CreateProject {
        name: model.name.clone(),
        display_name: Some(model.display_name.clone()),
        path: PathBuf::from(model.path.as_deref().unwrap_or_default()),
        default_agent_model: model.default_agent_model.clone(),
        default_agent_reasoning_effort: settings::normalize_optional(
            model.default_agent_reasoning_effort.clone(),
        )
        .map(|value| value.parse())
        .transpose()?,
        system_prompt: None,
        memory: None,
    })
}

fn update_input(
    model: &project::UpdateModel,
) -> rootcause::Result<(UpdateProject, UpdateProjectSettings)> {
    Ok((
        UpdateProject {
            display_name: Some(model.display_name.clone()),
            path: Some(match &model.path {
                Some(path) => super::super::ProjectPathUpdate::Set(PathBuf::from(path)),
                None => super::super::ProjectPathUpdate::Clear,
            }),
        },
        UpdateProjectSettings {
            knowledge_directory: Some(model.knowledge_directory.clone()),
            workspace_mode: Some(model.workspace_mode.parse()?),
            max_code_edit_agents: Some(model.max_code_edit_agents),
            max_read_only_agents: Some(model.max_read_only_agents),
            create_pr: Some(model.create_pr),
            auto_commit: Some(model.auto_commit),
            commit_standard: Some(model.commit_standard.clone()),
            revert_strategy: Some(model.revert_strategy.parse()?),
            stale_claim_minutes: Some(model.stale_claim_minutes),
            worktree_cleanup_policy: Some(model.worktree_cleanup_policy.parse()?),
            default_agent_tool: Some(model.default_agent_tool.parse()?),
            default_agent_model: Some(model.default_agent_model.clone()),
            default_agent_reasoning_effort: Some(
                settings::normalize_optional(model.default_agent_reasoning_effort.clone())
                    .map(|value| value.parse())
                    .transpose()?,
            ),
            agent_sandbox_mode: Some(model.agent_sandbox_mode.parse()?),
            agent_extra_writable_roots: Some(settings::parse_agent_extra_writable_roots_text(
                &model.agent_extra_writable_roots,
            )?),
            agent_git_command_policy: Some(
                super::super::repository::encoding::parse_agent_git_command_policy_storage(
                    &model.agent_git_command_policy,
                )?,
            ),
        },
    ))
}

crudkit_rs::impl_add_crud_routes!(
    crate::backend::projects::transport::crud::CrudProjectResource,
    project
);
pub(crate) fn routes() -> axum::Router {
    axum_project_crud_routes::add_crud_routes("/api", axum::Router::new())
}
