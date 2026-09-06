use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::backend::{
    automation,
    automation_controller::AutomationController,
    automation_workspace, codex_app_server,
    entities::{
        agent_run::{self, AgentRun, AgentRunModel},
        project::{Project, ProjectModel},
    },
    events,
    process_sessions::{ProcessSessionRegistry, ProjectDeletionAdmissionRejection},
    projects,
    storage::Store,
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
    store: Store,
    automation_controller: AutomationController,
    sessions: ProcessSessionRegistry,
    run_artifact_dir: PathBuf,
    codex_projects_dir: PathBuf,
    knowledge_projects_dir: PathBuf,
}

impl ProjectDeletionService {
    pub(crate) fn new(
        store: Store,
        automation_controller: AutomationController,
        sessions: ProcessSessionRegistry,
    ) -> Self {
        Self {
            store,
            automation_controller,
            sessions,
            run_artifact_dir: automation::automation_log_dir(),
            codex_projects_dir: codex_app_server::codex_home_dir().join("projects"),
            knowledge_projects_dir: crate::backend::storage::dispatch_home_dir()
                .join("knowledge-artifacts/projects"),
        }
    }

    pub(crate) async fn delete_by_name(&self, project_name: &str) -> Result<()> {
        let project = projects::find_project_by_name(&self.store, project_name).await?;
        self.delete_model(project).await
    }

    pub(crate) async fn delete_model(&self, project: ProjectModel) -> Result<()> {
        let project_id = project.id;
        let project_name = project.name.clone();
        let admission = match self.sessions.begin_project_deletion(project_id) {
            Ok(admission) => admission,
            Err(ProjectDeletionAdmissionRejection::InProgress) => {
                bail!(
                    "project '{}' (id {}) deletion is already in progress",
                    project_name,
                    project_id
                );
            }
            Err(ProjectDeletionAdmissionRejection::AlreadyDeleted) => {
                bail!(
                    "project '{}' (id {}) was already deleted",
                    project_name,
                    project_id
                );
            }
        };

        self.delete_after_admission_closed(&project).await?;
        admission.mark_deleted();

        events::publish_project_deleted(project_id, &project_name);
        events::publish_project_list_changed();
        Ok(())
    }

    async fn delete_after_admission_closed(&self, project: &ProjectModel) -> Result<()> {
        let _publication = self.store.lock_runtime_admission().await;
        crate::backend::knowledge::jobs::stop_project(&self.store, project.id).await?;
        self.automation_controller
            .stop_project(project.id, &project.name, &self.sessions)
            .await?;
        self.wait_for_sessions_to_stop(project).await?;
        automation::stop_automation(&self.store, project.id, &project.name).await?;

        let runs = AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project.id))
            .all(self.store.db().as_ref())
            .await
            .context_with(|| format!("failed to load runs for project '{}'", project.name))?;
        self.cleanup_run_workspaces(project, &runs)?;
        self.cleanup_run_artifacts(&runs)?;
        remove_path_if_exists(&self.codex_projects_dir.join(project.id.to_string()))?;
        remove_path_if_exists(&self.knowledge_projects_dir.join(project.id.to_string()))?;
        remove_path_if_exists(&crate::backend::knowledge::jobs::project_artifacts(
            &self.store,
            project.id,
        ))?;
        let deleted = Project::delete_by_id(project.id)
            .exec(self.store.db().as_ref())
            .await
            .context_with(|| format!("failed to delete project '{}'", project.name))?;
        if deleted.rows_affected != 1 {
            bail!(
                "project '{}' changed while it was being deleted; no row was removed",
                project.name
            );
        }
        Ok(())
    }

    async fn wait_for_sessions_to_stop(&self, project: &ProjectModel) -> Result<()> {
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

    fn cleanup_run_workspaces(&self, project: &ProjectModel, runs: &[AgentRunModel]) -> Result<()> {
        for run in runs {
            if run.branch_name.is_none() && run.worktree_path.is_none() {
                continue;
            }
            let repo_path = project.path.as_deref().ok_or_else(|| {
                report!(
                    "cannot clean workspace artifacts for run {} because project '{}' has no workspace path",
                    run.id,
                    project.name
                )
            })?;
            automation_workspace::remove_run_workspace(
                Path::new(repo_path),
                run.branch_name.as_deref(),
                run.worktree_path.as_deref().map(Path::new),
            )
            .context_with(|| format!("failed to clean workspace artifacts for run {}", run.id))?;
        }
        Ok(())
    }

    fn cleanup_run_artifacts(&self, runs: &[AgentRunModel]) -> Result<()> {
        cleanup_run_artifacts(&self.run_artifact_dir, runs.iter().map(|run| run.id))
    }
}

fn cleanup_run_artifacts(
    run_artifact_dir: &Path,
    run_ids: impl IntoIterator<Item = i64>,
) -> Result<()> {
    for run_id in run_ids {
        for suffix in [
            "developer-instructions.md",
            "user-prompt.md",
            "output.json",
            "incremental.jsonl",
            "codex-stderr.log",
            "git-policy.json",
        ] {
            remove_path_if_exists(&run_artifact_dir.join(format!("run-{run_id}.{suffix}")))?;
        }
        remove_path_if_exists(&run_artifact_dir.join(format!("run-{run_id}-bin")))?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            Err(err).context_with(|| format!("failed to inspect '{}'", path.display()))?;
            unreachable!("filesystem inspection error must propagate");
        }
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
            .context_with(|| format!("failed to remove directory '{}'", path.display()))?;
    } else {
        fs::remove_file(path)
            .context_with(|| format!("failed to remove file '{}'", path.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, sync::Arc};

    use super::*;
    use assertr::prelude::*;
    use crudkit_core::{
        id::Id,
        validation::violation::{Violation, Violations},
    };
    use crudkit_rs::prelude::{
        CrudError, DeleteById, EntityValidator, HasId, RequestContext, ValidationTrigger,
        delete_by_id,
    };
    use tempfile::TempDir;

    use crate::backend::{
        crudkit_resources::{self, CrudProjectResource},
        projects::{CreateProject, create_project, find_project_by_name, get_project},
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
        (
            ProjectDeletionService {
                store: store.clone(),
                automation_controller: AutomationController::new(),
                sessions: ProcessSessionRegistry::new(),
                run_artifact_dir: temp.path().join("runs"),
                codex_projects_dir: codex_projects_dir.clone(),
                knowledge_projects_dir: temp.path().join("knowledge-artifacts/projects"),
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

        fn validate_model(&self, _model: &ProjectModel, _trigger: ValidationTrigger) -> Violations {
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
        create_project(&store, project_input("demo", &workspace, None))
            .await
            .unwrap();
        let project = find_project_by_name(&store, "demo").await.unwrap();
        let (deletion, codex_projects_dir) = deletion_service(&temp, &store);
        let project_codex_home = codex_projects_dir.join(project.id.to_string());
        fs::create_dir_all(&project_codex_home).unwrap();
        fs::write(project_codex_home.join("config.toml"), "managed").unwrap();
        let context = crudkit_resources::build_contexts(store.clone(), deletion).project;

        let deleted = delete_by_id::<CrudProjectResource>(
            RequestContext::unauthenticated(),
            context,
            DeleteById {
                id: project.id().to_serializable_id(),
            },
        )
        .await
        .unwrap();

        assert_that!(&(deleted.entities_affected)).is_equal_to(1);
        assert_that!(&(get_project(&store, "demo").await.is_err())).is_true();
        assert_that!(&(!project_codex_home.exists())).is_true();
    }

    #[tokio::test]
    async fn crudkit_validation_rejects_before_project_cleanup() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let store = test_store(&temp).await;
        create_project(&store, project_input("demo", &workspace, None))
            .await
            .unwrap();
        let project = find_project_by_name(&store, "demo").await.unwrap();
        let (deletion, codex_projects_dir) = deletion_service(&temp, &store);
        let project_codex_home = codex_projects_dir.join(project.id.to_string());
        fs::create_dir_all(&project_codex_home).unwrap();
        fs::write(project_codex_home.join("config.toml"), "managed").unwrap();
        let context = crudkit_resources::build_contexts(store.clone(), deletion).project;
        let mut context = match Arc::try_unwrap(context) {
            Ok(context) => context,
            Err(_) => panic!("project CrudKit context should have a single owner"),
        };
        context.validators.push(Arc::new(RejectProjectDeletion));

        let error = delete_by_id::<CrudProjectResource>(
            RequestContext::unauthenticated(),
            Arc::new(context),
            DeleteById {
                id: project.id().to_serializable_id(),
            },
        )
        .await
        .unwrap_err();

        match error {
            CrudError::CriticalValidationErrors { .. } => {}
            other => panic!("expected critical validation error, got {other:?}"),
        }
        assert_that!(&(get_project(&store, "demo").await.is_ok())).is_true();
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
        let old = create_project(
            &store,
            project_input("demo", &old_workspace, Some("old project memory")),
        )
        .await
        .unwrap();
        let old_model = find_project_by_name(&store, "demo").await.unwrap();

        let run_artifact_dir = temp.path().join("runs");
        let codex_projects_dir = temp.path().join("codex-projects");
        let old_codex_home = codex_projects_dir.join(old.id.to_string());
        fs::create_dir_all(&old_codex_home).unwrap();
        fs::write(old_codex_home.join("config.toml"), "old").unwrap();
        let automation_controller = AutomationController::new();
        let sessions = ProcessSessionRegistry::new();
        automation_controller
            .start_project(&store, "demo".to_owned(), &sessions)
            .await
            .unwrap();
        let mut cancellation =
            sessions.begin(crate::backend::process_sessions::ProcessSessionStart {
                run_id: 99,
                project_id: old.id,
                project_name: old.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: old_workspace.to_string_lossy().into_owned(),
            });
        let deletion = ProjectDeletionService {
            store: store.clone(),
            automation_controller: automation_controller.clone(),
            sessions: sessions.clone(),
            run_artifact_dir,
            codex_projects_dir,
            knowledge_projects_dir: temp.path().join("knowledge-artifacts/projects"),
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
            sessions.begin(crate::backend::process_sessions::ProcessSessionStart {
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
        assert_that!(&(!automation_controller.is_project_running(old.id).await)).is_true();

        let stale_deletion_error = deletion.delete_model(old_model).await.unwrap_err();
        let expected_error = format!("project 'demo' (id {}) was already deleted", old.id);
        assert_that!(&(stale_deletion_error.to_string())).contains(expected_error.as_str());

        let recreated = create_project(&store, project_input("demo", &new_workspace, None))
            .await
            .unwrap();
        let delayed_old_session =
            sessions.begin(crate::backend::process_sessions::ProcessSessionStart {
                run_id: 101,
                project_id: old.id,
                project_name: old.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: old_workspace.to_string_lossy().into_owned(),
            });
        let replacement_session =
            sessions.begin(crate::backend::process_sessions::ProcessSessionStart {
                run_id: 102,
                project_id: recreated.id,
                project_name: recreated.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: new_workspace.to_string_lossy().into_owned(),
            });
        let loaded = get_project(&store, "demo").await.unwrap();
        assert_that!(&(recreated.id)).is_not_equal_to(old.id);
        assert_that!(&(delayed_old_session.cancellation_requested())).is_true();
        assert_that!(&(!delayed_old_session.is_registered())).is_true();
        assert_that!(&(sessions.get_for_project(old.id, 101).is_none())).is_true();
        assert_that!(&(!replacement_session.cancellation_requested())).is_true();
        assert_that!(&(replacement_session.is_registered())).is_true();
        assert_that!(&(!automation_controller.is_project_running(recreated.id).await)).is_true();
        assert_that!(&(loaded.path.as_deref())).is_equal_to(new_workspace.to_str());
    }

    #[tokio::test]
    async fn aborted_deletion_releases_single_flight_admission() {
        let temp = TempDir::new().unwrap();
        let workspace = temp.path().join("workspace");
        fs::create_dir_all(&workspace).unwrap();
        let store = test_store(&temp).await;
        let project = create_project(&store, project_input("demo", &workspace, None))
            .await
            .unwrap();
        let sessions = ProcessSessionRegistry::new();
        let mut cancellation =
            sessions.begin(crate::backend::process_sessions::ProcessSessionStart {
                run_id: 99,
                project_id: project.id,
                project_name: project.name.clone(),
                tool_name: "codex".to_owned(),
                command: String::new(),
                working_dir: workspace.to_string_lossy().into_owned(),
            });
        let deletion = ProjectDeletionService {
            store: store.clone(),
            automation_controller: AutomationController::new(),
            sessions: sessions.clone(),
            run_artifact_dir: temp.path().join("runs"),
            codex_projects_dir: temp.path().join("codex-projects"),
            knowledge_projects_dir: temp.path().join("knowledge-artifacts/projects"),
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
        assert_that!(&(get_project(&store, "demo").await.is_err())).is_true();
    }

    #[test]
    fn run_artifact_cleanup_removes_every_dispatch_file_for_run() {
        let temp = TempDir::new().unwrap();
        for suffix in [
            "developer-instructions.md",
            "user-prompt.md",
            "output.json",
            "incremental.jsonl",
            "codex-stderr.log",
            "git-policy.json",
        ] {
            fs::write(temp.path().join(format!("run-42.{suffix}")), suffix).unwrap();
        }
        let shim_dir = temp.path().join("run-42-bin");
        fs::create_dir(&shim_dir).unwrap();
        fs::write(shim_dir.join("git"), "shim").unwrap();
        fs::write(temp.path().join("run-7.output.json"), "keep").unwrap();

        cleanup_run_artifacts(temp.path(), [42]).unwrap();

        assert_that!(&(fs::read_dir(temp.path()).unwrap().count())).is_equal_to(1);
        assert_that!(&(temp.path().join("run-7.output.json").exists())).is_true();
    }
}
