use crate::{
    frontend::{
        components::{ActivePage, cached_query, selected_project_signal, top_bar},
        live_events::{api_docs_event_matches, refetch_on_live_event},
        services::{api_docs_service, project_cache},
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
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, api_docs_event_matches);
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
        ActivePage::Api,
        Signal::derive(|| None),
        codex_status,
    );

    view! {
        <Title text="Dispatch API"/>
        <div>
            {topbar}
            <main class="page-shell api-docs">
                <section class="page-heading">
                    <h1>"Dispatch API"</h1>
                </section>
                {dispatch_labels_panel()}
                {custom_endpoints_panel()}
            </main>
        </div>
    }
}

fn dispatch_labels_panel() -> impl IntoView {
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

fn custom_endpoints_panel() -> impl IntoView {
    let custom_endpoints = [
        "GET /api/projects/{project}/memory",
        "PUT /api/projects/{project}/memory",
        "POST /api/projects/{project}/memory/append",
        "GET /api/projects/{project}/memory/events",
        "POST /api/projects/{project}/memory/events/compact",
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
