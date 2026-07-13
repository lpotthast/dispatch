use crate::{
    frontend::{
        components::{ActivePage, LiveRunsSection, cached_query, selected_project_signal, top_bar},
        live_events::{refetch_on_live_event, runs_page_event_matches},
        services::{project_cache, run_service},
    },
    shared::view_models::{
        AgentRunOutputPiece, AgentRunView, AutomationStatusView, CodexAppServerStatusView,
        ProjectView, WorkspaceEditorView,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub automation_status: Option<AutomationStatusView>,
    pub automation_running: bool,
    pub run_sessions: Vec<BoardRunSessionView>,
    pub workspace_editors: Vec<WorkspaceEditorView>,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunsSection {
    pub automation_status: AutomationStatusView,
    pub automation_running: bool,
    pub run_sessions: Vec<BoardRunSessionView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardRunSessionView {
    pub run: AgentRunView,
    pub developer_instructions: Option<String>,
    pub user_prompt: Option<String>,
    pub output: Vec<AgentRunOutputPiece>,
    pub active: bool,
}

#[component]
pub fn PageRuns() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = run_service();
    let initial = service.cached_page_untracked(&selected_project.get_untracked());
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || selected_project.get(),
        move |selected_project| service_for_cache.cached_page(selected_project),
        move |selected_project| {
            let service = service_for_load.clone();
            let selected_project = selected_project.clone();
            async move { service.load_page(selected_project).await }
        },
    );
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, runs_page_event_matches);
    let active_project_names = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.active_project_names)
            .unwrap_or_default()
    });
    let codex_status = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.codex_status)
            .unwrap_or_default()
    });
    let topbar = top_bar(
        active_project_names,
        selected_project.into(),
        ActivePage::Runs,
        Signal::derive(|| None),
        codex_status,
    );

    view! {
        <Title text="Runs"/>
        <div>
            {topbar}
            <main class="page-shell runs-page">
                <section class="page-heading">
                    <h1>"Runs"</h1>
                </section>
                {move || {
                    result
                        .value
                        .get()
                        .map(runs_content)
                        .unwrap_or_else(runs_shell)
                }}
            </main>
        </div>
    }
}

fn runs_shell() -> AnyView {
    view! {
        <section class="automation">
            <div class="panel-heading">
                <h2>"Runs"</h2>
                <p class="muted">"0 running (0 mutating, 0 read-only), controller stopped"</p>
            </div>
            <div class="run-session-shell">
                <div class="run-session-list"><p class="muted">"No runs yet."</p></div>
                <aside class="run-session-detail"><p class="muted">"No run selected."</p></aside>
            </div>
        </section>
    }
    .into_any()
}

fn runs_content(page: RunsPage) -> AnyView {
    let RunsPage {
        projects: _,
        active_project_names: _,
        selected_project,
        automation_status,
        automation_running,
        run_sessions,
        workspace_editors,
        codex_status: _,
    } = page;

    if let (Some(project), Some(automation_status)) = (selected_project, automation_status) {
        view! {
            <LiveRunsSection
                project=project
                initial_status=automation_status
                initial_running=automation_running
                initial_run_sessions=run_sessions
                workspace_editors=workspace_editors
            />
        }
        .into_any()
    } else {
        view! {
            <section class="empty-state">
                <h2>"Choose a project"</h2>
                <a class="button-link" href="/projects">"Projects"</a>
            </section>
        }
        .into_any()
    }
}
