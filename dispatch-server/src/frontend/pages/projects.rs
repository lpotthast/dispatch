use crate::frontend::{crudkit::ProjectsPanel, services::project_service};
use leptos::prelude::*;
use leptos_meta::Title;

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
