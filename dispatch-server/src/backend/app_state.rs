use std::sync::Arc;

use crate::{
    backend::{
        automation::supervisor::AutomationSupervisor, execution::sessions::ProcessSessionRegistry,
        projects::deletion::ProjectDeletionService, storage::Store,
    },
    shared::view_models::CodexAppServerStatusView,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) event_controller: Arc<crate::backend::events::controller::EventController>,
    pub(crate) project_controller: Arc<crate::backend::projects::controller::ProjectController>,
    pub(crate) item_controller: Arc<crate::backend::items::controller::ItemController>,
    pub(crate) claim_controller: Arc<crate::backend::items::claims::controller::ClaimController>,
    pub(crate) label_controller: Arc<crate::backend::items::labels::controller::LabelController>,
    pub(crate) group_controller: Arc<crate::backend::items::groups::controller::GroupController>,
    pub(crate) comment_controller: Arc<crate::backend::comments::controller::CommentController>,
    pub(crate) relationship_controller:
        Arc<crate::backend::relationships::controller::RelationshipController>,
    pub(crate) run_controller: Arc<crate::backend::runs::controller::RunController>,
    pub(crate) automation_controller:
        Arc<crate::backend::automation::controller::AutomationController>,
    pub(crate) personality_controller:
        Arc<crate::backend::automation::personalities::controller::PersonalityController>,
    pub(crate) rule_controller: Arc<crate::backend::automation::rules::controller::RuleController>,
    pub(crate) revision_controller:
        Arc<crate::backend::automation::revisions::controller::RevisionController>,
    pub(crate) bundle_controller:
        Arc<crate::backend::automation::bundles::controller::BundleController>,
    pub(crate) routing_controller:
        Arc<crate::backend::automation::routing::controller::RoutingController>,
    pub(crate) workspace_controller:
        Arc<crate::backend::execution::workspaces::controller::WorkspaceController>,
    pub(crate) codex_controller: Arc<crate::backend::execution::codex::controller::CodexController>,
    pub(crate) knowledge_controller:
        Arc<crate::backend::knowledge::controller::KnowledgeController>,
    pub(crate) knowledge_job_controller:
        Arc<crate::backend::knowledge::jobs::controller::KnowledgeJobController>,
    pub(crate) run_control: Arc<crate::backend::runs::control::RunControlService>,
    pub(crate) knowledge_queries: Arc<crate::backend::knowledge::queries::KnowledgeQueryService>,
    pub(crate) knowledge: Arc<crate::backend::knowledge::service::KnowledgeService>,
    pub(crate) jobs: Arc<crate::backend::knowledge::jobs::service::JobService>,
    pub(crate) board_queries: Arc<crate::backend::board::queries::BoardQueryService>,
    pub(crate) operator_queries: Arc<crate::backend::operator::queries::OperatorQueryService>,
    pub(crate) scheduler: Arc<crate::backend::automation::scheduling::service::SchedulerService>,
    pub(crate) launch: Arc<crate::backend::automation::launch::service::LaunchService>,
    pub(crate) routing: Arc<crate::backend::automation::routing::service::RoutingService>,
    pub(crate) postconditions:
        Arc<crate::backend::automation::postconditions::service::PostconditionService>,
    pub(crate) run_queries: Arc<crate::backend::runs::queries::service::RunQueryService>,
    pub(crate) runs: Arc<crate::backend::runs::service::RunService>,
    pub(crate) workspaces: Arc<crate::backend::execution::workspaces::service::WorkspaceService>,
    pub(crate) bundles: Arc<crate::backend::automation::bundles::service::BundleService>,
    pub(crate) revision_queries:
        Arc<crate::backend::automation::revisions::service::RevisionQueryService>,
    pub(crate) rules: Arc<crate::backend::automation::rules::service::RuleService>,
    pub(crate) personalities:
        Arc<crate::backend::automation::personalities::service::PersonalityService>,
    pub(crate) tools: Arc<crate::backend::execution::tools::service::ToolService>,
    pub(crate) claims: Arc<crate::backend::items::claims::service::ClaimService>,
    pub(crate) label_catalog: Arc<crate::backend::items::labels::catalog::service::CatalogService>,
    pub(crate) lanes: Arc<crate::backend::board::lanes::service::LaneService>,
    pub(crate) states: Arc<crate::backend::items::states::service::StateService>,
    pub(crate) groups: Arc<crate::backend::items::groups::service::GroupService>,
    pub(crate) labels: Arc<crate::backend::items::labels::service::LabelService>,
    pub(crate) items: Arc<crate::backend::items::service::ItemService>,
    pub(crate) item_creation: Arc<crate::backend::items::creation::service::ItemCreationService>,
    pub(crate) production: Arc<crate::backend::automation::production::service::ProductionService>,
    pub(crate) run_admission:
        std::sync::Arc<crate::backend::runs::admission::service::RunAdmissionService>,
    pub(crate) comments: Arc<crate::backend::comments::service::CommentService>,
    pub(crate) relationships: Arc<crate::backend::relationships::service::RelationshipService>,
    pub(crate) projects: Arc<crate::backend::projects::service::ProjectService>,
    pub(crate) execution: Arc<crate::backend::execution::AgentExecutionService>,
    pub(crate) events: crate::backend::events::UiEventBus,
    pub(crate) store: Store,
    pub(crate) attribution: Arc<crate::backend::attribution::service::AttributionService>,
    pub(crate) sessions: ProcessSessionRegistry,
    pub(crate) automation_supervisor: AutomationSupervisor,
    pub(crate) project_deletion: ProjectDeletionService,
    pub(crate) codex_status: Arc<tokio::sync::RwLock<CodexAppServerStatusView>>,
    pub(crate) codex: Arc<crate::backend::execution::codex::service::CodexService>,
}
