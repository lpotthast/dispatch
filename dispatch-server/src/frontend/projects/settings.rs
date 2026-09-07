use crate::frontend::projects::types::ProjectSettings;
use crate::frontend::queries::query_resource;
use crate::{
    frontend::{
        board::lanes::SwimLanesPanel,
        components::selected_project_signal,
        items::{crud::WorkItemsPanel, label_keys::LabelKeysPanel, states::WorkItemStatesPanel},
        live_events::{project_page_event_matches, refetch_on_live_event},
        projects::service::project_service,
    },
    shared::view_models::{ProjectSystemPromptEventView, ProjectView},
};
use leptonic::components::prelude::{Modal, ModalBody, ModalFooter, ModalHeader, ModalTitle};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_query_map;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProjectHistoryClearTarget {
    SystemPrompt,
}

#[derive(Clone, Copy)]
struct ProjectTextSettingsState {
    project_id: RwSignal<Option<i64>>,
    system_prompt_draft: RwSignal<String>,
    system_prompt_baseline: RwSignal<String>,
    selected_system_prompt_event_id: RwSignal<Option<i64>>,
    system_prompt_pending: RwSignal<bool>,
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
            history_clear_target: RwSignal::new(None),
            history_clear_pending: RwSignal::new(false),
        }
    }

    fn sync(self, project: &ProjectView, system_prompt_events: &[ProjectSystemPromptEventView]) {
        if self.project_id.get_untracked() != Some(project.id) {
            self.project_id.set(Some(project.id));
            self.system_prompt_draft.set(project.system_prompt.clone());
            self.system_prompt_baseline
                .set(project.system_prompt.clone());
            self.selected_system_prompt_event_id.set(None);
            self.system_prompt_pending.set(false);
            self.history_clear_target.set(None);
            self.history_clear_pending.set(false);
            return;
        }

        sync_saved_text(
            self.system_prompt_draft,
            self.system_prompt_baseline,
            &project.system_prompt,
        );
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
    }

    fn clear(self) {
        self.project_id.set(None);
        self.system_prompt_draft.set(String::new());
        self.system_prompt_baseline.set(String::new());
        self.selected_system_prompt_event_id.set(None);
        self.system_prompt_pending.set(false);
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

#[component]
pub fn PageProject() -> impl IntoView {
    let selected_project = selected_project_signal();
    let text_settings_state = ProjectTextSettingsState::new();
    let query = use_query_map();
    let store = crate::frontend::projects::store::project_store();
    let initial = store.cached_project_page_untracked(&selected_project.get_untracked());
    let store_for_cache = store.clone();
    let store_for_load = store.clone();
    let store_for_refresh = store.clone();
    let store_for_seed = store.clone();
    let result = query_resource(
        initial,
        move || selected_project.get(),
        move |selected_project| store_for_cache.cached_project_page(selected_project),
        move |selected_project| {
            let store = store_for_load.clone();
            let selected_project = selected_project.clone();
            async move { store.load_project_page(selected_project).await }
        },
        move |key, value| store_for_seed.seed_project_page(key, value),
        move |key| store_for_refresh.invalidate_project_page(key),
    );
    let selected_project_for_events = selected_project;
    refetch_on_live_event(result.refresh, move |event| {
        let selected_project = selected_project_for_events.get();
        project_page_event_matches(event, selected_project.as_deref())
    });

    view! {
            <Title text="Project"/>
            <div>
                <main class="page-shell project-page">
                    <section class="page-heading">
                        <h1>"Project"</h1>
                        <p class="muted">"Settings and maintenance for the selected project."</p>
                    </section>
                    <Transition>
                    <For each=move || result.value.get() key=|page| page.selected_project_view.as_ref().map(|project| project.id) children=move |initial| {
                        let identity = initial.selected_project_view.as_ref().map(|project| project.id);
                        let page = Signal::derive(move || result.value.get().filter(|page| page.selected_project_view.as_ref().map(|project| project.id) == identity).unwrap_or_else(|| initial.clone()));
                        let edit_swim_lane_id = Signal::derive(move || query.read().get("edit_swim_lane").and_then(|value| value.parse().ok()));
                        view! { <ProjectContent page edit_swim_lane_id refresh=result.refresh text_settings_state/> }
                    }/>
                </Transition>
    <crate::frontend::components::QueryFeedback pending=result.pending error=result.error refresh=result.refresh/>
                </main>
            </div>
        }
}

#[component]
fn ProjectContent(
    page: Signal<ProjectSettings>,
    edit_swim_lane_id: Signal<Option<i64>>,
    refresh: Callback<()>,
    text_settings_state: ProjectTextSettingsState,
) -> impl IntoView {
    let api_base_url = crate::frontend::http::HttpService::get().api_base_url();
    let ProjectSettings {
        selected_project,
        selected_project_view,
        system_prompt_events,
    } = page.get_untracked();

    if let (Some(project), Some(project_view)) = (selected_project, selected_project_view) {
        text_settings_state.sync(&project_view, &system_prompt_events);
        let project_id = project_view.id;
        Effect::new(move |_| {
            let page = page.get();
            if let Some(project) = &page.selected_project_view {
                text_settings_state.sync(project, &page.system_prompt_events);
            }
        });
        let project_view = Signal::derive(move || {
            page.get()
                .selected_project_view
                .unwrap_or_else(|| project_view.clone())
        });
        let system_prompt_events = Signal::derive(move || page.get().system_prompt_events);
        view! {
            <>
                <ProjectTextSettings
                    project=project.clone()
                    project_view
                    system_prompt_events
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
    project_view: Signal<ProjectView>,
    system_prompt_events: Signal<Vec<ProjectSystemPromptEventView>>,
    refresh: Callback<()>,
    state: ProjectTextSettingsState,
) -> impl IntoView + 'static {
    let service = project_service();
    let prompt_service = service.clone();
    let history_service = service;
    let project_for_prompt = project.clone();
    let project_for_history = project;
    let prompt_refresh = refresh;
    let history_refresh = refresh;
    let project_id = project_view.get_untracked().id;
    let initial_system_prompt = project_view.get_untracked().system_prompt;
    let system_prompt_draft = state.system_prompt_draft;
    let system_prompt_baseline = state.system_prompt_baseline;
    let selected_system_prompt_event_id = state.selected_system_prompt_event_id;
    let system_prompt_pending = state.system_prompt_pending;
    let has_system_prompt_history = move || !system_prompt_events.get().is_empty();
    let system_prompt_history_for_options = system_prompt_events;
    let system_prompt_history_for_prompt = system_prompt_events;
    let system_prompt_value = move || {
        selected_system_prompt_event_id
            .get()
            .and_then(|event_id| {
                system_prompt_history_for_prompt
                    .get()
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
    let system_prompt_event_options = move || {
        system_prompt_history_for_options.get()
        .into_iter()
        .map(|event| {
            view! {
                <option value=event.id.to_string()>{system_prompt_event_select_label(&event)}</option>
            }
        })
        .collect::<Vec<_>>()
    };
    let history_clear_target = state.history_clear_target;
    let history_clear_pending = state.history_clear_pending;
    let system_prompt_has_unsaved_draft =
        Memo::new(move |_| system_prompt_draft.get() != system_prompt_baseline.get());
    let history_clear_blocked = Memo::new(move |_| {
        system_prompt_pending.get()
            || history_clear_pending.get()
            || history_clear_target.get().is_some()
            || system_prompt_has_unsaved_draft.get()
    });
    let history_clear_modal_open = Signal::derive(move || history_clear_target.get().is_some());
    let save_prompt = Action::new(move |body: &String| {
        let service = prompt_service.clone();
        let project = project_for_prompt.clone();
        let body = body.clone();
        async move {
            if service.update_system_prompt(project, body).await.is_ok()
                && state.project_id.get_untracked() == Some(project_id)
            {
                prompt_refresh.run(());
            }
        }
    });
    Effect::new(move |_| system_prompt_pending.set(save_prompt.pending().get()));
    let save_system_prompt = move |_| {
        if !save_prompt.pending().get_untracked()
            && history_clear_target.get_untracked().is_none()
            && !history_clear_pending.get_untracked()
            && selected_system_prompt_event_id.get_untracked().is_none()
        {
            save_prompt.dispatch(system_prompt_draft.get_untracked());
        }
    };
    let dismiss_history_clear = Callback::new(move |()| {
        if !history_clear_pending.get_untracked() {
            history_clear_target.set(None);
        }
    });
    let clear_history = Action::new(move |target: &ProjectHistoryClearTarget| {
        let service = history_service.clone();
        let project = project_for_history.clone();
        let target = *target;
        async move {
            let result = match target {
                ProjectHistoryClearTarget::SystemPrompt => {
                    service.clear_system_prompt_history(project).await
                }
            };
            if result.is_ok() && state.project_id.get_untracked() == Some(project_id) {
                history_clear_target.set(None);
                history_refresh.run(());
            }
        }
    });
    Effect::new(move |_| history_clear_pending.set(clear_history.pending().get()));
    let confirm_history_clear = Callback::new(move |()| {
        if !clear_history.pending().get_untracked()
            && !save_prompt.pending().get_untracked()
            && !system_prompt_has_unsaved_draft.get_untracked()
            && let Some(target) = history_clear_target.get_untracked()
        {
            clear_history.dispatch(target);
        }
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
                                    !has_system_prompt_history() || history_clear_blocked.get()
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

fn system_prompt_event_select_label(event: &ProjectSystemPromptEventView) -> String {
    format!("#{} {}", event.id, event.created_at)
}

#[component]
fn MaintenancePanel(project: String, refresh: Callback<()>) -> impl IntoView + 'static {
    let service = project_service();
    let cleanup_action = Action::new(move |_: &()| {
        let service = service.clone();
        let project = project.clone();
        async move {
            if service.cleanup_worktrees(project).await.is_ok() {
                refresh.run(());
            }
        }
    });
    let pending = cleanup_action.pending();
    let cleanup = move |_| {
        if pending.get_untracked() {
            return;
        }
        cleanup_action.dispatch(());
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
