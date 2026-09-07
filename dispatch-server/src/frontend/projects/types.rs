use crate::frontend::http::request::ServiceRequest;
use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProjectCatalog {
    pub(crate) projects: Vec<ProjectView>,
    pub(crate) active_project_names: Vec<String>,
}
impl From<ProjectsPage> for ProjectCatalog {
    fn from(value: ProjectsPage) -> Self {
        Self {
            projects: value.projects,
            active_project_names: value.active_project_names,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ProjectSettings {
    pub(crate) selected_project: Option<String>,
    pub(crate) selected_project_view: Option<ProjectView>,
    pub(crate) system_prompt_events: Vec<ProjectSystemPromptEventView>,
}
impl From<ProjectPage> for ProjectSettings {
    fn from(value: ProjectPage) -> Self {
        Self {
            selected_project: value.selected_project,
            selected_project_view: value.selected_project_view,
            system_prompt_events: value.system_prompt_events,
        }
    }
}

pub(super) struct ProjectRequests {
    pub(super) load_page: ServiceRequest<(), ProjectsPage>,
    pub(super) load_project_page: ServiceRequest<Option<String>, ProjectPage>,
    pub(super) load_workspace_bar: ServiceRequest<Option<String>, WorkspaceBarData>,
    #[cfg(not(feature = "ssr"))]
    pub(super) current_project_id: ServiceRequest<String, Option<i64>>,
    pub(super) update_auto_commit: ServiceRequest<(String, bool), ()>,
    pub(super) update_system_prompt: ServiceRequest<(String, String), ()>,
    pub(super) clear_system_prompt_history: ServiceRequest<String, HistoryClearResult>,
    pub(super) update_commit_policy: ServiceRequest<(String, CommitPolicyUpdate), ()>,
    pub(super) open_workspace: ServiceRequest<(String, String), ()>,
    pub(super) cleanup_worktrees: ServiceRequest<String, ()>,
}
