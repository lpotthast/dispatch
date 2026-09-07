use super::{model::ProjectScope, repository::ProjectRepository, runtime::ProjectDeletionRuntime};
use crate::backend::storage::TransactionManager;
use std::{sync::Arc, time::Duration};

use rootcause::{Result, prelude::*};

use crate::backend::{
    automation::supervisor::AutomationSupervisor,
    execution::sessions::{DeletionAdmissionRejection, ProcessSessionRegistry},
};

const SESSION_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);
const SESSION_SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Owns the complete project deletion lifecycle.
///
/// A project row is deleted only after its automation has stopped and every Dispatch-owned
/// filesystem or repository artifact has been removed. Every entry point, including CrudKit,
/// must use this service instead of deleting the project row directly. CrudKit invokes it through
/// `ProjectCrudRepository` after its before-delete validation has succeeded.
#[derive(Clone)]
pub(crate) struct ProjectDeletionService {
    runs: Arc<crate::backend::runs::service::RunService>,
    events: crate::backend::events::UiEventBus,
    jobs: Arc<crate::backend::knowledge::jobs::service::JobService>,
    automation_supervisor: AutomationSupervisor,
    sessions: ProcessSessionRegistry,
    repository: Arc<ProjectRepository>,
    transactions: Arc<TransactionManager>,
    runtime: Arc<ProjectDeletionRuntime>,
    run_admission: Arc<crate::backend::runs::admission::service::RunAdmissionService>,
}

impl ProjectDeletionService {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        runs: Arc<crate::backend::runs::service::RunService>,
        run_admission: Arc<crate::backend::runs::admission::service::RunAdmissionService>,
        events: crate::backend::events::UiEventBus,
        jobs: Arc<crate::backend::knowledge::jobs::service::JobService>,
        automation_supervisor: AutomationSupervisor,
        sessions: ProcessSessionRegistry,
        repository: Arc<ProjectRepository>,
        transactions: Arc<TransactionManager>,
        runtime: Arc<ProjectDeletionRuntime>,
    ) -> Self {
        Self {
            runs,
            run_admission,
            events,
            jobs,
            automation_supervisor,
            sessions,
            repository,
            transactions,
            runtime,
        }
    }

    pub(crate) async fn delete_by_name(&self, project_name: &str) -> Result<()> {
        let project = self.repository.scope_by_name(project_name).await?;
        self.delete_project(project).await
    }

    pub(crate) async fn delete_by_id(&self, id: i64) -> Result<()> {
        let project = self.repository.scope_by_id(id).await?;
        self.delete_project(project).await
    }

    pub(crate) async fn delete_project(&self, project: ProjectScope) -> Result<()> {
        let project_id = project.id;
        let project_name = project.name.clone();
        let admission = match self.sessions.begin_project_deletion(project_id) {
            Ok(admission) => admission,
            Err(DeletionAdmissionRejection::InProgress) => {
                bail!(
                    "project '{}' (id {}) deletion is already in progress",
                    project_name,
                    project_id
                );
            }
            Err(DeletionAdmissionRejection::AlreadyDeleted) => {
                bail!(
                    "project '{}' (id {}) was already deleted",
                    project_name,
                    project_id
                );
            }
        };

        self.delete_after_admission_closed(&project).await?;
        admission.mark_deleted();

        self.events
            .publish_project_deleted(project_id, &project_name);
        self.events.publish_project_list_changed();
        Ok(())
    }

    async fn delete_after_admission_closed(&self, project: &ProjectScope) -> Result<()> {
        let run_service = self.runs.clone();

        let _publication = self.run_admission.acquire().await;
        self.jobs.stop_project(project.id).await?;
        self.automation_supervisor
            .close_project(project.id, &project.name);
        self.wait_for_sessions_to_stop(project).await?;
        run_service.clone().cancel_project(project.id).await?;

        let transaction = self.transactions.begin().await?;
        let current = self
            .repository
            .scope_by_id_in(&transaction, project.id)
            .await?;
        if &current != project {
            bail!("project working copy changed during deletion; retry cleanup");
        }
        let runs = self
            .repository
            .run_artifacts_in(&transaction, project)
            .await?;
        transaction.commit().await?;
        let runtime = self.runtime.clone();
        let prepared = project.clone();
        let artifacts = self.jobs.project_artifacts(project.id);
        tokio::task::spawn_blocking(move || runtime.cleanup(&prepared, &runs, &artifacts))
            .await??;
        let transaction = self.transactions.begin().await?;
        let current = self
            .repository
            .scope_by_id_in(&transaction, project.id)
            .await?;
        if &current != project {
            bail!("project working copy changed during deletion; retry cleanup");
        }
        self.repository.delete_in(&transaction, &current).await?;
        transaction.commit().await?;
        Ok(())
    }

    async fn wait_for_sessions_to_stop(&self, project: &ProjectScope) -> Result<()> {
        tokio::time::timeout(SESSION_SHUTDOWN_TIMEOUT, async {
            loop {
                if self.sessions.list_for_project(project.id).is_empty() {
                    return;
                }
                tokio::time::sleep(SESSION_SHUTDOWN_POLL_INTERVAL).await;
            }
        })
        .await
        .map_err(|_| {
            report!(
                "timed out waiting for automation runs to stop for project '{}'",
                project.name
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::backend::storage::Store;
    use std::{
        borrow::Cow,
        fs,
        path::{Path, PathBuf},
        sync::Arc,
    };

    use super::*;
    use assertr::prelude::*;
    use crudkit_core::{
        id::Id,
        validation::violation::{Violation, Violations},
    };
    use crudkit_rs::prelude::{
        CrudError, DeleteById, EntityValidator, RequestContext, ValidationTrigger, delete_by_id,
    };
    use tempfile::TempDir;

    use crate::backend::{
        crudkit_resources, projects::CreateProject, projects::transport::crud::CrudProjectResource,
    };

    async fn test_store(temp: &TempDir) -> Store {
        Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap()
    }

    fn project_input(name: &str, path: &Path, memory: Option<&str>) -> CreateProject {
        CreateProject {
            name: name.to_owned(),
            display_name: None,
            path: path.to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: memory.map(ToOwned::to_owned),
        }
    }

    fn deletion_service(temp: &TempDir, store: &Store) -> (ProjectDeletionService, PathBuf) {
        let codex_projects_dir = temp.path().join("codex-projects");
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        (
            ProjectDeletionService {
                runs: crate::backend::runs::tests::service(
                    store,
                    crate::backend::events::UiEventBus::new(),
                ),
                run_admission: crate::backend::runs::admission::tests::service(store),
                events: crate::backend::events::UiEventBus::new(),
                jobs: crate::backend::knowledge::jobs::tests::service(store),
                automation_supervisor: crate::backend::automation::supervisor::tests::service(
                    store,
                    sessions.clone(),
                ),
                sessions: sessions.clone(),
                repository: Arc::new(ProjectRepository::new(store.db())),
                transactions: Arc::new(TransactionManager::new(store)),
                runtime: Arc::new(ProjectDeletionRuntime {
                    runs: Arc::new(crate::backend::runs::cleanup::RunCleanupRuntime::new(
                        temp.path().join("runs"),
                        std::sync::Arc::new(
                            crate::backend::execution::workspaces::runs::RunWorkspaceRuntime,
                        ),
                    )),
                    codex_projects_dir: codex_projects_dir.clone(),
                    knowledge_projects_dir: temp.path().join("knowledge-artifacts/projects"),
                }),
            },
            codex_projects_dir,
        )
    }

    struct RejectProjectDeletion;

    impl EntityValidator<CrudProjectResource> for RejectProjectDeletion {
        fn name(&self) -> Cow<'static, str> {
            Cow::Borrowed("reject_project_deletion")
        }

        fn version(&self) -> u32 {
            1
        }

        fn validate_model(
            &self,
            _model: &crate::backend::entities::project::ProjectModel,
            _trigger: ValidationTrigger,
        ) -> Violations {
            let mut violations = Violations::empty();
            violations.push(Violation::critical("project deletion rejected"));
            violations
        }
    }

    #[tokio::test]
    async fn crudkit_deletion_runs_project_lifecycle_exactly_once() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let store = test_store(&temp).await;
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(project_input("demo", &workspace, None))
            .await
            .unwrap();
        let project = crate::backend::projects::repository::ProjectRepository::new(store.db())
            .by_name("demo")
            .await
            .unwrap();
        let (deletion, codex_projects_dir) = deletion_service(&temp, &store);
        let project_codex_home = codex_projects_dir.join(project.id.to_string());
        fs::create_dir_all(&project_codex_home).unwrap();
        fs::write(project_codex_home.join("config.toml"), "managed").unwrap();
        let app = crate::backend::application::Application::from_store(
            store.clone(),
            "http://127.0.0.1:4000".into(),
        );
        let context = crudkit_resources::build_contexts(
            store.clone(),
            deletion,
            crate::backend::projects::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::comments::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::items::creation::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            app.state.states.clone(),
            app.state.lanes.clone(),
            app.state.label_catalog.clone(),
            app.state.tools.clone(),
            crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            app.state.personalities.clone(),
            app.state.rules.clone(),
            app.contexts.agent_run.repository.clone(),
        )
        .project;

        let deleted = delete_by_id::<CrudProjectResource>(
            RequestContext::unauthenticated(),
            context,
            DeleteById {
                id: crate::backend::entities::project::ProjectId { id: project.id }
                    .to_serializable_id(),
            },
        )
        .await
        .unwrap();

        assert_that!(&(deleted.entities_affected)).is_equal_to(1);
        assert_that!(
            &(crate::backend::projects::repository::ProjectRepository::new(store.db())
                .by_name("demo")
                .await
                .is_err())
        )
        .is_true();
        assert_that!(&(!project_codex_home.exists())).is_true();
    }

    #[tokio::test]
    async fn crudkit_validation_rejects_before_project_cleanup() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let store = test_store(&temp).await;
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(project_input("demo", &workspace, None))
            .await
            .unwrap();
        let project = crate::backend::projects::repository::ProjectRepository::new(store.db())
            .by_name("demo")
            .await
            .unwrap();
        let (deletion, codex_projects_dir) = deletion_service(&temp, &store);
        let project_codex_home = codex_projects_dir.join(project.id.to_string());
        fs::create_dir_all(&project_codex_home).unwrap();
        fs::write(project_codex_home.join("config.toml"), "managed").unwrap();
        let app = crate::backend::application::Application::from_store(
            store.clone(),
            "http://127.0.0.1:4000".into(),
        );
        let context = crudkit_resources::build_contexts(
            store.clone(),
            deletion,
            crate::backend::projects::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::comments::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            crate::backend::items::creation::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            app.state.states.clone(),
            app.state.lanes.clone(),
            app.state.label_catalog.clone(),
            app.state.tools.clone(),
            crate::backend::items::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            app.state.personalities.clone(),
            app.state.rules.clone(),
            app.contexts.agent_run.repository.clone(),
        )
        .project;
        let mut context = match Arc::try_unwrap(context) {
            Ok(context) => context,
            Err(_) => panic!("project CrudKit context should have a single owner"),
        };
        context.validators.push(Arc::new(RejectProjectDeletion));

        let error = delete_by_id::<CrudProjectResource>(
            RequestContext::unauthenticated(),
            Arc::new(context),
            DeleteById {
                id: crate::backend::entities::project::ProjectId { id: project.id }
                    .to_serializable_id(),
            },
        )
        .await
        .unwrap_err();

        match error {
            CrudError::CriticalValidationErrors { .. } => {}
            other => panic!("expected critical validation error, got {other:?}"),
        }
        assert_that!(
            &(crate::backend::projects::repository::ProjectRepository::new(store.db())
                .by_name("demo")
                .await
                .is_ok())
        )
        .is_true();
        assert_that!(&(project_codex_home.exists())).is_true();
    }

    #[tokio::test]
    async fn deletion_removes_managed_state_and_recreated_key_is_fresh() {
        let temp = TempDir::new().unwrap();
        let old_workspace = temp.path().join("old-workspace");
        let new_workspace = temp.path().join("new-workspace");
        fs::create_dir_all(&old_workspace).unwrap();
        fs::create_dir_all(&new_workspace).unwrap();
        let store = test_store(&temp).await;
        let old = crate::backend::projects::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(project_input(
            "demo",
            &old_workspace,
            Some("old project memory"),
        ))
        .await
        .unwrap();
        let old_model = crate::backend::projects::repository::ProjectRepository::new(store.db())
            .scope_by_name("demo")
            .await
            .unwrap();

        let run_artifact_dir = temp.path().join("runs");
        let codex_projects_dir = temp.path().join("codex-projects");
        let old_codex_home = codex_projects_dir.join(old.id.to_string());
        fs::create_dir_all(&old_codex_home).unwrap();
        fs::write(old_codex_home.join("config.toml"), "old").unwrap();
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let automation_supervisor =
            crate::backend::automation::supervisor::tests::service(&store, sessions.clone());
        automation_supervisor
            .start_project("demo".to_owned())
            .await
            .unwrap();
        let mut cancellation =
            sessions.begin(crate::backend::execution::sessions::ProcessSessionStart {
                run_id: 99,
                project_id: old.id,
                project_name: old.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: old_workspace.to_string_lossy().into_owned(),
            });
        let deletion = ProjectDeletionService {
            runs: crate::backend::runs::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            run_admission: crate::backend::runs::admission::tests::service(&store),
            events: crate::backend::events::UiEventBus::new(),
            jobs: crate::backend::knowledge::jobs::tests::service(&store),
            automation_supervisor: automation_supervisor.clone(),
            sessions: sessions.clone(),
            repository: Arc::new(ProjectRepository::new(store.db())),
            transactions: Arc::new(TransactionManager::new(&store)),
            runtime: Arc::new(ProjectDeletionRuntime {
                runs: Arc::new(crate::backend::runs::cleanup::RunCleanupRuntime::new(
                    run_artifact_dir,
                    std::sync::Arc::new(
                        crate::backend::execution::workspaces::runs::RunWorkspaceRuntime,
                    ),
                )),
                codex_projects_dir,
                knowledge_projects_dir: temp.path().join("knowledge-artifacts/projects"),
            }),
        };

        let deletion_task = {
            let deletion = deletion.clone();
            tokio::spawn(async move { deletion.delete_by_name("demo").await })
        };
        cancellation.wait_for_cancellation().await;
        assert_that!(&(cancellation.cancellation_requested())).is_true();
        assert_that!(&(!deletion_task.is_finished())).is_true();
        let duplicate_error = deletion.delete_by_name("demo").await.unwrap_err();
        let expected_error = format!(
            "project 'demo' (id {}) deletion is already in progress",
            old.id
        );
        assert_that!(&(duplicate_error.to_string())).contains(expected_error.as_str());

        let during_deletion =
            sessions.begin(crate::backend::execution::sessions::ProcessSessionStart {
                run_id: 100,
                project_id: old.id,
                project_name: old.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: old_workspace.to_string_lossy().into_owned(),
            });
        assert_that!(&(during_deletion.cancellation_requested())).is_true();
        assert_that!(&(!during_deletion.is_registered())).is_true();
        assert_that!(&(sessions.get_for_project(old.id, 100).is_none())).is_true();

        sessions.finish(99);
        deletion_task.await.unwrap().unwrap();
        assert_that!(&(!old_codex_home.exists())).is_true();
        assert_that!(&(!automation_supervisor.is_project_running(old.id).await)).is_true();

        let stale_deletion_error = deletion.delete_project(old_model).await.unwrap_err();
        let expected_error = format!("project 'demo' (id {}) was already deleted", old.id);
        assert_that!(&(stale_deletion_error.to_string())).contains(expected_error.as_str());

        let recreated = crate::backend::projects::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(project_input("demo", &new_workspace, None))
        .await
        .unwrap();
        let delayed_old_session =
            sessions.begin(crate::backend::execution::sessions::ProcessSessionStart {
                run_id: 101,
                project_id: old.id,
                project_name: old.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: old_workspace.to_string_lossy().into_owned(),
            });
        let replacement_session =
            sessions.begin(crate::backend::execution::sessions::ProcessSessionStart {
                run_id: 102,
                project_id: recreated.id,
                project_name: recreated.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: new_workspace.to_string_lossy().into_owned(),
            });
        let loaded = crate::backend::projects::repository::ProjectRepository::new(store.db())
            .by_name("demo")
            .await
            .unwrap();
        assert_that!(&(recreated.id)).is_not_equal_to(old.id);
        assert_that!(&(delayed_old_session.cancellation_requested())).is_true();
        assert_that!(&(!delayed_old_session.is_registered())).is_true();
        assert_that!(&(sessions.get_for_project(old.id, 101).is_none())).is_true();
        assert_that!(&(!replacement_session.cancellation_requested())).is_true();
        assert_that!(&(replacement_session.is_registered())).is_true();
        assert_that!(&(!automation_supervisor.is_project_running(recreated.id).await)).is_true();
        assert_that!(&(loaded.path.as_deref())).is_equal_to(new_workspace.to_str());
    }

    #[tokio::test]
    async fn aborted_deletion_releases_single_flight_admission() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let store = test_store(&temp).await;
        let project = crate::backend::projects::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(project_input("demo", &workspace, None))
        .await
        .unwrap();
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let mut cancellation =
            sessions.begin(crate::backend::execution::sessions::ProcessSessionStart {
                run_id: 99,
                project_id: project.id,
                project_name: project.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: workspace.to_string_lossy().into_owned(),
            });
        let deletion = ProjectDeletionService {
            runs: crate::backend::runs::tests::service(
                &store,
                crate::backend::events::UiEventBus::new(),
            ),
            run_admission: crate::backend::runs::admission::tests::service(&store),
            events: crate::backend::events::UiEventBus::new(),
            jobs: crate::backend::knowledge::jobs::tests::service(&store),
            automation_supervisor: crate::backend::automation::supervisor::tests::service(
                &store,
                sessions.clone(),
            ),
            sessions: sessions.clone(),
            repository: Arc::new(ProjectRepository::new(store.db())),
            transactions: Arc::new(TransactionManager::new(&store)),
            runtime: Arc::new(ProjectDeletionRuntime {
                runs: Arc::new(crate::backend::runs::cleanup::RunCleanupRuntime::new(
                    temp.path().join("runs"),
                    std::sync::Arc::new(
                        crate::backend::execution::workspaces::runs::RunWorkspaceRuntime,
                    ),
                )),
                codex_projects_dir: temp.path().join("codex-projects"),
                knowledge_projects_dir: temp.path().join("knowledge-artifacts/projects"),
            }),
        };
        let deletion_task = {
            let deletion = deletion.clone();
            tokio::spawn(async move { deletion.delete_by_name("demo").await })
        };
        cancellation.wait_for_cancellation().await;

        deletion_task.abort();
        assert_that!(&(deletion_task.await.unwrap_err().is_cancelled())).is_true();
        sessions.finish(99);

        deletion.delete_by_name("demo").await.unwrap();
        assert_that!(
            &(crate::backend::projects::repository::ProjectRepository::new(store.db())
                .by_name("demo")
                .await
                .is_err())
        )
        .is_true();
    }

    #[tokio::test]
    async fn deletion_uses_lifecycle_scope_even_when_stored_settings_are_invalid() {
        use sea_orm::{ConnectionTrait, DbBackend, Statement};
        let temp = TempDir::new().unwrap();
        let store = test_store(&temp).await;
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(project_input("demo", temp.path(), None))
            .await
            .unwrap();
        store
            .db()
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                "UPDATE projects SET workspace_mode='invalid-settings' WHERE name='demo'"
                    .to_owned(),
            ))
            .await
            .unwrap();
        let (deletion, _) = deletion_service(&temp, &store);
        deletion.delete_by_name("demo").await.unwrap();
        assert_that!(&deletion.repository.find_id("demo").await.unwrap()).is_none();
    }

    #[tokio::test]
    async fn filesystem_cleanup_failure_preserves_project_and_releases_deletion_admission() {
        let temp = TempDir::new().unwrap();
        let store = test_store(&temp).await;
        let project = crate::backend::projects::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
        .create(project_input("demo", temp.path(), None))
        .await
        .unwrap();
        let (deletion, codex_projects_dir) = deletion_service(&temp, &store);
        fs::write(&codex_projects_dir, "directory replaced by a file").unwrap();
        let mut notifications = deletion.events.subscribe();
        assert_that!(&deletion.delete_by_name("demo").await.is_err()).is_true();
        assert_that!(&deletion.repository.find_id("demo").await.unwrap())
            .is_equal_to(Some(project.id));
        while let Ok(event) = notifications.try_recv() {
            assert_that!(&matches!(
                event,
                dispatch_types::UiEvent::ProjectDeleted { .. }
            ))
            .is_false();
        }
        fs::remove_file(codex_projects_dir).unwrap();
        deletion.delete_by_name("demo").await.unwrap();
        assert_that!(&deletion.repository.find_id("demo").await.unwrap()).is_none();
    }
}
