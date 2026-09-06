use crate::{
    frontend::{
        components::{cached_query, selected_project_signal},
        live_events::{api_docs_event_matches, refetch_on_live_event},
        services::api_docs_service,
    },
    shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, CLAIMED_FROM_STATE_LABEL_KEY, CodexAppServerStatusView,
        FEEDBACK_REQUESTED_LABEL_KEY, ProjectView, STATE_LABEL_KEY,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ApiDocsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageApiDocs() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = api_docs_service();
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
    refetch_on_live_event(result.refresh, api_docs_event_matches);

    view! {
        <Title text="Dispatch API"/>
        <div>
            <main class="page-shell api-docs">
                <section class="page-heading">
                    <h1>"Dispatch API"</h1>
                </section>
                <DispatchLabelsPanel/>
                <CustomEndpointsPanel/>
            </main>
        </div>
    }
}

#[component]
fn DispatchLabelsPanel() -> impl IntoView {
    view! {
        <section class="dispatch-labels panel">
            <div class="panel-heading">
                <h2>"Dispatch labels"</h2>
            </div>
            <div class="system-label-grid">
                <article>
                    <code>{STATE_LABEL_KEY}</code>
                    <span>"Swim-lane state."</span>
                </article>
                <article>
                    <code>{CLAIMED_FROM_STATE_LABEL_KEY}</code>
                    <span>"Temporary claim origin."</span>
                </article>
                <article>
                    <code>{AUTOMATION_BLOCKED_LABEL_KEY}</code>
                    <span>"Excluded from automation pickup."</span>
                </article>
                <article>
                    <code>{FEEDBACK_REQUESTED_LABEL_KEY}</code>
                    <span>"Waiting for user feedback."</span>
                </article>
            </div>
        </section>
    }
}

#[component]
fn CustomEndpointsPanel() -> impl IntoView {
    let custom_endpoints = [
        "GET /api/projects",
        "GET /api/events/ws",
        "GET /api/projects/{project}/events",
        "GET /api/projects/{project}/items/{item_id}/events",
        "GET /api/projects/{project}/work-groups",
        "POST /api/projects/{project}/work-groups",
        "POST /api/projects/{project}/work-groups/{group_key}/items",
        "GET /api/projects/{project}/items/{item_id}/relationships",
        "POST /api/projects/{project}/items/{item_id}/relationships",
        "PATCH /api/projects/{project}/relationships/{relationship_id}",
        "DELETE /api/projects/{project}/relationships/{relationship_id}",
        "GET /api/projects/{project}/automation/sessions",
        "GET /api/projects/{project}/knowledge/root",
        "GET /api/projects/{project}/knowledge/node?path=README.md",
        "GET /api/projects/{project}/knowledge/search?text=architecture",
        "GET /api/projects/{project}/knowledge/check",
        "GET /api/projects/{project}/knowledge/documents",
        "GET /api/projects/{project}/knowledge/graph",
        "POST /projects/{project}/automation/start",
        "POST /projects/{project}/automation/stop",
        "POST /projects/{project}/automation/recover-stale-claims",
        "POST /projects/{project}/automation/cleanup-worktrees",
        "POST /projects/{project}/workspace/open",
        "POST /projects/{project}/automation/runs/{run_id}/workspace/open",
        "POST /projects/{project}/automation/runs/{run_id}/cancel",
        "POST /api/projects/{project}/items/{item_id}/request-feedback",
        "POST /system/database/open",
        "GET /projects/{project}/automation/runs/{run_id}/log",
    ]
    .into_iter()
    .map(|endpoint| view! { <li>{endpoint}</li> })
    .collect::<Vec<_>>();

    view! {
        <section class="panel">
            <h2>"Custom endpoints"</h2>
            <ul class="compact-list">{custom_endpoints}</ul>
        </section>
    }
}
