use crate::{
    frontend::{
        components::{ActivePage, cached_query, selected_project_signal, top_bar},
        crudkit::{SwimLanesPanel, WorkItemStatesPanel, agent_tools_panel, projects_panel},
        services::{project_cache, project_service},
    },
    shared::view_models::{CodexAppServerStatusView, ProjectView},
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProjectsPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageProjects() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = project_service();
    let api_base_url_for_panel = service.crudkit_api_base_url().to_owned();
    let api_base_url_for_projects = api_base_url_for_panel.clone();
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
        ActivePage::Projects,
        Signal::derive(|| None),
        codex_status,
    );

    view! {
        <Title text="Projects"/>
        <div>
            {topbar}
            <main class="page-shell projects-page">
                <section class="page-heading">
                    <h1>"Projects"</h1>
                </section>
                {projects_panel(api_base_url_for_projects)}
                {move || result.value.get().map(projects_content)}
            </main>
        </div>
        <div class="page-shell projects-page crudkit-tools-shell">
            {agent_tools_panel(api_base_url_for_panel)}
        </div>
    }
}

fn projects_content(page: ProjectsPage) -> AnyView {
    view! {
        <ProjectsContent
            projects=page.projects
            selected_project=page.selected_project
            api_base_url=page.api_base_url
        />
    }
    .into_any()
}

#[component]
fn ProjectsContent(
    projects: Vec<ProjectView>,
    selected_project: Option<String>,
    api_base_url: String,
) -> impl IntoView + 'static {
    let selected_project_view = selected_project
        .as_ref()
        .and_then(|project| projects.iter().find(|candidate| candidate.name == *project))
        .cloned()
        .or_else(|| projects.first().cloned());
    let query = use_query_map();
    let edit_swim_lane_id = query
        .read_untracked()
        .get("edit_swim_lane")
        .and_then(|value| value.parse().ok());
    let project_authoring = selected_project_view.as_ref().map(|project_view| {
        let project_name = project_view.name.clone();
        let project_id = project_view.id;
        view! {
            <WorkItemStatesPanel
                api_base_url=api_base_url.clone()
                project=project_name.clone()
                project_id=project_id
            />
            <SwimLanesPanel
                api_base_url=api_base_url.clone()
                project=project_name
                project_id=project_id
                edit_lane_id=edit_swim_lane_id
            />
        }
    });

    view! {
        {project_authoring}
    }
}
