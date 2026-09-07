use crate::*;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApiDocsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
}
