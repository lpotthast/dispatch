use crate::{
    frontend::{
        components::{ActivePage, TopBar, cached_query, selected_project_signal},
        crudkit::ProjectsPanel,
        live_events::{projects_page_event_matches, refetch_on_live_event},
        services::{project_cache, project_service},
    },
    shared::view_models::{CodexAppServerStatusView, ProjectView},
};
use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageProjects() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = project_service();
    let api_base_url_for_projects = service.crudkit_api_base_url().to_owned();
    let initial = service.cached_page_untracked();
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        || (),
        move |()| service_for_cache.cached_page(),
        move |()| {
            let service = service_for_load.clone();
            async move { service.load_page().await }
        },
    );
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, projects_page_event_matches);
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
    let topbar = view! {
        <TopBar
            active_project_names
            selected_project=selected_project.into()
            active=ActivePage::Projects
            automation=Signal::derive(|| None)
            codex_status
        />
    };

    view! {
        <Title text="Projects"/>
        <div>
            {topbar}
            <main class="page-shell projects-page">
                <section class="page-heading">
                    <h1>"Projects"</h1>
                </section>
                <ProjectsPanel api_base_url=api_base_url_for_projects/>
            </main>
        </div>
    }
}
