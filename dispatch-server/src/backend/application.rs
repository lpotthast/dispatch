//! The application composition root. Construction does not install process-global application state.
use crate::backend::{
    app_state::AppState,
    automation::supervisor::AutomationSupervisor,
    crudkit_resources::{self, CrudContexts},
    execution::codex::runtime as codex_app_server,
    execution::sessions::ProcessSessionRegistry,
    projects,
    projects::deletion::ProjectDeletionService,
    storage::Store,
};
use rootcause::{Result, prelude::*};
use std::{path::PathBuf, sync::Arc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub(crate) struct Application {
    pub(crate) state: AppState,
    pub(crate) contexts: CrudContexts,
    shutdown: CancellationToken,
    workers: Vec<JoinHandle<()>>,
}

impl Application {
    pub(crate) async fn open(database: PathBuf, api_url: String) -> Result<Self> {
        Ok(Self::from_store(Store::open(database).await?, api_url))
    }

    pub(crate) fn from_store(store: Store, api_url: String) -> Self {
        let event_bus = crate::backend::events::UiEventBus::new();
        let transactions = Arc::new(crate::backend::storage::TransactionManager::new(&store));
        let project_repository = Arc::new(projects::repository::ProjectRepository::new(store.db()));
        let project_service = Arc::new(projects::service::ProjectService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(projects::repository::ProjectDefaultsRepository::new()),
            Arc::new(projects::runtime::ProjectRuntime),
            event_bus.clone(),
            store.path().to_owned(),
        ));
        let revision_queries = Arc::new(
            crate::backend::automation::revisions::service::RevisionQueryService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::automation::rules::repository::RuleRepository),
                Arc::new(
                    crate::backend::automation::revisions::repository::RevisionQueryRepository,
                ),
            ),
        );
        let rules = Arc::new(
            crate::backend::automation::rules::service::RuleService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::automation::rules::repository::RuleRepository),
                Arc::new(
                    crate::backend::automation::personalities::repository::PersonalityRepository,
                ),
                event_bus.clone(),
            ),
        );
        let personalities = Arc::new(
            crate::backend::automation::personalities::service::PersonalityService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(
                    crate::backend::automation::personalities::repository::PersonalityRepository,
                ),
                event_bus.clone(),
            ),
        );
        let bundles = Arc::new(crate::backend::automation::bundles::service::BundleService::new(transactions.clone(), project_repository.clone(), Arc::new(crate::backend::automation::bundles::repository::BundleRepository::new(Arc::new(crate::backend::automation::rules::repository::RuleRepository), Arc::new(crate::backend::automation::personalities::repository::PersonalityRepository))), rules.clone(), personalities.clone(), event_bus.clone()));
        let tools = Arc::new(crate::backend::execution::tools::service::ToolService::new(
            transactions.clone(),
            Arc::new(crate::backend::execution::tools::repository::ToolRepository),
            Arc::new(
                crate::backend::execution::tools::runtime::ToolDiscovery::new(std::env::var_os(
                    "PATH",
                )),
            ),
            event_bus.clone(),
        ));
        let run_admission = Arc::new(
            crate::backend::runs::admission::service::RunAdmissionService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::runs::admission::repository::RunAdmissionRepository),
            ),
        );
        let routing = Arc::new(
            crate::backend::automation::routing::service::RoutingService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::automation::rules::repository::RuleRepository),
                Arc::new(crate::backend::items::repository::ItemRepository),
                Arc::new(crate::backend::automation::routing::repository::RoutingRepository),
                run_admission.clone(),
            ),
        );
        let postconditions = Arc::new(
            crate::backend::automation::postconditions::service::PostconditionService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(
                    crate::backend::automation::postconditions::repository::PostconditionRepository,
                ),
                Arc::new(crate::backend::items::repository::ItemRepository),
            ),
        );
        let sessions = ProcessSessionRegistry::new(event_bus.clone());
        let run_queries = Arc::new(
            crate::backend::runs::queries::service::RunQueryService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::runs::queries::repository::RunQueryRepository),
                Arc::new(crate::backend::runs::queries::runtime::RunArtifacts),
                sessions.clone(),
                tools.clone(),
                Arc::new(crate::backend::runs::admission::repository::RunAdmissionRepository),
            ),
        );
        let workspaces = Arc::new(
            crate::backend::execution::workspaces::service::WorkspaceService::new(
                project_repository.clone(),
                run_queries.clone(),
                Arc::new(
                    crate::backend::execution::workspaces::runtime::WorkspaceRuntime::from_env(),
                ),
                store.path().to_path_buf(),
            ),
        );
        let codex_home =
            crate::backend::execution::codex::runtime::codex_home_dir_for_dispatch_home(
                &crate::backend::storage::dispatch_home_dir(),
            );
        let codex = Arc::new(
            crate::backend::execution::codex::service::CodexService::new(
                tools.clone(),
                codex_home.clone(),
                event_bus.clone(),
                sessions.clone(),
            ),
        );
        let attribution = Arc::new(
            crate::backend::attribution::service::AttributionService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::attribution::repository::AttributionRepository::new()),
            ),
        );
        let claims = Arc::new(crate::backend::items::claims::service::ClaimService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::items::claims::repository::ClaimRepository),
            Arc::new(crate::backend::items::repository::ItemRepository),
            Arc::new(crate::backend::runs::launch::repository::AgentRunLaunchRepository),
            attribution.clone(),
            event_bus.clone(),
        ));
        let runs = Arc::new(crate::backend::runs::service::RunService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::runs::repository::RunRepository),
            claims.clone(),
            Arc::new(crate::backend::runs::queries::runtime::RunArtifacts),
            event_bus.clone(),
        ));
        let automation_supervisor = AutomationSupervisor::new(
            project_repository.clone(),
            runs.clone(),
            sessions.clone(),
            event_bus.clone(),
        );
        let git_runtime = Arc::new(crate::backend::execution::git::GitRuntime::new(
            std::env::var_os("PATH"),
        ));
        let cli = Arc::new(crate::backend::execution::cli::CliService::new(
            crate::backend::execution::cli::CliConfig::from_env(),
        ));
        let execution = Arc::new(crate::backend::execution::AgentExecutionService::new(
            api_url,
            git_runtime.clone(),
            std::sync::Arc::new(crate::backend::execution::runtime::AgentRuntime),
            Some(sessions.clone()),
        ));
        let document_writer =
            Arc::new(crate::backend::knowledge::editing::DocumentWriter::default());
        let knowledge_files = Arc::new(crate::backend::knowledge::runtime::KnowledgeFiles::new(
            document_writer.clone(),
        ));
        let job_files = Arc::new(crate::backend::knowledge::jobs::files::JobFiles::new(
            store
                .path()
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."))
                .join("knowledge-artifacts/projects"),
            document_writer,
        ));
        let knowledge_execution = Arc::new(
            crate::backend::knowledge::jobs::execution::KnowledgePassService::new(
                runs.clone(),
                codex.clone(),
                tools.clone(),
                execution.clone(),
                cli.clone(),
                git_runtime.clone(),
                sessions.clone(),
                Arc::new(crate::backend::knowledge::jobs::execution_files::PassFiles),
            ),
        );
        let jobs = Arc::new(crate::backend::knowledge::jobs::service::JobService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::knowledge::jobs::repository::JobRepository),
            job_files,
            runs.clone(),
            run_admission.clone(),
            attribution.clone(),
            knowledge_execution,
            sessions.clone(),
            event_bus.clone(),
        ));
        let knowledge = Arc::new(crate::backend::knowledge::service::KnowledgeService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::knowledge::repository::KnowledgeRepository),
            knowledge_files.clone(),
            attribution.clone(),
        ));
        let knowledge_queries = Arc::new(
            crate::backend::knowledge::queries::KnowledgeQueryService::new(
                knowledge.clone(),
                jobs.clone(),
            ),
        );
        let run_control = Arc::new(crate::backend::runs::control::RunControlService::new(
            run_queries.clone(),
            jobs.clone(),
            sessions.clone(),
        ));
        let run_workspaces =
            Arc::new(crate::backend::execution::workspaces::runs::RunWorkspaceRuntime);
        let run_cleanup = Arc::new(crate::backend::runs::cleanup::RunCleanupRuntime::new(
            crate::backend::execution::automation_log_dir(),
            run_workspaces.clone(),
        ));
        let run_administration = Arc::new(
            crate::backend::runs::administration::RunAdministrationService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::runs::repository::RunRepository),
                runs.clone(),
                jobs.clone(),
                run_admission.clone(),
                sessions.clone(),
                run_cleanup.clone(),
            ),
        );
        let project_deletion = ProjectDeletionService::new(
            runs.clone(),
            run_admission.clone(),
            event_bus.clone(),
            jobs.clone(),
            automation_supervisor.clone(),
            sessions.clone(),
            project_repository.clone(),
            transactions.clone(),
            Arc::new(projects::runtime::ProjectDeletionRuntime::new(
                run_cleanup,
                codex_home.join("projects"),
                crate::backend::storage::dispatch_home_dir().join("knowledge-artifacts/projects"),
            )),
        );
        let states = Arc::new(crate::backend::items::states::service::StateService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::items::states::repository::StateRepository),
            event_bus.clone(),
        ));
        let lanes = Arc::new(crate::backend::board::lanes::service::LaneService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::board::lanes::repository::LaneRepository),
            event_bus.clone(),
        ));
        let groups = Arc::new(crate::backend::items::groups::service::GroupService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::items::groups::repository::GroupRepository),
            attribution.clone(),
            event_bus.clone(),
        ));
        let label_catalog = Arc::new(
            crate::backend::items::labels::catalog::service::CatalogService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::items::labels::catalog::repository::CatalogRepository),
                event_bus.clone(),
            ),
        );
        let labels = Arc::new(crate::backend::items::labels::service::LabelService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::items::labels::repository::LabelRepository),
            Arc::new(crate::backend::items::repository::ItemRepository),
            attribution.clone(),
            event_bus.clone(),
        ));
        let items = Arc::new(crate::backend::items::service::ItemService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::items::repository::ItemRepository),
            Arc::new(crate::backend::items::events::repository::EventRepository),
            attribution.clone(),
            event_bus.clone(),
        ));
        let item_creation = Arc::new(
            crate::backend::items::creation::service::ItemCreationService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::items::creation::repository::ItemCreationRepository),
                attribution.clone(),
                event_bus.clone(),
            ),
        );
        let production = Arc::new(
            crate::backend::automation::production::service::ProductionService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::items::repository::ItemRepository),
                item_creation.clone(),
                Arc::new(crate::backend::automation::production::repository::ProductionRepository),
                event_bus.clone(),
            ),
        );
        let comments = Arc::new(crate::backend::comments::service::CommentService::new(
            transactions.clone(),
            project_repository.clone(),
            Arc::new(crate::backend::comments::repository::CommentRepository),
            attribution.clone(),
            event_bus.clone(),
        ));
        let relationships = Arc::new(
            crate::backend::relationships::service::RelationshipService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::relationships::repository::RelationshipRepository),
                attribution.clone(),
                event_bus.clone(),
            ),
        );
        let contexts = crudkit_resources::build_contexts(
            store.clone(),
            project_deletion.clone(),
            project_service.clone(),
            comments.clone(),
            item_creation.clone(),
            states.clone(),
            lanes.clone(),
            label_catalog.clone(),
            tools.clone(),
            items.clone(),
            personalities.clone(),
            rules.clone(),
            Arc::new(
                crate::backend::runs::transport::crud::AgentRunCrudRepository::new(
                    store.db(),
                    runs.clone(),
                    run_administration,
                ),
            ),
        );
        let codex_status = Default::default();
        let codex_status = Arc::new(tokio::sync::RwLock::new(codex_status));
        let launch = Arc::new(
            crate::backend::automation::launch::service::LaunchService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::items::repository::ItemRepository),
                runs.clone(),
                postconditions.clone(),
                personalities.clone(),
                codex.clone(),
                tools.clone(),
                claims.clone(),
                run_admission.clone(),
                execution.clone(),
                cli.clone(),
                git_runtime.clone(),
                knowledge_files.clone(),
                Arc::new(
                    crate::backend::automation::launch::runtime::LaunchRuntime::new(
                        crate::backend::execution::automation_log_dir(),
                        run_workspaces.clone(),
                        Arc::new(crate::backend::automation::launch::commit::CommitRuntime),
                    ),
                ),
                event_bus.clone(),
                Some(sessions.clone()),
                Some(codex_status.clone()),
            ),
        );
        let scheduler = Arc::new(
            crate::backend::automation::scheduling::service::SchedulerService::new(
                transactions.clone(),
                project_repository.clone(),
                Arc::new(crate::backend::automation::scheduling::repository::ScheduleRepository),
                Arc::new(crate::backend::items::repository::ItemRepository),
                claims.clone(),
                production.clone(),
                run_admission.clone(),
                launch.clone(),
                event_bus.clone(),
            ),
        );
        let operator_queries = Arc::new(
            crate::backend::operator::queries::OperatorQueryService::new(
                codex.clone(),
                run_queries.clone(),
                states.clone(),
                labels.clone(),
                items.clone(),
                project_service.clone(),
                automation_supervisor.clone(),
                codex_status.clone(),
                run_admission.clone(),
                sessions.clone(),
                comments.clone(),
                relationships.clone(),
                workspaces.clone(),
                personalities.clone(),
            ),
        );
        let board_queries = Arc::new(crate::backend::board::queries::BoardQueryService::new(
            run_queries.clone(),
            label_catalog.clone(),
            states.clone(),
            lanes.clone(),
            labels.clone(),
            items.clone(),
            project_service.clone(),
            automation_supervisor.clone(),
            codex_status.clone(),
            operator_queries.clone(),
        ));
        let shutdown = CancellationToken::new();
        let project_controller = Arc::new(
            crate::backend::projects::controller::ProjectController::new(
                project_deletion.clone(),
                project_service.clone(),
            ),
        );
        let item_controller = Arc::new(crate::backend::items::controller::ItemController::new(
            attribution.clone(),
            item_creation.clone(),
            items.clone(),
        ));
        let claim_controller = Arc::new(
            crate::backend::items::claims::controller::ClaimController::new(claims.clone()),
        );
        let label_controller = Arc::new(
            crate::backend::items::labels::controller::LabelController::new(labels.clone()),
        );
        let group_controller = Arc::new(
            crate::backend::items::groups::controller::GroupController::new(groups.clone()),
        );
        let comment_controller = Arc::new(
            crate::backend::comments::controller::CommentController::new(comments.clone()),
        );
        let relationship_controller = Arc::new(
            crate::backend::relationships::controller::RelationshipController::new(
                relationships.clone(),
            ),
        );
        let run_controller = Arc::new(crate::backend::runs::controller::RunController::new(
            run_control.clone(),
            run_queries.clone(),
        ));
        let automation_controller = Arc::new(
            crate::backend::automation::controller::AutomationController::new(
                automation_supervisor.clone(),
                claims.clone(),
                launch.clone(),
            ),
        );
        let personality_controller = Arc::new(
            crate::backend::automation::personalities::controller::PersonalityController::new(
                personalities.clone(),
            ),
        );
        let rule_controller = Arc::new(
            crate::backend::automation::rules::controller::RuleController::new(
                attribution.clone(),
                rules.clone(),
            ),
        );
        let revision_controller = Arc::new(
            crate::backend::automation::revisions::controller::RevisionController::new(
                revision_queries.clone(),
            ),
        );
        let bundle_controller = Arc::new(
            crate::backend::automation::bundles::controller::BundleController::new(bundles.clone()),
        );
        let routing_controller = Arc::new(
            crate::backend::automation::routing::controller::RoutingController::new(
                attribution.clone(),
                routing.clone(),
            ),
        );
        let workspace_controller = Arc::new(
            crate::backend::execution::workspaces::controller::WorkspaceController::new(
                workspaces.clone(),
            ),
        );
        let codex_controller = Arc::new(
            crate::backend::execution::codex::controller::CodexController::new(
                codex.clone(),
                codex_status.clone(),
            ),
        );
        let knowledge_controller = Arc::new(
            crate::backend::knowledge::controller::KnowledgeController::new(
                knowledge_queries.clone(),
            ),
        );
        let knowledge_job_controller = Arc::new(
            crate::backend::knowledge::jobs::controller::KnowledgeJobController::new(jobs.clone()),
        );
        let event_controller = Arc::new(crate::backend::events::controller::EventController::new(
            event_bus.clone(),
        ));
        let state = AppState {
            event_controller,
            project_controller,
            item_controller,
            claim_controller,
            label_controller,
            group_controller,
            comment_controller,
            relationship_controller,
            run_controller,
            automation_controller,
            personality_controller,
            rule_controller,
            revision_controller,
            bundle_controller,
            routing_controller,
            workspace_controller,
            codex_controller,
            knowledge_controller,
            knowledge_job_controller,
            run_control,
            knowledge_queries,
            knowledge,
            jobs,
            board_queries,
            operator_queries,
            scheduler,
            launch,
            bundles,
            workspaces,
            runs,
            run_queries,
            postconditions,
            routing,
            revision_queries,
            rules: rules.clone(),
            personalities: personalities.clone(),
            tools: tools.clone(),
            claims,
            label_catalog,
            lanes,
            states,
            groups,
            labels,
            items,
            item_creation,
            production,
            run_admission,
            comments,
            relationships,
            projects: project_service,
            execution,
            events: event_bus,
            attribution,
            store: store.clone(),
            sessions: sessions.clone(),
            automation_supervisor: automation_supervisor.clone(),
            project_deletion,
            codex_status: codex_status.clone(),
            codex: codex.clone(),
        };

        Self {
            state,
            contexts,
            shutdown,
            workers: Vec::new(),
        }
    }

    pub(crate) async fn start_workers(&mut self) -> Result<()> {
        if !self.workers.is_empty() {
            bail!("application workers are already running");
        }
        let codex_status = self.state.codex.readiness().await;
        if !codex_status.usable {
            tracing::warn!(
                "{}",
                codex_app_server::operator_guidance(&codex_status).join("\n")
            );
        }
        *self.state.codex_status.write().await = codex_status;
        let shutdown = self.shutdown.child_token();
        self.state.jobs.recover().await?;
        self.workers.push(
            crate::backend::knowledge::jobs::worker::JobWorker::new(self.state.jobs.clone())
                .spawn_until(shutdown.clone()),
        );
        self.workers.push(projects::spawn_path_status_checker_until(
            self.state.projects.clone(),
            shutdown.clone(),
        ));
        self.workers.push(
            crate::backend::automation::scheduling::worker::SchedulerWorker::new(
                self.state.scheduler.clone(),
                self.state.claims.clone(),
                self.state.automation_supervisor.clone(),
            )
            .spawn_until(shutdown),
        );
        Ok(())
    }

    pub(crate) fn shutdown_token(&self) -> CancellationToken {
        self.shutdown.clone()
    }

    pub(crate) async fn finish_workers(mut self) -> Result<()> {
        self.shutdown.cancel();
        self.state.automation_supervisor.shutdown_all().await;
        cancel_active_sessions(self.state.runs.clone(), &self.state.sessions).await;
        let mut failures = Vec::new();
        for worker in &mut self.workers {
            if let Err(error) = worker.await {
                failures.push(error.to_string());
            }
        }
        if !failures.is_empty() {
            bail!(
                "application workers failed during shutdown: {}",
                failures.join("; ")
            );
        }
        Ok(())
    }
}

impl Drop for Application {
    fn drop(&mut self) {
        self.shutdown.cancel();
        self.state.sessions.cancel_all();
        for worker in &self.workers {
            worker.abort();
        }
    }
}

const ACTIVE_SESSION_SHUTDOWN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
pub(crate) async fn cancel_active_sessions(
    run_service: Arc<crate::backend::runs::service::RunService>,
    sessions: &ProcessSessionRegistry,
) {
    let active = sessions.list_all();
    let mut projects = active
        .into_iter()
        .map(|session| (session.project_id, session.project_name))
        .collect::<Vec<_>>();
    projects.sort();
    projects.dedup();

    sessions.cancel_all();
    if let Err(_elapsed) = tokio::time::timeout(ACTIVE_SESSION_SHUTDOWN_TIMEOUT, async {
        loop {
            if sessions.list_all().is_empty() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    {
        tracing::warn!("timed out waiting for active automation sessions to stop");
    }

    for (project_id, project_name) in projects {
        if let Err(err) = run_service.clone().cancel_project(project_id).await {
            tracing::error!(
                project = %project_name,
                error = %format_args!("{err:#}"),
                "failed to mark running automation cancelled"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::projects::CreateProject;
    use crate::shared::view_models::UiEvent;
    use assertr::prelude::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn independently_constructed_applications_keep_state_events_and_urls_separate() {
        let temp = TempDir::new().unwrap();
        let first = Application::open(
            temp.path().join("first.sqlite3"),
            "http://127.0.0.1:4101".into(),
        )
        .await
        .unwrap();
        let second = Application::open(
            temp.path().join("second.sqlite3"),
            "http://127.0.0.1:4102".into(),
        )
        .await
        .unwrap();
        let mut first_events = first.state.events.subscribe();
        let mut second_events = second.state.events.subscribe();
        first
            .state
            .projects
            .create(CreateProject {
                name: "first".into(),
                display_name: None,
                path: temp.path().to_owned(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
        assert_that!(&second.state.projects.list().await.unwrap()).is_empty();
        assert_that!(&second_events.try_recv().is_err()).is_true();
        assert_that!(&matches!(
            first_events.try_recv().unwrap(),
            UiEvent::ProjectListChanged { sequence: 1, .. }
        ))
        .is_true();
        second.state.events.publish_project_list_changed();
        assert_that!(&matches!(
            second_events.try_recv().unwrap(),
            UiEvent::ProjectListChanged { sequence: 1, .. }
        ))
        .is_true();
        assert_that!(&first.state.execution.api_url()).is_equal_to("http://127.0.0.1:4101");
        assert_that!(&second.state.execution.api_url()).is_equal_to("http://127.0.0.1:4102");
        let first_owner = leptos::prelude::Owner::new();
        first_owner.with(|| leptos::prelude::provide_context(first.state.clone()));
        let second_owner = leptos::prelude::Owner::new();
        second_owner.with(|| leptos::prelude::provide_context(second.state.clone()));
        let first_url = first_owner.with(|| {
            leptos::prelude::expect_context::<AppState>()
                .execution
                .api_url()
                .to_owned()
        });
        let second_url = second_owner.with(|| {
            leptos::prelude::expect_context::<AppState>()
                .execution
                .api_url()
                .to_owned()
        });
        assert_that!(&first_url).is_equal_to("http://127.0.0.1:4101");
        assert_that!(&second_url).is_equal_to("http://127.0.0.1:4102");
    }

    #[tokio::test]
    async fn rejected_mutation_publishes_no_success_event_or_partial_history() {
        let temp = TempDir::new().unwrap();
        let app = Application::open(
            temp.path().join("dispatch.sqlite3"),
            "http://127.0.0.1:4103".into(),
        )
        .await
        .unwrap();
        let create = CreateProject {
            name: "demo".into(),
            display_name: None,
            path: temp.path().to_owned(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: Some("Original instructions".into()),
            memory: None,
        };
        app.state.projects.create(create.clone()).await.unwrap();
        let history = app
            .state
            .projects
            .system_prompt_events("demo")
            .await
            .unwrap();
        let mut events = app.state.events.subscribe();
        assert_that!(&app.state.projects.create(create).await.is_err()).is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(
            &app.state
                .projects
                .system_prompt_events("demo")
                .await
                .unwrap()
        )
        .is_equal_to(history);
        assert_that!(&app.state.projects.list().await.unwrap().len()).is_equal_to(1);
    }

    #[tokio::test]
    async fn shutdown_waits_for_owned_workers() {
        let temp = TempDir::new().unwrap();
        let mut app = Application::open(
            temp.path().join("dispatch.sqlite3"),
            "http://127.0.0.1:4104".into(),
        )
        .await
        .unwrap();
        let shutdown = app.shutdown.child_token();
        let completed = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let completed_worker = completed.clone();
        app.workers.push(tokio::spawn(async move {
            shutdown.cancelled().await;
            completed_worker.store(true, std::sync::atomic::Ordering::SeqCst);
        }));
        app.finish_workers().await.unwrap();
        assert_that!(&completed.load(std::sync::atomic::Ordering::SeqCst)).is_true();
    }
}
