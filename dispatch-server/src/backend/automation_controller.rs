use std::{collections::HashMap, sync::Arc};

use rootcause::Result;
use tokio::sync::{Mutex, watch};

use crate::backend::{events, process_sessions::ProcessSessionRegistry, projects, storage::Store};

#[derive(Clone, Debug, Default)]
pub struct AutomationController {
    projects: Arc<Mutex<HashMap<String, ProjectAutomation>>>,
}

#[derive(Debug)]
struct ProjectAutomation {
    shutdown: watch::Sender<bool>,
}

impl AutomationController {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn start_project(&self, store: &Store, project_name: String) -> Result<()> {
        projects::get_project(store, &project_name).await?;

        let mut projects = self.projects.lock().await;
        if projects.contains_key(&project_name) {
            return Ok(());
        }

        let (shutdown, _) = watch::channel(false);
        projects.insert(project_name.clone(), ProjectAutomation { shutdown });
        events::publish_automation_changed(&project_name);
        Ok(())
    }

    pub async fn stop_project(
        &self,
        project_name: &str,
        sessions: &ProcessSessionRegistry,
    ) -> Result<()> {
        let automation = self.projects.lock().await.remove(project_name);
        sessions.cancel_project(project_name).await;
        if let Some(automation) = automation {
            let _ = automation.shutdown.send(true);
        }
        events::publish_automation_changed(project_name);
        Ok(())
    }

    pub async fn shutdown_all(&self, sessions: &ProcessSessionRegistry) {
        let projects = std::mem::take(&mut *self.projects.lock().await);
        let project_names = projects.keys().cloned().collect::<Vec<_>>();
        for automation in projects.values() {
            let _ = automation.shutdown.send(true);
        }
        sessions.cancel_all().await;
        for project_name in project_names {
            events::publish_automation_changed(&project_name);
        }
    }

    pub async fn is_project_running(&self, project_name: &str) -> bool {
        self.projects.lock().await.contains_key(project_name)
    }

    pub async fn active_project_names(&self) -> Vec<String> {
        let mut names = self
            .projects
            .lock()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    pub async fn project_cancellations(&self) -> HashMap<String, watch::Receiver<bool>> {
        self.projects
            .lock()
            .await
            .iter()
            .map(|(project_name, automation)| {
                (project_name.clone(), automation.shutdown.subscribe())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
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
            .start_project(&store, "demo".to_owned())
            .await
            .unwrap();

        assert!(controller.is_project_running("demo").await);
        assert_eq!(controller.active_project_names().await, vec!["demo"]);

        let cancellations = controller.project_cancellations().await;
        let cancellation = cancellations
            .get("demo")
            .expect("active project should expose cancellation");
        assert!(!*cancellation.borrow());

        controller.stop_project("demo", &sessions).await.unwrap();

        assert!(!controller.is_project_running("demo").await);
        assert_eq!(
            controller.active_project_names().await,
            Vec::<String>::new()
        );
        assert!(*cancellation.borrow());
    }
}
