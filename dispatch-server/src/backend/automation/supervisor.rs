use crate::backend::projects::repository::ProjectRepository;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use rootcause::{Result, prelude::*};
use tokio::sync::watch;

use crate::backend::execution::sessions::{DeletionAdmissionRejection, ProcessSessionRegistry};

#[derive(Clone)]
pub(crate) struct AutomationSupervisor {
    repository: Arc<ProjectRepository>,
    runs: Arc<crate::backend::runs::service::RunService>,
    sessions: ProcessSessionRegistry,
    events: crate::backend::events::UiEventBus,
    projects: Arc<Mutex<HashMap<i64, ProjectAutomation>>>,
}

#[derive(Debug)]
struct ProjectAutomation {
    project_name: String,
    shutdown: watch::Sender<bool>,
}

impl AutomationSupervisor {
    pub(crate) fn new(
        repository: Arc<ProjectRepository>,
        runs: Arc<crate::backend::runs::service::RunService>,
        sessions: ProcessSessionRegistry,
        events: crate::backend::events::UiEventBus,
    ) -> Self {
        Self {
            repository,
            runs,
            sessions,
            events,
            projects: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    fn lock_projects(&self) -> MutexGuard<'_, HashMap<i64, ProjectAutomation>> {
        match self.projects.lock() {
            Ok(projects) => projects,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub(crate) async fn start_project(&self, project_name: String) -> Result<()> {
        let project = self.repository.by_name(&project_name).await?;
        self.start_resolved_project(project.id, project_name)
    }
    fn start_resolved_project(&self, project_id: i64, project_name: String) -> Result<()> {
        let inserted = match self.sessions.with_project_start_admitted(project_id, || {
            let mut projects = self.lock_projects();
            if projects.contains_key(&project_id) {
                return false;
            }

            let (shutdown, _) = watch::channel(false);
            projects.insert(
                project_id,
                ProjectAutomation {
                    project_name: project_name.clone(),
                    shutdown,
                },
            );
            true
        }) {
            Ok(inserted) => inserted,
            Err(DeletionAdmissionRejection::InProgress) => {
                bail!(
                    "project '{}' (id {}) is being deleted; automation cannot start",
                    project_name,
                    project_id
                );
            }
            Err(DeletionAdmissionRejection::AlreadyDeleted) => {
                bail!(
                    "project '{}' (id {}) was deleted; automation cannot start",
                    project_name,
                    project_id
                );
            }
        };
        if inserted {
            self.events.publish_automation_changed(&project_name);
        }
        Ok(())
    }

    pub(crate) async fn stop_project(&self, project_name: &str) -> Result<()> {
        let project = self.repository.by_name(project_name).await?;
        self.cancel_project(project.id);
        self.runs.cancel_project(project.id).await?;
        self.events.publish_automation_changed(project_name);
        Ok(())
    }
    fn cancel_project(&self, project_id: i64) {
        let automation = self.lock_projects().remove(&project_id);
        self.sessions.cancel_project(project_id);
        if let Some(automation) = automation {
            let _ = automation.shutdown.send(true);
        }
    }
    pub(crate) fn close_project(&self, project_id: i64, project_name: &str) {
        self.cancel_project(project_id);
        self.events.publish_automation_changed(project_name);
    }

    pub async fn shutdown_all(&self) {
        let projects = std::mem::take(&mut *self.lock_projects());
        let project_names = projects
            .values()
            .map(|automation| automation.project_name.clone())
            .collect::<Vec<_>>();
        for automation in projects.values() {
            let _ = automation.shutdown.send(true);
        }
        self.sessions.cancel_all();
        for project_name in project_names {
            self.events.publish_automation_changed(&project_name);
        }
    }

    pub async fn is_project_running(&self, project_id: i64) -> bool {
        self.lock_projects().contains_key(&project_id)
    }

    pub async fn active_project_names(&self) -> Vec<String> {
        let mut names = self
            .lock_projects()
            .values()
            .map(|automation| automation.project_name.clone())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub async fn project_cancellations(&self) -> HashMap<i64, watch::Receiver<bool>> {
        self.lock_projects()
            .iter()
            .map(|(project_id, automation)| (*project_id, automation.shutdown.subscribe()))
            .collect()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use assertr::prelude::*;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::projects::CreateProject;
    use crate::backend::storage::Store;

    async fn test_store() -> (TempDir, Store) {
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
        (temp, store)
    }

    #[tokio::test]
    async fn supervisor_tracks_active_projects_and_cancellation_senders() {
        let (_temp, store) = test_store().await;
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let supervisor = service(&store, sessions.clone());

        supervisor.start_project("demo".to_owned()).await.unwrap();

        assert_that!(&(supervisor.is_project_running(1).await)).is_true();
        assert_that!(&(supervisor.active_project_names().await)).is_equal_to(vec!["demo"]);

        let cancellations = supervisor.project_cancellations().await;
        let cancellation = cancellations
            .get(&1)
            .expect("active project should expose cancellation");
        assert_that!(&(!*cancellation.borrow())).is_true();

        supervisor.stop_project("demo").await.unwrap();

        assert_that!(&(!supervisor.is_project_running(1).await)).is_true();
        assert_that!(&(supervisor.active_project_names().await)).is_equal_to(Vec::<String>::new());
        assert_that!(&(*cancellation.borrow())).is_true();
    }

    #[tokio::test]
    async fn delayed_supervisor_start_cannot_reopen_deleted_project_admission() {
        use sea_orm::EntityTrait;

        use crate::backend::entities::project::Project;

        let (temp, store) = test_store().await;
        let old_project = ProjectRepository::new(store.db())
            .by_name("demo")
            .await
            .unwrap();
        let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
        let supervisor = service(&store, sessions.clone());
        let deletion = sessions.begin_project_deletion(old_project.id).unwrap();

        supervisor.close_project(old_project.id, "demo");
        let in_progress_error = supervisor
            .start_resolved_project(old_project.id, "demo".to_owned())
            .unwrap_err();
        assert_that!(&(in_progress_error.to_string())).contains("is being deleted");

        Project::delete_by_id(old_project.id)
            .exec(store.db().as_ref())
            .await
            .unwrap();
        deletion.mark_deleted();
        let stale_error = supervisor
            .start_resolved_project(old_project.id, "demo".to_owned())
            .unwrap_err();
        assert_that!(&(stale_error.to_string())).contains("was deleted");

        let replacement = crate::backend::projects::tests::service(
            &store,
            crate::backend::events::UiEventBus::new(),
        )
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
        supervisor.start_project("demo".to_owned()).await.unwrap();

        assert_that!(&(replacement.id)).is_not_equal_to(old_project.id);
        assert_that!(&(!supervisor.is_project_running(old_project.id).await)).is_true();
        assert_that!(&(supervisor.is_project_running(replacement.id).await)).is_true();
        assert_that!(
            &(supervisor
                .project_cancellations()
                .await
                .contains_key(&replacement.id))
        )
        .is_true();
    }
    pub(crate) fn service(store: &Store, sessions: ProcessSessionRegistry) -> AutomationSupervisor {
        let events = crate::backend::events::UiEventBus::new();
        AutomationSupervisor::new(
            Arc::new(ProjectRepository::new(store.db())),
            crate::backend::runs::tests::service(store, events.clone()),
            sessions,
            events,
        )
    }

    #[tokio::test]
    async fn stop_cancels_owned_sessions_and_commits_run_termination_before_notification() {
        use crate::backend::runs::{launch::model::AgentLaunchTargetV1, model::CreateRunConfig};
        use dispatch_types::{
            AgentRunKind, AgentRunPurposeV1, AgentRunStatus, AgentToolName, AutomationRunMutability,
        };
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let project_id = app.state.projects.id("demo").await.unwrap();
        let run = app
            .state
            .runs
            .create(
                project_id,
                CreateRunConfig {
                    tool: AgentToolName::Codex,
                    mutability: AutomationRunMutability::ReadOnly,
                    trigger: None,
                    personality_revision_id: None,
                    effective_timeout_seconds: 60,
                    effective_concurrency_group: None,
                    run_kind: AgentRunKind::Task,
                    purpose: AgentRunPurposeV1::Ordinary,
                    knowledge_job_id: None,
                    launch_target: &AgentLaunchTargetV1::none(),
                },
            )
            .await
            .unwrap();
        app.state
            .automation_supervisor
            .start_project("demo".into())
            .await
            .unwrap();
        let session =
            app.state
                .sessions
                .begin(crate::backend::execution::sessions::ProcessSessionStart {
                    run_id: run.id,
                    project_id,
                    project_name: "demo".into(),
                    tool_name: "codex".into(),
                    command: String::new(),
                    working_dir: String::new(),
                });
        let mut events = app.state.events.subscribe();
        app.state
            .automation_supervisor
            .stop_project("demo")
            .await
            .unwrap();
        assert_that!(&session.cancellation_requested()).is_true();
        assert_that!(
            &app.state
                .automation_supervisor
                .is_project_running(project_id)
                .await
        )
        .is_false();
        assert_that!(
            &app.state
                .run_queries
                .get("demo", run.id)
                .await
                .unwrap()
                .status
        )
        .is_equal_to(AgentRunStatus::Cancelled);
        assert_that!(&events.try_recv().is_ok()).is_true();
    }
}
