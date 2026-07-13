use crate::{
    frontend::{
        components::{
            ActivePage, cached_query, selected_project_signal, top_bar, trigger_runs_panel,
        },
        crudkit::{
            AutomationTableKind, PersonalitiesPanel, automation_triggers_crudkit_instance,
            selected_trigger_id_from_context,
        },
        services::{automation_service, project_cache},
    },
    shared::view_models::{
        CodexAppServerStatusView, PersonalityView, ProjectView, WorkspaceEditorView,
    },
};
use crudkit_leptos::crud_instance::CrudInstanceContext;
use leptos::prelude::*;
use leptos_meta::Title;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TriggersPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub personalities: Vec<PersonalityView>,
    pub workspace_editors: Vec<WorkspaceEditorView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageTriggers() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = automation_service();
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
        ActivePage::Triggers,
        Signal::derive(|| None),
        codex_status,
    );

    view! {
        <Title text="Automation"/>
        <div>
            {topbar}
            <main class="page-shell triggers-page">
                <section class="page-heading">
                    <h1>"Automation"</h1>
                </section>
                {move || {
                    result
                        .value
                        .get()
                        .map(triggers_content)
                        .unwrap_or_else(triggers_shell)
                }}
            </main>
        </div>
    }
}

fn triggers_shell() -> AnyView {
    view! {
        <>
            <section class="personalities panel">
                <div class="panel-heading"><h2>"Personalities"</h2></div>
            </section>
            <section class="automation-triggers panel">
                <div class="panel-heading"><h2>"Work-consuming automations"</h2></div>
            </section>
            <section class="automation-triggers panel">
                <div class="panel-heading"><h2>"Work-producing automations"</h2></div>
            </section>
        </>
    }
    .into_any()
}

fn triggers_content(page: TriggersPage) -> AnyView {
    let TriggersPage {
        projects: _,
        active_project_names: _,
        selected_project,
        selected_project_view,
        personalities,
        workspace_editors,
        api_base_url,
        codex_status: _,
    } = page;

    if let (Some(project), Some(project_view)) = (selected_project, selected_project_view) {
        let (consumer_context, set_consumer_context) = signal(None::<CrudInstanceContext>);
        let (producer_context, set_producer_context) = signal(None::<CrudInstanceContext>);
        let selected_trigger_id = Memo::new(move |_| {
            consumer_context
                .get()
                .and_then(selected_trigger_id_from_context)
                .or_else(|| {
                    producer_context
                        .get()
                        .and_then(selected_trigger_id_from_context)
                })
        });
        let consuming_triggers = automation_triggers_crudkit_instance(
            api_base_url.clone(),
            project.clone(),
            project_view.id,
            personalities.clone(),
            AutomationTableKind::Consuming,
            Callback::new(move |context| set_consumer_context.set(Some(context))),
        );
        let producing_triggers = automation_triggers_crudkit_instance(
            api_base_url.clone(),
            project.clone(),
            project_view.id,
            Vec::new(),
            AutomationTableKind::Producing,
            Callback::new(move |context| set_producer_context.set(Some(context))),
        );
        let trigger_runs = trigger_runs_panel(
            project.clone(),
            selected_trigger_id,
            workspace_editors.clone(),
        );
        view! {
            <>
                    <PersonalitiesPanel
                        api_base_url=api_base_url
                        project=project.clone()
                        project_id=project_view.id
                    />
                    <section class="automation-triggers panel">
                        <div class="panel-heading">
                            <h2>"Work-consuming automations"</h2>
                        </div>
                        <div class="crudkit-automation-triggers" data-crudkit-leptos="automation-triggers">
                            {consuming_triggers}
                        </div>
                    </section>
                    <section class="automation-triggers panel">
                        <div class="panel-heading">
                            <h2>"Work-producing automations"</h2>
                        </div>
                        <div class="crudkit-automation-triggers" data-crudkit-leptos="automation-triggers">
                            {producing_triggers}
                        </div>
                    </section>
                    {trigger_runs}
            </>
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
