use crate::*;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardItemView {
    pub item: BoardWorkItemView,
    pub run_count: usize,
    pub recent_runs: Vec<BoardRunPreview>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardItemsSection {
    pub items: Vec<BoardItemView>,
    pub swim_lanes: Vec<SwimLaneView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub label_accent_colors: BTreeMap<String, String>,
    pub misconfigured_item_count: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub automation_status: Option<AutomationStatusView>,
    pub automation_running: bool,
    pub items: Vec<BoardItemView>,
    pub swim_lanes: Vec<SwimLaneView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub label_suggestions: Vec<ProjectLabelView>,
    pub label_accent_colors: BTreeMap<String, String>,
    pub misconfigured_item_count: i64,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardRunPreview {
    pub id: i64,
    pub status: AgentRunStatus,
    pub result_summary: String,
    pub created_at: String,
}
