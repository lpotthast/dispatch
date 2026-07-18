use crate::{
    frontend::{
        components::{ActivePage, TopBar, cached_query, selected_project_signal},
        crudkit::{SwimLanesPanel, WorkItemStatesPanel, WorkItemsPanel},
        services::{project_cache, project_service},
    },
    shared::view_models::{
        CodexAppServerStatusView, ProjectMemoryEventRefView, ProjectMemoryEventView,
        ProjectSystemPromptEventView, ProjectView, WorkspaceEditorView,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ProjectPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub system_prompt_events: Vec<ProjectSystemPromptEventView>,
    pub memory_events: Vec<ProjectMemoryEventView>,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct WorkspaceBarData {
    pub project: Option<ProjectView>,
    pub workspace_editors: Vec<WorkspaceEditorView>,
}

#[component]
pub fn PageProject() -> impl IntoView {
    let selected_project = selected_project_signal();
    let query = use_query_map();
    let service = project_service();
    let initial = service.cached_project_page_untracked(&selected_project.get_untracked());
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || selected_project.get(),
        move |selected_project| service_for_cache.cached_project_page(selected_project),
        move |selected_project| {
            let service = service_for_load.clone();
            let selected_project = selected_project.clone();
            async move { service.load_project_page(selected_project).await }
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
    let topbar = view! {
        <TopBar
            active_project_names
            selected_project=selected_project.into()
            active=ActivePage::Project
            automation=Signal::derive(|| None)
            codex_status
        />
    };

    view! {
        <Title text="Project"/>
        <div>
            {topbar}
            <main class="page-shell project-page">
                <section class="page-heading">
                    <h1>"Project"</h1>
                    <p class="muted">"Settings and maintenance for the selected project."</p>
                </section>
                {move || {
                    let edit_swim_lane_id = query
                        .read()
                        .get("edit_swim_lane")
                        .and_then(|value| value.parse().ok());
                    result
                        .value
                        .get()
                        .map(|page| view! {
                            <ProjectContent
                                page
                                edit_swim_lane_id
                                refresh=result.refresh
                            />
                        })
                }}
            </main>
        </div>
    }
}

#[component]
fn ProjectContent(
    page: ProjectPage,
    edit_swim_lane_id: Option<i64>,
    refresh: Callback<()>,
) -> impl IntoView {
    let ProjectPage {
        projects: _,
        active_project_names: _,
        selected_project,
        selected_project_view,
        system_prompt_events,
        memory_events,
        api_base_url,
        codex_status: _,
    } = page;

    if let (Some(project), Some(project_view)) = (selected_project, selected_project_view) {
        let project_id = project_view.id;
        view! {
            <>
                <ProjectTextSettings
                    project=project.clone()
                    project_view
                    system_prompt_events
                    memory_events
                    refresh
                />
                <WorkItemsPanel
                    api_base_url=api_base_url.clone()
                    project=project.clone()
                    project_id=project_id
                />
                <WorkItemStatesPanel
                    api_base_url=api_base_url.clone()
                    project=project.clone()
                    project_id=project_id
                />
                <SwimLanesPanel
                    api_base_url=api_base_url
                    project=project.clone()
                    project_id=project_id
                    edit_lane_id=edit_swim_lane_id
                />
                <MaintenancePanel project refresh/>
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

#[component]
fn ProjectTextSettings(
    project: String,
    project_view: ProjectView,
    system_prompt_events: Vec<ProjectSystemPromptEventView>,
    memory_events: Vec<ProjectMemoryEventView>,
    refresh: Callback<()>,
) -> impl IntoView + 'static {
    let service = project_service();
    let memory_service = service.clone();
    let project_for_prompt = project.clone();
    let project_for_memory = project;
    let prompt_refresh = refresh;
    let initial_system_prompt = project_view.system_prompt.clone();
    let system_prompt_dirty_baseline = initial_system_prompt.clone();
    let system_prompt_history_for_options = system_prompt_events.clone();
    let system_prompt_history_for_prompt = system_prompt_events;
    let (selected_system_prompt_event_id, set_selected_system_prompt_event_id) =
        signal(None::<i64>);
    let (system_prompt_draft, set_system_prompt_draft) = signal(initial_system_prompt.clone());
    let (system_prompt_pending, set_system_prompt_pending) = signal(false);
    let system_prompt_value = move || {
        selected_system_prompt_event_id
            .get()
            .and_then(|event_id| {
                system_prompt_history_for_prompt
                    .iter()
                    .find(|event| event.id == event_id)
                    .map(|event| event.system_prompt.clone())
                    .or_else(|| {
                        Some(format!(
                            "System prompt event #{event_id} is no longer available."
                        ))
                    })
            })
            .unwrap_or_else(|| system_prompt_draft.get())
    };
    let system_prompt_textarea_class = move || {
        if selected_system_prompt_event_id.get().is_none()
            && system_prompt_draft.get() != system_prompt_dirty_baseline
        {
            "project-system-prompt-text dirty"
        } else {
            "project-system-prompt-text"
        }
    };
    let system_prompt_event_options = system_prompt_history_for_options
        .into_iter()
        .map(|event| {
            view! {
                <option value=event.id.to_string()>{system_prompt_event_select_label(&event)}</option>
            }
        })
        .collect::<Vec<_>>();
    let save_system_prompt = move |_| {
        if system_prompt_pending.get_untracked()
            || selected_system_prompt_event_id.get_untracked().is_some()
        {
            return;
        }
        set_system_prompt_pending.set(true);
        let service = service.clone();
        let project = project_for_prompt.clone();
        let body = system_prompt_draft.get_untracked();
        leptos::task::spawn_local(async move {
            if service.update_system_prompt(project, body).await.is_ok() {
                prompt_refresh.run(());
            }
            set_system_prompt_pending.set(false);
        });
    };
    let initial_memory = project_view.memory.clone();
    let memory_dirty_baseline = initial_memory.clone();
    let memory_history_for_options = memory_events.clone();
    let memory_history_for_memory = memory_events;
    let (selected_memory_event_id, set_selected_memory_event_id) = signal(None::<i64>);
    let (memory_draft, set_memory_draft) = signal(initial_memory.clone());
    let (memory_pending, set_memory_pending) = signal(false);
    let memory_value = move || {
        selected_memory_event_id
            .get()
            .and_then(|event_id| {
                memory_history_for_memory
                    .iter()
                    .find(|event| event.id == event_id)
                    .map(|event| event.memory.clone())
                    .or_else(|| Some(format!("Memory event #{event_id} is no longer available.")))
            })
            .unwrap_or_else(|| memory_draft.get())
    };
    let memory_textarea_class = move || {
        if selected_memory_event_id.get().is_none() && memory_draft.get() != memory_dirty_baseline {
            "project-memory-text dirty"
        } else {
            "project-memory-text"
        }
    };
    let memory_event_options = memory_history_for_options
        .into_iter()
        .map(|event| {
            view! {
                <option value=event.id.to_string()>{memory_event_select_label(&event)}</option>
            }
        })
        .collect::<Vec<_>>();
    let save_memory = move |_| {
        if memory_pending.get_untracked() || selected_memory_event_id.get_untracked().is_some() {
            return;
        }
        set_memory_pending.set(true);
        let service = memory_service.clone();
        let project = project_for_memory.clone();
        let body = memory_draft.get_untracked();
        leptos::task::spawn_local(async move {
            if service.update_memory(project, body).await.is_ok() {
                refresh.run(());
            }
            set_memory_pending.set(false);
        });
    };

    view! {
        <section class="project-settings">
            <div>
                <h2>"System prompt"</h2>
                <div class="project-text-editor project-system-prompt-editor">
                    <div class="project-text-history">
                        <label for="project-system-prompt-version">"system prompt history"</label>
                        <select
                            id="project-system-prompt-version"
                            prop:value=move || {
                                selected_system_prompt_event_id
                                    .get()
                                    .map(|event_id| event_id.to_string())
                                    .unwrap_or_else(|| "current".to_owned())
                            }
                            on:change=move |event| {
                                let selected = event_target_value(&event);
                                if selected == "current" {
                                    set_selected_system_prompt_event_id.set(None);
                                } else if let Ok(event_id) = selected.parse::<i64>() {
                                    set_selected_system_prompt_event_id.set(Some(event_id));
                                }
                            }
                        >
                            <option value="current">"Current"</option>
                            {system_prompt_event_options}
                        </select>
                    </div>
                    <textarea
                        name="body"
                        class=system_prompt_textarea_class
                        placeholder="Project system prompt"
                        prop:value=system_prompt_value
                        readonly=move || selected_system_prompt_event_id.get().is_some()
                        on:input=move |event| {
                            if selected_system_prompt_event_id.get().is_none() {
                                set_system_prompt_draft.set(event_target_value(&event));
                            }
                        }
                    >
                        {initial_system_prompt}
                    </textarea>
                    <button
                        type="button"
                        disabled=move || {
                            selected_system_prompt_event_id.get().is_some()
                                || system_prompt_pending.get()
                        }
                        on:click=save_system_prompt
                    >
                        "Save prompt"
                    </button>
                </div>
            </div>
            <div>
                <h2>"Memory"</h2>
                <div class="project-text-editor project-memory-editor">
                    <div class="project-text-history">
                        <label for="project-memory-version">"memory history"</label>
                        <select
                            id="project-memory-version"
                            prop:value=move || {
                                selected_memory_event_id
                                    .get()
                                    .map(|event_id| event_id.to_string())
                                    .unwrap_or_else(|| "current".to_owned())
                            }
                            on:change=move |event| {
                                let selected = event_target_value(&event);
                                if selected == "current" {
                                    set_selected_memory_event_id.set(None);
                                } else if let Ok(event_id) = selected.parse::<i64>() {
                                    set_selected_memory_event_id.set(Some(event_id));
                                }
                            }
                        >
                            <option value="current">"Current"</option>
                            {memory_event_options}
                        </select>
                    </div>
                    <textarea
                        name="body"
                        class=memory_textarea_class
                        placeholder="Project memory"
                        prop:value=memory_value
                        readonly=move || selected_memory_event_id.get().is_some()
                        on:input=move |event| {
                            if selected_memory_event_id.get().is_none() {
                                set_memory_draft.set(event_target_value(&event));
                            }
                        }
                    >
                        {initial_memory}
                    </textarea>
                    <button
                        type="button"
                        disabled=move || {
                            selected_memory_event_id.get().is_some() || memory_pending.get()
                        }
                        on:click=save_memory
                    >
                        "Save memory"
                    </button>
                </div>
            </div>
        </section>
    }
}

fn memory_event_select_label(event: &ProjectMemoryEventView) -> String {
    format!("#{} {}", event.id, event.created_at)
}

fn system_prompt_event_select_label(event: &ProjectSystemPromptEventView) -> String {
    format!("#{} {}", event.id, event.created_at)
}

pub(crate) fn memory_event_ref_label(event: &ProjectMemoryEventRefView) -> String {
    if event.available {
        match event.created_at.as_deref() {
            Some(created_at) => format!("MemoryChanged #{} {}", event.event_id, created_at),
            None => format!("MemoryChanged #{}", event.event_id),
        }
    } else {
        format!("MemoryChanged #{} unavailable", event.event_id)
    }
}

#[component]
fn MaintenancePanel(project: String, refresh: Callback<()>) -> impl IntoView + 'static {
    let service = project_service();
    let (pending, set_pending) = signal(false);
    let cleanup = move |_| {
        if pending.get_untracked() {
            return;
        }
        set_pending.set(true);
        let service = service.clone();
        let project = project.clone();
        leptos::task::spawn_local(async move {
            if service.cleanup_worktrees(project).await.is_ok() {
                refresh.run(());
            }
            set_pending.set(false);
        });
    };

    view! {
        <section class="maintenance panel">
            <div class="panel-heading">
                <h2>"Maintenance"</h2>
            </div>
            <button type="button" disabled=move || pending.get() on:click=cleanup>
                "Cleanup worktrees"
            </button>
        </section>
    }
}
