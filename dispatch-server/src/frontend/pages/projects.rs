use crate::{
    frontend::{crudkit::ProjectsPanel, services::project_service},
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
    let api_base_url_for_projects = project_service().crudkit_api_base_url().to_owned();
    view! {
        <Title text="Projects"/>
        <div>
            <main class="page-shell projects-page">
                <section class="page-heading">
                    <h1>"Projects"</h1>
                </section>
                <ProjectsPanel api_base_url=api_base_url_for_projects/>
            </main>
        </div>
    }
}
