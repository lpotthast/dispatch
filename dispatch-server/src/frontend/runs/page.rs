use crate::frontend::{components::selected_project_signal, runs::panels::LiveRunsSection};
use leptos::prelude::*;
use leptos_meta::Title;

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
