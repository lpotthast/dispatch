use crate::{
    frontend::components::{LiveRunsSection, selected_project_signal},
    shared::view_models::AgentRunView,
};
use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunsSection {
    pub automation_running: bool,
    pub running_runs: i64,
    pub running_mutating_runs: i64,
    pub running_read_only_runs: i64,
    pub runs: Vec<RunSummaryView>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunSummaryView {
    pub run: AgentRunView,
    pub active: bool,
}

#[component]
pub fn PageRuns() -> impl IntoView {
    let selected_project = selected_project_signal();
    view! {
        <Title text="Runs"/>
        <div>
            <main class="page-shell runs-page">
                <section class="page-heading">
                    <h1>"Runs"</h1>
                </section>
                <Show
                    when=move || selected_project.get().is_some()
                    fallback=|| view! {
                        <section class="empty-state">
                            <h2>"Choose a project"</h2>
                            <a class="button-link" href="/projects">"Projects"</a>
                        </section>
                    }
                >
                    <For
                        each=move || selected_project.get()
                        key=|project| project.clone()
                        children=move |project| view! { <LiveRunsSection project/> }
                    />
                </Show>
            </main>
        </div>
    }
}
