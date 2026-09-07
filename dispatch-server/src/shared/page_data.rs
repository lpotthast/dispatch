//! Typed page query results shared by SSR, hydration, and backend query services.

use super::view_models::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TriggersPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub settings: Option<ProjectSettingsView>,
    pub personalities: Vec<PersonalityView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CodexStatusPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
}

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

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricsPageData {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
    pub metrics: BackendMetricsSnapshot,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub system_prompt_events: Vec<ProjectSystemPromptEventView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkspaceBarData {
    pub project: Option<ProjectView>,
    pub workspace_editors: Vec<WorkspaceEditorView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ItemPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub project: String,
    pub item: WorkItemView,
    pub comments: Vec<CommentView>,
    pub relationships: Vec<WorkItemRelationshipListEntry>,
    pub label_suggestions: Vec<ProjectLabelView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub automation_runs: Vec<AgentRunView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApiDocsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunSummaryView {
    pub run: AgentRunView,
    pub active: bool,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunsSection {
    pub automation_running: bool,
    pub running_runs: i64,
    pub running_mutating_runs: i64,
    pub running_read_only_runs: i64,
    pub runs: Vec<RunSummaryView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunLogPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub project: String,
    pub run_log: RunLogView,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct AutomationPersonalityInspectorView {
    pub personality: PersonalityView,
    pub revisions: Vec<PersonalityRevisionView>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AutomationRuleInspectorView {
    pub trigger: AutomationTriggerView,
    pub revisions: Vec<AutomationRevisionView>,
    pub evaluations: Vec<AutomationEvaluationView>,
    pub current_revision_analytics: Option<RevisionAnalyticsView>,
}
