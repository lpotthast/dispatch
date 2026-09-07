use crate::backend::automation::personalities::transport::crud::{
    CrudPersonalityResource, PersonalityCrudRepository, PersonalityResourceContext,
};
use crate::backend::automation::rules::transport::crud::{
    AutomationTriggerCrudRepository, AutomationTriggerResourceContext,
    CrudAutomationTriggerResource,
};
use crate::backend::board::lanes::transport::{
    CrudSwimLaneResource, SwimLaneCrudRepository, SwimLaneResourceContext,
};
use crate::backend::comments::{
    service::CommentService,
    transport::crud::{CommentCrudRepository, CommentResourceContext, CrudCommentResource},
};
use crate::backend::execution::tools::transport::{
    AgentToolCrudRepository, AgentToolResourceContext, CrudAgentToolResource,
};
use crate::backend::items::labels::catalog::transport::{
    CrudLabelKeyResource, LabelKeyCrudRepository, LabelKeyResourceContext,
};
use crate::backend::items::states::transport::{
    CrudWorkItemStateResource, WorkItemStateCrudRepository, WorkItemStateResourceContext,
};
use crate::backend::items::transport::crud::{
    CrudWorkItemResource, WorkItemRepository, WorkItemResourceContext,
};
use crate::backend::runs::transport::crud::{AgentRunResourceContext, CrudAgentRunResource};
use std::{convert::Infallible, sync::Arc};

use crudkit_core::collaboration::CollabMessage;
use crudkit_rs::{
    collaboration::CollaborationService, prelude::*, resource::ResourceType,
    validate::GlobalValidationState,
};

use crate::backend::{
    projects::deletion::ProjectDeletionService,
    projects::{
        self,
        transport::crud::{CrudProjectResource, ProjectCrudRepository, ProjectResourceContext},
    },
    storage::Store,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrudResources {
    Project,
    WorkItem,
    Comment,
    AgentTool,
    AgentRun,
    AutomationTrigger,
    Personality,
    SwimLane,
    WorkItemState,
    LabelKey,
}

impl ResourceType for CrudResources {
    fn name(&self) -> &'static str {
        match self {
            Self::Project => "projects",
            Self::WorkItem => "work_items",
            Self::Comment => "comments",
            Self::AgentTool => "agent_tools",
            Self::AgentRun => "agent_runs",
            Self::AutomationTrigger => "automation_triggers",
            Self::Personality => "personalities",
            Self::SwimLane => "swim_lanes",
            Self::WorkItemState => "work_item_states",
            Self::LabelKey => "label_keys",
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct NoopCollaborationService;

impl CollaborationService for NoopCollaborationService {
    type Error = Infallible;

    async fn broadcast_json(&self, _json: CollabMessage) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Clone)]
pub struct CrudContexts {
    pub project: Arc<CrudContext<CrudProjectResource>>,
    pub work_item: Arc<CrudContext<CrudWorkItemResource>>,
    pub comment: Arc<CrudContext<CrudCommentResource>>,
    pub agent_tool: Arc<CrudContext<CrudAgentToolResource>>,
    pub agent_run: Arc<CrudContext<CrudAgentRunResource>>,
    pub automation_trigger: Arc<CrudContext<CrudAutomationTriggerResource>>,
    pub personality: Arc<CrudContext<CrudPersonalityResource>>,
    pub swim_lane: Arc<CrudContext<CrudSwimLaneResource>>,
    pub work_item_state: Arc<CrudContext<CrudWorkItemStateResource>>,
    pub label_key: Arc<CrudContext<CrudLabelKeyResource>>,
}

#[allow(clippy::too_many_arguments)]
pub fn build_contexts(
    store: Store,
    project_deletion: ProjectDeletionService,
    project_service: Arc<projects::service::ProjectService>,
    comment_service: Arc<CommentService>,
    item_creation: Arc<crate::backend::items::creation::service::ItemCreationService>,
    states: Arc<crate::backend::items::states::service::StateService>,
    lanes: Arc<crate::backend::board::lanes::service::LaneService>,
    label_catalog: Arc<crate::backend::items::labels::catalog::service::CatalogService>,
    tools: Arc<crate::backend::execution::tools::service::ToolService>,
    items: Arc<crate::backend::items::service::ItemService>,
    personality_service: Arc<
        crate::backend::automation::personalities::service::PersonalityService,
    >,
    rule_service: Arc<crate::backend::automation::rules::service::RuleService>,
    runs: Arc<crate::backend::runs::transport::crud::AgentRunCrudRepository>,
) -> CrudContexts {
    let db = store.db();
    let project_repository = Arc::new(ProjectCrudRepository::new(
        &store,
        project_deletion,
        project_service.clone(),
    ));
    let validation_result_repository = Arc::new(
        crudkit_sea_orm::validation::unified::repository::UnifiedValidationRepository { db },
    );
    let collab_service = Arc::new(NoopCollaborationService);

    CrudContexts {
        project: Arc::new(CrudContext {
            res_context: Arc::new(ProjectResourceContext {
                service: project_service,
            }),
            repository: project_repository,
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        work_item: Arc::new(CrudContext {
            res_context: Arc::new(WorkItemResourceContext),
            repository: Arc::new(WorkItemRepository::new(
                store.db(),
                item_creation,
                items.clone(),
            )),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        comment: Arc::new(CrudContext {
            res_context: Arc::new(CommentResourceContext),
            repository: Arc::new(CommentCrudRepository::new(store.db(), comment_service)),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        agent_tool: Arc::new(CrudContext {
            res_context: Arc::new(AgentToolResourceContext),
            repository: Arc::new(AgentToolCrudRepository::new(store.db(), tools)),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        agent_run: Arc::new(CrudContext {
            res_context: Arc::new(AgentRunResourceContext),
            repository: runs,
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        automation_trigger: Arc::new(CrudContext {
            res_context: Arc::new(AutomationTriggerResourceContext {
                service: rule_service.clone(),
            }),
            repository: Arc::new(AutomationTriggerCrudRepository::new(
                store.db(),
                rule_service,
            )),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        personality: Arc::new(CrudContext {
            res_context: Arc::new(PersonalityResourceContext {
                service: personality_service.clone(),
            }),
            repository: Arc::new(PersonalityCrudRepository::new(
                store.db(),
                personality_service.clone(),
            )),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        swim_lane: Arc::new(CrudContext {
            res_context: Arc::new(SwimLaneResourceContext),
            repository: Arc::new(SwimLaneCrudRepository::new(store.db(), lanes)),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        work_item_state: Arc::new(CrudContext {
            res_context: Arc::new(WorkItemStateResourceContext),
            repository: Arc::new(WorkItemStateCrudRepository::new(store.db(), states)),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository: validation_result_repository.clone(),
            collab_service: collab_service.clone(),
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
        label_key: Arc::new(CrudContext {
            res_context: Arc::new(LabelKeyResourceContext {
                service: label_catalog.clone(),
            }),
            repository: Arc::new(LabelKeyCrudRepository::new(store.db(), label_catalog)),
            validators: vec![],
            resource_validators: vec![],
            validation_result_repository,
            collab_service,
            global_validation_state: Arc::new(GlobalValidationState::new()),
        }),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use tempfile::TempDir;

    use crate::backend::{
        projects::{CreateProject, repository::ProjectRepository},
        storage::Store,
    };

    pub(crate) async fn test_store() -> (TempDir, Store, i64) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
        let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
        (temp, store, project_id)
    }
}
