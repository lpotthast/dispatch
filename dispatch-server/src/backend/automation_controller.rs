use std::{
    collections::HashMap,
    sync::{Arc, Mutex, MutexGuard},
};

use rootcause::{Result, prelude::*};
use tokio::sync::watch;

use crate::backend::{
    events,
    process_sessions::{ProcessSessionRegistry, ProjectDeletionAdmissionRejection},
    projects,
    storage::Store,
};

#[derive(Clone, Debug, Default)]
pub struct AutomationController {
    projects: Arc<Mutex<HashMap<i64, ProjectAutomation>>>,
}

#[derive(Debug)]
struct ProjectAutomation {
    project_name: String,
    shutdown: watch::Sender<bool>,
}

impl AutomationController {
    pub fn new() -> Self {
        Self::default()
    }

    fn lock_projects(&self) -> MutexGuard<'_, HashMap<i64, ProjectAutomation>> {
        match self.projects.lock() {
            Ok(projects) => projects,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    pub async fn start_project(
        &self,
        store: &Store,
        project_name: String,
        sessions: &ProcessSessionRegistry,
    ) -> Result<()> {
        let project = projects::get_project(store, &project_name).await?;
        self.start_resolved_project(project.id, project_name, sessions)
    }

    fn start_resolved_project(
        &self,
        project_id: i64,
        project_name: String,
        sessions: &ProcessSessionRegistry,
    ) -> Result<()> {
        let inserted = match sessions.with_project_start_admitted(project_id, || {
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
            Err(ProjectDeletionAdmissionRejection::InProgress) => {
                bail!(
                    "project '{}' (id {}) is being deleted; automation cannot start",
                    project_name,
                    project_id
                );
            }
            Err(ProjectDeletionAdmissionRejection::AlreadyDeleted) => {
                bail!(
                    "project '{}' (id {}) was deleted; automation cannot start",
                    project_name,
                    project_id
                );
            }
        };
        if inserted {
            events::publish_automation_changed(&project_name);
        }
        Ok(())
    }

    pub async fn stop_project(
        &self,
        project_id: i64,
        project_name: &str,
        sessions: &ProcessSessionRegistry,
    ) -> Result<()> {
        let automation = self.lock_projects().remove(&project_id);
        sessions.cancel_project(project_id);
        if let Some(automation) = automation {
            let _ = automation.shutdown.send(true);
        }
        events::publish_automation_changed(project_name);
        Ok(())
    }

    pub async fn shutdown_all(&self, sessions: &ProcessSessionRegistry) {
        let projects = std::mem::take(&mut *self.lock_projects());
        let project_names = projects
            .values()
            .map(|automation| automation.project_name.clone())
            .collect::<Vec<_>>();
        for automation in projects.values() {
            let _ = automation.shutdown.send(true);
        }
        sessions.cancel_all();
        for project_name in project_names {
            events::publish_automation_changed(&project_name);
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
mod tests {
    use assertr::prelude::*;
    use tempfile::TempDir;

    use super::*;
    use crate::backend::projects::{CreateProject, create_project};

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
        (temp, store)
    }

    #[tokio::test]
    async fn controller_tracks_active_projects_and_cancellation_senders() {
        let (_temp, store) = test_store().await;
        let controller = AutomationController::new();
        let sessions = ProcessSessionRegistry::new();

        controller
            .start_project(&store, "demo".to_owned(), &sessions)
            .await
            .unwrap();

        assert_that!(&(controller.is_project_running(1).await)).is_true();
        assert_that!(&(controller.active_project_names().await)).is_equal_to(vec!["demo"]);

        let cancellations = controller.project_cancellations().await;
        let cancellation = cancellations
            .get(&1)
            .expect("active project should expose cancellation");
        assert_that!(&(!*cancellation.borrow())).is_true();

        controller.stop_project(1, "demo", &sessions).await.unwrap();

        assert_that!(&(!controller.is_project_running(1).await)).is_true();
        assert_that!(&(controller.active_project_names().await)).is_equal_to(Vec::<String>::new());
        assert_that!(&(*cancellation.borrow())).is_true();
    }

    #[tokio::test]
    async fn delayed_controller_start_cannot_reopen_deleted_project_admission() {
        use sea_orm::EntityTrait;

        use crate::backend::entities::project::Project;

        let (temp, store) = test_store().await;
        let old_project = projects::get_project(&store, "demo").await.unwrap();
        let controller = AutomationController::new();
        let sessions = ProcessSessionRegistry::new();
        let deletion = sessions.begin_project_deletion(old_project.id).unwrap();

        controller
            .stop_project(old_project.id, "demo", &sessions)
            .await
            .unwrap();
        let in_progress_error = controller
            .start_resolved_project(old_project.id, "demo".to_owned(), &sessions)
            .unwrap_err();
        assert_that!(&(in_progress_error.to_string())).contains("is being deleted");

        Project::delete_by_id(old_project.id)
            .exec(store.db().as_ref())
            .await
            .unwrap();
        deletion.mark_deleted();
        let stale_error = controller
            .start_resolved_project(old_project.id, "demo".to_owned(), &sessions)
            .unwrap_err();
        assert_that!(&(stale_error.to_string())).contains("was deleted");

        let replacement = create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
        controller
            .start_project(&store, "demo".to_owned(), &sessions)
            .await
            .unwrap();

        assert_that!(&(replacement.id)).is_not_equal_to(old_project.id);
        assert_that!(&(!controller.is_project_running(old_project.id).await)).is_true();
        assert_that!(&(controller.is_project_running(replacement.id).await)).is_true();
        assert_that!(
            &(controller
                .project_cancellations()
                .await
                .contains_key(&replacement.id))
        )
        .is_true();
    }
}
