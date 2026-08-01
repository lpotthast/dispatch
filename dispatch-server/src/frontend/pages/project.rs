use crate::{
    frontend::{
        components::{ActivePage, TopBar, cached_query, selected_project_signal},
        crudkit::{LabelKeysPanel, SwimLanesPanel, WorkItemStatesPanel, WorkItemsPanel},
        live_events::{project_page_event_matches, refetch_on_live_event},
        services::{project_cache, project_service},
    },
    shared::view_models::{
        CodexAppServerStatusView, ProjectMemoryEventRefView, ProjectMemoryEventView,
        ProjectSystemPromptEventView, ProjectView, WorkspaceEditorView,
    },
};
use leptonic::components::prelude::{Modal, ModalBody, ModalFooter, ModalHeader, ModalTitle};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectHistoryClearTarget {
    SystemPrompt,
    Memory,
}

#[derive(Clone, Copy)]
struct ProjectTextSettingsState {
    project_id: RwSignal<Option<i64>>,
    system_prompt_draft: RwSignal<String>,
    system_prompt_baseline: RwSignal<String>,
    selected_system_prompt_event_id: RwSignal<Option<i64>>,
    system_prompt_pending: RwSignal<bool>,
    memory_draft: RwSignal<String>,
    memory_baseline: RwSignal<String>,
    selected_memory_event_id: RwSignal<Option<i64>>,
    memory_pending: RwSignal<bool>,
    history_clear_target: RwSignal<Option<ProjectHistoryClearTarget>>,
    history_clear_pending: RwSignal<bool>,
}

impl ProjectTextSettingsState {
    fn new() -> Self {
        Self {
            project_id: RwSignal::new(None),
            system_prompt_draft: RwSignal::new(String::new()),
            system_prompt_baseline: RwSignal::new(String::new()),
            selected_system_prompt_event_id: RwSignal::new(None),
            system_prompt_pending: RwSignal::new(false),
            memory_draft: RwSignal::new(String::new()),
            memory_baseline: RwSignal::new(String::new()),
            selected_memory_event_id: RwSignal::new(None),
            memory_pending: RwSignal::new(false),
            history_clear_target: RwSignal::new(None),
            history_clear_pending: RwSignal::new(false),
        }
    }

    fn sync(
        self,
        project: &ProjectView,
        system_prompt_events: &[ProjectSystemPromptEventView],
        memory_events: &[ProjectMemoryEventView],
    ) {
        if self.project_id.get_untracked() != Some(project.id) {
            self.project_id.set(Some(project.id));
            self.system_prompt_draft.set(project.system_prompt.clone());
            self.system_prompt_baseline
                .set(project.system_prompt.clone());
            self.selected_system_prompt_event_id.set(None);
            self.system_prompt_pending.set(false);
            self.memory_draft.set(project.memory.clone());
            self.memory_baseline.set(project.memory.clone());
            self.selected_memory_event_id.set(None);
            self.memory_pending.set(false);
            self.history_clear_target.set(None);
            self.history_clear_pending.set(false);
            return;
        }

        sync_saved_text(
            self.system_prompt_draft,
            self.system_prompt_baseline,
            &project.system_prompt,
        );
        sync_saved_text(self.memory_draft, self.memory_baseline, &project.memory);
        if self
            .selected_system_prompt_event_id
            .get_untracked()
            .is_some_and(|selected_id| {
                !system_prompt_events
                    .iter()
                    .any(|event| event.id == selected_id)
            })
        {
            self.selected_system_prompt_event_id.set(None);
        }
        if self
            .selected_memory_event_id
            .get_untracked()
            .is_some_and(|selected_id| !memory_events.iter().any(|event| event.id == selected_id))
        {
            self.selected_memory_event_id.set(None);
        }
    }

    fn clear(self) {
        self.project_id.set(None);
        self.system_prompt_draft.set(String::new());
        self.system_prompt_baseline.set(String::new());
        self.selected_system_prompt_event_id.set(None);
        self.system_prompt_pending.set(false);
        self.memory_draft.set(String::new());
        self.memory_baseline.set(String::new());
        self.selected_memory_event_id.set(None);
        self.memory_pending.set(false);
        self.history_clear_target.set(None);
        self.history_clear_pending.set(false);
    }
}

fn sync_saved_text(draft: RwSignal<String>, baseline: RwSignal<String>, saved: &str) {
    let draft_value = draft.get_untracked();
    let was_clean = draft_value == baseline.get_untracked();
    baseline.set(saved.to_owned());
    if was_clean || draft_value == saved {
        draft.set(saved.to_owned());
    }
}

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
    let text_settings_state = ProjectTextSettingsState::new();
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
    let selected_project_for_events = selected_project;
    refetch_on_live_event(result.refresh, move |event| {
        let selected_project = selected_project_for_events.get();
        project_page_event_matches(event, selected_project.as_deref())
    });
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
                                text_settings_state
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
    text_settings_state: ProjectTextSettingsState,
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
        text_settings_state.sync(&project_view, &system_prompt_events, &memory_events);
        let project_id = project_view.id;
        view! {
            <>
                <ProjectTextSettings
                    project=project.clone()
                    project_view
                    system_prompt_events
                    memory_events
                    refresh
                    state=text_settings_state
                />
                <WorkItemsPanel
                    api_base_url=api_base_url.clone()
                    project=project.clone()
                    project_id=project_id
                />
                <LabelKeysPanel
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
        text_settings_state.clear();
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
    state: ProjectTextSettingsState,
) -> impl IntoView + 'static {
    let service = project_service();
    let prompt_service = service.clone();
    let memory_service = service.clone();
    let history_service = service;
    let project_for_prompt = project.clone();
    let project_for_memory = project.clone();
    let project_for_history = project;
    let prompt_refresh = refresh;
    let history_refresh = refresh;
    let project_id = project_view.id;
    let initial_system_prompt = project_view.system_prompt.clone();
    let system_prompt_draft = state.system_prompt_draft;
    let system_prompt_baseline = state.system_prompt_baseline;
    let selected_system_prompt_event_id = state.selected_system_prompt_event_id;
    let system_prompt_pending = state.system_prompt_pending;
    let has_system_prompt_history = !system_prompt_events.is_empty();
    let system_prompt_history_for_options = system_prompt_events.clone();
    let system_prompt_history_for_prompt = system_prompt_events;
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
            && system_prompt_draft.get() != system_prompt_baseline.get()
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
    let initial_memory = project_view.memory.clone();
    let memory_draft = state.memory_draft;
    let memory_baseline = state.memory_baseline;
    let selected_memory_event_id = state.selected_memory_event_id;
    let memory_pending = state.memory_pending;
    let has_memory_history = !memory_events.is_empty();
    let memory_history_for_options = memory_events.clone();
    let memory_history_for_memory = memory_events;
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
        if selected_memory_event_id.get().is_none() && memory_draft.get() != memory_baseline.get() {
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
    let history_clear_target = state.history_clear_target;
    let history_clear_pending = state.history_clear_pending;
    let system_prompt_has_unsaved_draft =
        Memo::new(move |_| system_prompt_draft.get() != system_prompt_baseline.get());
    let memory_has_unsaved_draft = Memo::new(move |_| memory_draft.get() != memory_baseline.get());
    let history_clear_blocked = Memo::new(move |_| {
        system_prompt_pending.get()
            || memory_pending.get()
            || history_clear_pending.get()
            || history_clear_target.get().is_some()
            || system_prompt_has_unsaved_draft.get()
            || memory_has_unsaved_draft.get()
    });
    let history_clear_modal_open = Signal::derive(move || history_clear_target.get().is_some());
    let save_system_prompt = move |_| {
        if system_prompt_pending.get_untracked()
            || history_clear_target.get_untracked().is_some()
            || history_clear_pending.get_untracked()
            || selected_system_prompt_event_id.get_untracked().is_some()
        {
            return;
        }
        system_prompt_pending.set(true);
        let service = prompt_service.clone();
        let project = project_for_prompt.clone();
        let body = system_prompt_draft.get_untracked();
        leptos::task::spawn_local(async move {
            if service.update_system_prompt(project, body).await.is_ok() {
                prompt_refresh.run(());
            }
            if state.project_id.get_untracked() == Some(project_id) {
                system_prompt_pending.set(false);
            }
        });
    };
    let save_memory = move |_| {
        if memory_pending.get_untracked()
            || history_clear_target.get_untracked().is_some()
            || history_clear_pending.get_untracked()
            || selected_memory_event_id.get_untracked().is_some()
        {
            return;
        }
        memory_pending.set(true);
        let service = memory_service.clone();
        let project = project_for_memory.clone();
        let body = memory_draft.get_untracked();
        leptos::task::spawn_local(async move {
            if service.update_memory(project, body).await.is_ok() {
                refresh.run(());
            }
            if state.project_id.get_untracked() == Some(project_id) {
                memory_pending.set(false);
            }
        });
    };
    let dismiss_history_clear = Callback::new(move |()| {
        if !history_clear_pending.get_untracked() {
            history_clear_target.set(None);
        }
    });
    let confirm_history_clear = Callback::new(move |()| {
        if history_clear_pending.get_untracked()
            || system_prompt_pending.get_untracked()
            || memory_pending.get_untracked()
            || system_prompt_has_unsaved_draft.get_untracked()
            || memory_has_unsaved_draft.get_untracked()
        {
            return;
        }
        let Some(target) = history_clear_target.get_untracked() else {
            return;
        };
        history_clear_pending.set(true);
        let service = history_service.clone();
        let project = project_for_history.clone();
        leptos::task::spawn_local(async move {
            let result = match target {
                ProjectHistoryClearTarget::SystemPrompt => {
                    service.clear_system_prompt_history(project).await
                }
                ProjectHistoryClearTarget::Memory => service.clear_memory_history(project).await,
            };
            if state.project_id.get_untracked() == Some(project_id) {
                history_clear_pending.set(false);
            }
            if result.is_ok() && state.project_id.get_untracked() == Some(project_id) {
                history_clear_target.set(None);
                history_refresh.run(());
            }
        });
    });

    view! {
        <section class="project-settings">
            <div>
                <h2>"System prompt"</h2>
                <div class="project-text-editor project-system-prompt-editor">
                    <div class="project-text-history">
                        <label for="project-system-prompt-version">"system prompt history"</label>
                        <div class="project-text-history-controls">
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
                                        selected_system_prompt_event_id.set(None);
                                    } else if let Ok(event_id) = selected.parse::<i64>() {
                                        selected_system_prompt_event_id.set(Some(event_id));
                                    }
                                }
                            >
                                <option value="current">"Current"</option>
                                {system_prompt_event_options}
                            </select>
                            <button
                                type="button"
                                class="secondary project-system-prompt-history-clear"
                                disabled=move || {
                                    !has_system_prompt_history || history_clear_blocked.get()
                                }
                                on:click=move |_| {
                                    history_clear_target
                                        .set(Some(ProjectHistoryClearTarget::SystemPrompt));
                                }
                            >
                                "Clear history"
                            </button>
                        </div>
                    </div>
                    <textarea
                        name="body"
                        class=system_prompt_textarea_class
                        placeholder="Project system prompt"
                        prop:value=system_prompt_value
                        readonly=move || selected_system_prompt_event_id.get().is_some()
                        on:input=move |event| {
                            if selected_system_prompt_event_id.get().is_none() {
                                system_prompt_draft.set(event_target_value(&event));
                            }
                        }
                    >
                        {initial_system_prompt}
                    </textarea>
                    <button
                        type="button"
                        class="project-system-prompt-save"
                        disabled=move || {
                            selected_system_prompt_event_id.get().is_some()
                                || system_prompt_pending.get()
                                || history_clear_modal_open.get()
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
                        <div class="project-text-history-controls">
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
                                        selected_memory_event_id.set(None);
                                    } else if let Ok(event_id) = selected.parse::<i64>() {
                                        selected_memory_event_id.set(Some(event_id));
                                    }
                                }
                            >
                                <option value="current">"Current"</option>
                                {memory_event_options}
                            </select>
                            <button
                                type="button"
                                class="secondary project-memory-history-clear"
                                disabled=move || {
                                    !has_memory_history || history_clear_blocked.get()
                                }
                                on:click=move |_| {
                                    history_clear_target
                                        .set(Some(ProjectHistoryClearTarget::Memory));
                                }
                            >
                                "Clear history"
                            </button>
                        </div>
                    </div>
                    <textarea
                        name="body"
                        class=memory_textarea_class
                        placeholder="Project memory"
                        prop:value=memory_value
                        readonly=move || selected_memory_event_id.get().is_some()
                        on:input=move |event| {
                            if selected_memory_event_id.get().is_none() {
                                memory_draft.set(event_target_value(&event));
                            }
                        }
                    >
                        {initial_memory}
                    </textarea>
                    <button
                        type="button"
                        class="project-memory-save"
                        disabled=move || {
                            selected_memory_event_id.get().is_some()
                                || memory_pending.get()
                                || history_clear_modal_open.get()
                        }
                        on:click=save_memory
                    >
                        "Save memory"
                    </button>
                </div>
            </div>
            <Modal
                id="project-history-clear-modal"
                class="history-clear-modal"
                show_when=history_clear_modal_open
                on_escape=move || dismiss_history_clear.run(())
                on_backdrop_interaction=move || dismiss_history_clear.run(())
            >
                <ModalHeader>
                    <ModalTitle>
                        {move || match history_clear_target.get() {
                            Some(ProjectHistoryClearTarget::SystemPrompt) => {
                                "Clear system prompt history?"
                            }
                            Some(ProjectHistoryClearTarget::Memory) => "Clear memory history?",
                            None => "Clear history?",
                        }}
                    </ModalTitle>
                </ModalHeader>
                <ModalBody>
                    <p>
                        {move || match history_clear_target.get() {
                            Some(ProjectHistoryClearTarget::SystemPrompt) => {
                                "This permanently deletes all saved system prompt history. The currently saved system prompt is kept."
                            }
                            Some(ProjectHistoryClearTarget::Memory) => {
                                "This permanently deletes all saved memory history. The currently saved memory is kept."
                            }
                            None => "",
                        }}
                    </p>
                    <p>"This action cannot be undone."</p>
                </ModalBody>
                <ModalFooter>
                    <button
                        type="button"
                        class="secondary history-clear-cancel"
                        disabled=move || history_clear_pending.get()
                        on:click=move |_| dismiss_history_clear.run(())
                    >
                        "Cancel"
                    </button>
                    <button
                        type="button"
                        class="danger history-clear-confirm"
                        disabled=move || history_clear_pending.get()
                        on:click=move |_| confirm_history_clear.run(())
                    >
                        {move || {
                            if history_clear_pending.get() {
                                "Clearing history…"
                            } else {
                                "Clear history"
                            }
                        }}
                    </button>
                </ModalFooter>
            </Modal>
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
