use crate::{
    frontend::{
        components::{
            ActivePage, TopBarAutomation, WorkItemStatesContext, cached_query,
            claim_badge_with_source, encode_path, format_label, preview, project_workspace_panel,
            provide_work_item_states_context, selected_project_signal, top_bar,
        },
        crudkit::{WorkItemsPanel, work_items_crudkit_config_for_view},
        live_events::{board_items_event_matches, refetch_on_live_event},
        services::{board_service, project_cache},
        work_item_creation::{
            CreateItemOpenRequest, CreateItemStateOption, default_state_identifier,
            state_identifier_from_lane_filter, state_options_for_open_request,
            state_options_from_project_states,
        },
    },
    shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, AgentGitHardResetPolicy, AutomationStatusView,
        CodexAppServerStatusView, FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView,
        ProjectMemoryEventRefView, ProjectMemoryEventView, ProjectSettingsView,
        ProjectSystemPromptEventView, ProjectView, RevertStrategy, SwimLaneItemOrder, SwimLaneView,
        WorkItemStateView, WorkItemView, WorkspaceEditorView, WorkspaceMode,
    },
};
use crudkit_leptos::{
    crud_instance::CrudInstanceContext,
    crud_instance_config::{CrudActionsPlacement, CrudNavigationConfig},
    crud_instance_mgr::CrudInstanceMgr,
    crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    },
    crudkit_web::view::SerializableCrudView,
    prelude::*,
};
use leptonic::components::prelude::{Icon, Modal, ModalBody, ModalFooter, ModalHeader, ModalTitle};
use leptonic::prelude::icondata;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_use::use_interval_fn;
use serde::{Deserialize, Serialize};

const BOARD_ITEMS_REFRESH_INTERVAL_MS: u64 = 30_000;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub settings: Option<ProjectSettingsView>,
    pub workspace_editors: Vec<WorkspaceEditorView>,
    pub system_prompt_events: Vec<ProjectSystemPromptEventView>,
    pub memory_events: Vec<ProjectMemoryEventView>,
    pub automation_status: Option<AutomationStatusView>,
    pub automation_running: bool,
    pub items: Vec<WorkItemView>,
    pub swim_lanes: Vec<SwimLaneView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub label_suggestions: Vec<ProjectLabelView>,
    pub misconfigured_item_count: i64,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardItemsSection {
    pub items: Vec<WorkItemView>,
    pub swim_lanes: Vec<SwimLaneView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub misconfigured_item_count: i64,
}

#[component]
pub fn PageBoard() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = board_service();
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
    let cache = project_cache();
    cache.track(result.value, |page| &page.projects);
    let initial_auto_commit = result
        .value
        .get_untracked()
        .and_then(|page| page.settings.map(|settings| settings.auto_commit))
        .or_else(|| {
            let selected = selected_project.get_untracked();
            cache.projects().with_untracked(|projects| {
                selected
                    .as_ref()
                    .and_then(|selected| projects.iter().find(|project| project.name == *selected))
                    .or_else(|| projects.first())
                    .map(|project| project.auto_commit)
            })
        })
        .unwrap_or(false);
    let (auto_commit, set_auto_commit) = signal(initial_auto_commit);
    Effect::new(move |_| {
        let page_setting = result
            .value
            .get()
            .and_then(|page| page.settings.map(|settings| settings.auto_commit));
        let cached_setting = cache.projects().with(|projects| {
            let selected = selected_project.get();
            selected
                .as_ref()
                .and_then(|selected| projects.iter().find(|project| project.name == *selected))
                .or_else(|| projects.first())
                .map(|project| project.auto_commit)
        });
        if let Some(auto_commit) = page_setting.or(cached_setting) {
            set_auto_commit.set(auto_commit);
        }
    });
    let active_project_names = Signal::derive(move || {
        result
            .value
            .get()
            .map(|page| page.active_project_names)
            .unwrap_or_default()
    });
    let automation = Signal::derive(move || {
        let page = result.value.get();
        let project = page
            .as_ref()
            .and_then(|page| page.selected_project.clone())
            .or_else(|| selected_project.get())
            .or_else(|| {
                cache
                    .projects()
                    .get()
                    .first()
                    .map(|project| project.name.clone())
            })?;
        let cached_project = cache
            .projects()
            .get()
            .into_iter()
            .find(|candidate| candidate.name == project);
        let workspace_mode = page
            .as_ref()
            .and_then(|page| page.settings.as_ref())
            .map(|settings| settings.workspace_mode)
            .or_else(|| cached_project.map(|project| project.workspace_mode))
            .unwrap_or(WorkspaceMode::CurrentBranch);
        let running = page
            .as_ref()
            .map(|page| {
                page.automation_running
                    || page
                        .automation_status
                        .as_ref()
                        .is_some_and(|status| status.running_runs > 0)
            })
            .unwrap_or(false);
        Some(TopBarAutomation {
            project,
            running,
            workspace_mode,
            auto_commit,
            set_auto_commit,
        })
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
        ActivePage::Board,
        automation,
        codex_status,
    );
    view! {
        <Title text="Dispatch"/>
        <div>
            {topbar}
            <main class="page-shell">
                <For
                    each=move || result.value.get()
                    key=|page| page.selected_project.clone()
                    children=move |page| board_content(page, auto_commit, set_auto_commit)
                />
            </main>
        </div>
    }
}

fn board_content(
    page: BoardPage,
    auto_commit: ReadSignal<bool>,
    set_auto_commit: WriteSignal<bool>,
) -> AnyView {
    let BoardPage {
        projects: _,
        active_project_names: _,
        selected_project,
        selected_project_view,
        settings,
        workspace_editors,
        system_prompt_events,
        memory_events,
        automation_status: _,
        automation_running: _,
        items,
        swim_lanes,
        work_item_states,
        label_suggestions,
        misconfigured_item_count,
        api_base_url,
        codex_status: _,
    } = page;

    if let (Some(project), Some(project_view), Some(settings)) =
        (selected_project.clone(), selected_project_view, settings)
    {
        let board_return_to = format!("/?project={}", encode_path(&project));
        let project_workspace = project_workspace_panel(
            &project,
            &project_view,
            workspace_editors.clone(),
            board_return_to.clone(),
        );
        let (show_create_item_modal, set_show_create_item_modal) = signal(false);
        let initial_create_item_state_options =
            state_options_from_project_states(&work_item_states);
        let work_item_states_context = provide_work_item_states_context(work_item_states);
        let initial_create_item_state =
            default_state_identifier(&initial_create_item_state_options);
        let (create_item_state, set_create_item_state) = signal(initial_create_item_state);
        let (create_item_state_options, set_create_item_state_options) =
            signal(initial_create_item_state_options);
        let create_item_label_suggestions = Signal::derive(move || label_suggestions.clone());
        let open_create_item = Callback::new(move |request: CreateItemOpenRequest| {
            let states = work_item_states_context.states.get_untracked();
            let options = state_options_for_open_request(&states, &request);
            if options.is_empty() {
                return;
            }
            set_create_item_state.set(default_state_identifier(&options));
            set_create_item_state_options.set(options);
            set_show_create_item_modal.set(true);
        });
        let board = view! {
            <LiveBoardItems
                project=project.clone()
                initial_items=items
                initial_swim_lanes=swim_lanes
                initial_misconfigured_item_count=misconfigured_item_count
                open_create_item=open_create_item
            />
        };
        let admin_project_id = project_view.id;
        let create_item = create_item_modal(
            api_base_url.clone(),
            admin_project_id,
            show_create_item_modal,
            set_show_create_item_modal,
            create_item_state_options,
            create_item_state,
            create_item_label_suggestions,
        );
        let work_items_api_base_url = api_base_url.clone();
        let project_settings = project_settings_view(
            &project,
            project_view,
            settings,
            system_prompt_events,
            memory_events,
            auto_commit,
            set_auto_commit,
        );
        let maintenance = maintenance_view(&project);

        view! {
            <>
                    <section class="workspace-bar" aria-label="Workspace">
                        <span class="workspace-bar-title">"Workspace"</span>
                        {project_workspace}
                    </section>
                    {board}
                    {create_item}
                    <WorkItemsPanel
                        api_base_url=work_items_api_base_url
                        project=project.clone()
                        project_id=admin_project_id
                    />
                    {project_settings}
                    {maintenance}
            </>
        }
        .into_any()
    } else {
        view! {
            <>
                <section class="empty-state">
                    <h2>"Choose a project"</h2>
                    <a class="button-link" href="/projects">"Projects"</a>
                </section>
            </>
        }
        .into_any()
    }
}

#[component]
fn LiveBoardItems(
    project: String,
    initial_items: Vec<WorkItemView>,
    initial_swim_lanes: Vec<SwimLaneView>,
    initial_misconfigured_item_count: i64,
    open_create_item: Callback<CreateItemOpenRequest>,
) -> impl IntoView + 'static {
    let service = board_service();
    let (items, set_items) = signal(initial_items);
    let (swim_lanes, set_swim_lanes) = signal(initial_swim_lanes);
    let work_item_states_context = use_context::<WorkItemStatesContext>()
        .expect("work item states context should be provided before rendering board items");
    let work_item_states = work_item_states_context.states;
    let set_work_item_states = work_item_states_context.set_states;
    let (misconfigured_item_count, set_misconfigured_item_count) =
        signal(initial_misconfigured_item_count);
    let initial = service.cached_items_untracked(&project);
    let project_for_loader = project.clone();
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let section = cached_query(
        initial,
        move || project_for_loader.clone(),
        move |project| service_for_cache.cached_items(project),
        move |project| {
            let service = service_for_load.clone();
            let project = project.clone();
            async move { service.load_items(project).await }
        },
    );
    let refresh = section.refresh;
    let _poll = use_interval_fn(move || refresh.run(()), BOARD_ITEMS_REFRESH_INTERVAL_MS);
    let project_for_events = project.clone();
    refetch_on_live_event(section.refresh, move |event| {
        board_items_event_matches(event, project_for_events.as_str())
    });

    Effect::new(move |_| {
        if let Some(section) = section.value.get() {
            set_items.set(section.items);
            let updated_swim_lanes = section.swim_lanes;
            let updated_work_item_states = section.work_item_states;
            set_swim_lanes.set(updated_swim_lanes);
            set_work_item_states.set(updated_work_item_states);
            set_misconfigured_item_count.set(section.misconfigured_item_count);
        }
    });

    view! {
        {move || {
            board_view(
                project.clone(),
                items.get(),
                swim_lanes.get(),
                work_item_states.get(),
                misconfigured_item_count.get(),
                open_create_item,
            )
        }}
    }
}

fn project_settings_view(
    project: &str,
    project_view: ProjectView,
    settings: ProjectSettingsView,
    system_prompt_events: Vec<ProjectSystemPromptEventView>,
    memory_events: Vec<ProjectMemoryEventView>,
    auto_commit: ReadSignal<bool>,
    set_auto_commit: WriteSignal<bool>,
) -> impl IntoView + 'static {
    let prompt_action = format!("/projects/{}/system-prompt", encode_path(project));
    let memory_action = format!("/projects/{}/memory", encode_path(project));
    let commit_policy_action = format!("/projects/{}/settings/commit-policy", encode_path(project));
    let commit_standard = settings.commit_standard.clone();
    let max_read_only_agents = settings.max_read_only_agents.to_string();
    let manual_revert_selected = settings.revert_strategy == RevertStrategy::Manual;
    let git_reset_selected = settings.revert_strategy == RevertStrategy::GitReset;
    let git_policy = settings.agent_git_command_policy.clone();
    let hard_reset_never_selected = git_policy.hard_reset == AgentGitHardResetPolicy::Never;
    let hard_reset_isolated_selected =
        git_policy.hard_reset == AgentGitHardResetPolicy::IsolatedWorkspaces;
    let initial_system_prompt = project_view.system_prompt.clone();
    let system_prompt_dirty_baseline = initial_system_prompt.clone();
    let system_prompt_history_for_options = system_prompt_events.clone();
    let system_prompt_history_for_prompt = system_prompt_events;
    let (selected_system_prompt_event_id, set_selected_system_prompt_event_id) =
        signal(None::<i64>);
    let (system_prompt_draft, set_system_prompt_draft) = signal(initial_system_prompt.clone());
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
    let initial_memory = project_view.memory.clone();
    let memory_dirty_baseline = initial_memory.clone();
    let memory_history_for_options = memory_events.clone();
    let memory_history_for_memory = memory_events.clone();
    let (selected_memory_event_id, set_selected_memory_event_id) = signal(None::<i64>);
    let (memory_draft, set_memory_draft) = signal(initial_memory.clone());
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

    view! {
        <section class="project-settings">
            <div>
                <h2>"System prompt"</h2>
                <form method="post" action=prompt_action>
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
                    <button disabled=move || selected_system_prompt_event_id.get().is_some()>
                        "Save prompt"
                    </button>
                </form>
            </div>
            <div>
                <h2>"Memory"</h2>
                <form method="post" action=memory_action>
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
                    <button disabled=move || selected_memory_event_id.get().is_some()>
                        "Save memory"
                    </button>
                </form>
            </div>
            <div class="commit-policy">
                <h2>"Automation policy"</h2>
                <form method="post" action=commit_policy_action>
                    <label for="project-max-read-only-agents">"Read-only agents"</label>
                    <input
                        id="project-max-read-only-agents"
                        type="number"
                        min="0"
                        step="1"
                        name="max_read_only_agents"
                        value=max_read_only_agents
                    />
                    <label class="checkbox-row" for="project-auto-commit">
                        <input
                            id="project-auto-commit"
                            type="checkbox"
                            name="auto_commit"
                            prop:checked=move || auto_commit.get()
                            on:change=move |event| {
                                set_auto_commit.set(event_target_checked(&event));
                            }
                        />
                        <span>"Auto-Commit"</span>
                    </label>
                    <label for="project-commit-standard">"Commit standard"</label>
                    <textarea
                        id="project-commit-standard"
                        name="commit_standard"
                        placeholder="Commit message standard"
                    >
                        {commit_standard}
                    </textarea>
                    <label for="project-revert-strategy">"Failure revert"</label>
                    <select id="project-revert-strategy" name="revert_strategy">
                        <option value="manual" selected=manual_revert_selected>"revert manually"</option>
                        <option value="git_reset" selected=git_reset_selected>"git reset"</option>
                    </select>
                    <div class="git-command-policy">
                        <label class="checkbox-row" for="project-git-add">
                            <input
                                id="project-git-add"
                                type="checkbox"
                                name="git_add"
                                checked=git_policy.add
                            />
                            <span>"git add"</span>
                        </label>
                        <label class="checkbox-row" for="project-git-commit">
                            <input
                                id="project-git-commit"
                                type="checkbox"
                                name="git_commit"
                                checked=git_policy.commit
                            />
                            <span>"git commit"</span>
                        </label>
                        <label class="checkbox-row" for="project-git-push">
                            <input
                                id="project-git-push"
                                type="checkbox"
                                name="git_push"
                                checked=git_policy.push
                            />
                            <span>"git push"</span>
                        </label>
                        <label class="checkbox-row" for="project-git-reset">
                            <input
                                id="project-git-reset"
                                type="checkbox"
                                name="git_reset"
                                checked=git_policy.reset
                            />
                            <span>"git reset"</span>
                        </label>
                    </div>
                    <label for="project-git-hard-reset">"Hard reset"</label>
                    <select id="project-git-hard-reset" name="git_hard_reset">
                        <option value="isolated_workspaces" selected=hard_reset_isolated_selected>
                            "isolated branches/worktrees only"
                        </option>
                        <option value="never" selected=hard_reset_never_selected>"never"</option>
                    </select>
                    <button>"Save policy"</button>
                </form>
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

fn maintenance_view(project: &str) -> impl IntoView + 'static {
    let cleanup_action = format!(
        "/projects/{}/automation/cleanup-worktrees",
        encode_path(project)
    );

    view! {
        <section class="maintenance panel">
            <div class="panel-heading">
                <h2>"Maintenance"</h2>
            </div>
            <form method="post" action=cleanup_action>
                <button type="submit">"Cleanup worktrees"</button>
            </form>
        </section>
    }
}

fn create_item_modal(
    api_base_url: String,
    project_id: i64,
    show_when: ReadSignal<bool>,
    set_show_when: WriteSignal<bool>,
    state_options: ReadSignal<Vec<CreateItemStateOption>>,
    selected_state: ReadSignal<String>,
    label_suggestions: Signal<Vec<ProjectLabelView>>,
) -> impl IntoView + 'static {
    let api_base_url = StoredValue::new(api_base_url);
    let (context, set_context) = signal(None::<CrudInstanceContext>);
    let close_modal = Callback::new(move |()| set_show_when.set(false));
    let close_modal_for_exit = close_modal;
    let request_close = Callback::new(move |()| {
        if let Some(context) = context.get_untracked() {
            context.request_leave();
        } else {
            close_modal.run(());
        }
    });
    Effect::new(move |_| {
        if !show_when.get() {
            set_context.set(None);
        }
    });
    let default_create_state = Signal::derive(move || selected_state.get());
    let crud_state_options = Signal::derive(move || state_options.get());
    let request_close_on_escape = request_close;
    let request_close_on_backdrop = request_close;
    let request_close_on_header = request_close;
    let request_close_on_footer = request_close;
    view! {
        <Modal
            id="new-item-modal"
            class="new-item-modal"
            show_when=show_when
            on_escape=move || request_close_on_escape.run(())
            on_backdrop_interaction=move || request_close_on_backdrop.run(())
        >
            <ModalHeader>
                <ModalTitle>"New item"</ModalTitle>
                <button
                    type="button"
                    class="secondary icon-button modal-close-button"
                    title="Close"
                    aria-label="Close"
                    on:click=move |_| request_close_on_header.run(())
                >
                    <Icon icon=icondata::BsX/>
                </button>
            </ModalHeader>
            <ModalBody>
                {move || {
                    if !show_when.get() {
                        return ().into_any();
                    }
                    if state_options.get().is_empty() {
                        return view! {
                            <p class="muted">"No states available."</p>
                        }
                        .into_any();
                    }
                    let api_base_url = api_base_url.get_value();
                    let on_exit = close_modal_for_exit;
                    view! {
                        <div class="new-item-form crudkit-new-item" data-crudkit-leptos="work-item-create">
                            <CrudInstanceMgr>
                                <CrudInstance
                                    name="work-item-create"
                                    config=work_items_crudkit_config_for_view(
                                        api_base_url.clone(),
                                        project_id,
                                        SerializableCrudView::Create,
                                        CrudNavigationConfig::embedded_single_entity()
                                            .with_create_actions_placement(CrudActionsPlacement::External),
                                        default_create_state,
                                        Some(crud_state_options),
                                        label_suggestions,
                                    )
                                    on_exit=on_exit
                                    on_context_created=Callback::new(move |context| {
                                        set_context.set(Some(context));
                                    })
                                />
                            </CrudInstanceMgr>
                        </div>
                    }
                    .into_any()
                }}
            </ModalBody>
            <ModalFooter>
                <button
                    type="button"
                    class="secondary"
                    on:click=move |_| request_close_on_footer.run(())
                >
                    "Cancel"
                </button>
                <CrudActionsOutlet context=context action_slot=CrudActionSlot::CreatePrimary />
            </ModalFooter>
        </Modal>
    }
}

fn board_view(
    project: String,
    items: Vec<WorkItemView>,
    swim_lanes: Vec<SwimLaneView>,
    _work_item_states: Vec<WorkItemStateView>,
    misconfigured_item_count: i64,
    open_create_item: Callback<CreateItemOpenRequest>,
) -> impl IntoView + 'static {
    let lanes = swim_lanes
        .into_iter()
        .map(|lane| {
            let label = lane.name.clone();
            let mut lane_items = items
                .iter()
                .filter(|item| item_matches_condition(item, &lane.filter))
                .cloned()
                .collect::<Vec<_>>();
            sort_lane_items(&mut lane_items, lane.item_order);
            let cards = lane_items
                .into_iter()
                .map(|item| item_card(project.clone(), item))
                .collect::<Vec<_>>();
            let count = cards.len();
            let create_state = state_identifier_from_lane_filter(&lane.filter);
            let add_button = if lane.can_create_items {
                create_state
                    .map(|create_state| {
                        view! {
                            <button
                                type="button"
                                class="lane-add"
                                on:click=move |_| {
                                    open_create_item.run(CreateItemOpenRequest::SingleState(create_state.clone()))
                                }
                            >
                                "+ Add"
                            </button>
                        }
                        .into_any()
                    })
                    .unwrap_or_else(|| ().into_any())
            } else {
                ().into_any()
            };
            let edit_href = lane_edit_href(&project, lane.id);
            let edit_label = format!("Edit {}", label);
            view! {
                <section class="lane">
                    <header class="lane-header">
                        <div class="lane-heading">
                            <h2>{label}</h2>
                            <span class="lane-count">{count}</span>
                        </div>
                        <div class="lane-actions">
                            {add_button}
                            <a
                                class="lane-edit"
                                href=edit_href
                                title=edit_label.clone()
                                aria-label=edit_label
                            >
                                "⚙"
                            </a>
                        </div>
                    </header>
                    <div class="lane-cards">{cards}</div>
                </section>
            }
        })
        .collect::<Vec<_>>();
    let warning = if misconfigured_item_count > 0 {
        let item_word = if misconfigured_item_count == 1 {
            "item"
        } else {
            "items"
        };
        let verb = if misconfigured_item_count == 1 {
            "has"
        } else {
            "have"
        };
        let message =
            format!("{misconfigured_item_count} {item_word} {verb} an unknown or missing state.");

        view! {
            <section class="board-state-warning" role="status">
                <strong>"State warning"</strong>
                <span>{message}</span>
                <a href="#work-items-admin">"Review work items"</a>
            </section>
        }
        .into_any()
    } else {
        ().into_any()
    };
    view! {
        <div class="board-stack">
            <section class="board">{lanes}</section>
            {warning}
        </div>
    }
}

fn lane_edit_href(project: &str, lane_id: i64) -> String {
    format!(
        "/projects?project={}&edit_swim_lane={}#swim-lanes",
        encode_path(project),
        lane_id
    )
}

fn item_matches_condition(item: &WorkItemView, condition: &Condition) -> bool {
    match condition {
        Condition::All(elements) => elements
            .iter()
            .all(|element| item_matches_condition_element(item, element)),
        Condition::Any(elements) => elements
            .iter()
            .any(|element| item_matches_condition_element(item, element)),
    }
}

fn item_matches_condition_element(item: &WorkItemView, element: &ConditionElement) -> bool {
    match element {
        ConditionElement::Clause(clause) => item_matches_clause(item, clause),
        ConditionElement::Condition(condition) => item_matches_condition(item, condition),
    }
}

fn item_matches_clause(item: &WorkItemView, clause: &ConditionClause) -> bool {
    let key = clause.column_name.trim();
    let label = item.labels.iter().find(|label| label.key == key);
    let label_value = label.and_then(|label| label.value.as_deref());

    match (&clause.operator, &clause.value) {
        (Operator::Equal, ConditionClauseValue::Bool(expected)) => label.is_some() == *expected,
        (Operator::NotEqual, ConditionClauseValue::Bool(expected)) => label.is_some() != *expected,
        (Operator::Equal, ConditionClauseValue::String(expected)) => {
            label_value == Some(expected.as_str())
        }
        (Operator::NotEqual, ConditionClauseValue::String(expected)) => {
            label_value != Some(expected.as_str())
        }
        (Operator::Equal, ConditionClauseValue::Json(serde_json::Value::Null)) => {
            label.is_some() && label_value.is_none()
        }
        (Operator::NotEqual, ConditionClauseValue::Json(serde_json::Value::Null)) => {
            label.is_none() || label_value.is_some()
        }
        (Operator::IsIn, ConditionClauseValue::Json(serde_json::Value::Array(values))) => {
            let Some(label_value) = label_value else {
                return false;
            };
            values
                .iter()
                .filter_map(|value| value.as_str())
                .any(|expected| expected == label_value)
        }
        _ => false,
    }
}

fn sort_lane_items(items: &mut [WorkItemView], item_order: SwimLaneItemOrder) {
    match item_order {
        SwimLaneItemOrder::UpdatedAsc => items.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.id.cmp(&right.id))
        }),
        SwimLaneItemOrder::CreatedDesc => items.sort_by(|left, right| {
            right
                .created_at
                .cmp(&left.created_at)
                .then_with(|| right.id.cmp(&left.id))
        }),
        SwimLaneItemOrder::CreatedAsc => items.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.id.cmp(&right.id))
        }),
        SwimLaneItemOrder::IdDesc => items.sort_by_key(|item| std::cmp::Reverse(item.id)),
        SwimLaneItemOrder::IdAsc => items.sort_by_key(|item| item.id),
        SwimLaneItemOrder::TitleAsc => items.sort_by(|left, right| {
            left.title
                .to_lowercase()
                .cmp(&right.title.to_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        }),
        SwimLaneItemOrder::TitleDesc => items.sort_by(|left, right| {
            right
                .title
                .to_lowercase()
                .cmp(&left.title.to_lowercase())
                .then_with(|| right.id.cmp(&left.id))
        }),
        SwimLaneItemOrder::UpdatedDesc => items.sort_by(|left, right| {
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.id.cmp(&left.id))
        }),
    }
}

fn item_card(project: String, item: WorkItemView) -> impl IntoView + 'static {
    let href = format!("/projects/{}/items/{}", encode_path(&project), item.id);
    let description = preview(&item.description);
    let claimed = item.claimed_by.is_some();
    let label_chips = item
        .labels
        .iter()
        .map(|label| {
            let blocked = label.key == AUTOMATION_BLOCKED_LABEL_KEY;
            let feedback_requested = label.key == FEEDBACK_REQUESTED_LABEL_KEY;
            let label = format_label(&label.key, label.value.as_deref());
            view! {
                <span
                    class="label-chip"
                    class:blocked=blocked
                    class:feedback=feedback_requested
                >
                    {label}
                </span>
            }
        })
        .collect::<Vec<_>>();
    let claim = item.claimed_by.clone().map(|agent| {
        let status = if item.state.as_deref() == Some("in_progress") {
            "In progress"
        } else {
            "Claimed"
        };
        claim_badge_with_source(
            &project,
            agent,
            status,
            item.claimed_at.clone(),
            item.claim_source.clone(),
        )
    });

    view! {
        <article class="card" class:claimed=claimed>
            <a href=href>
                <h3>{item.title}</h3>
            </a>
            <p>{description}</p>
            <div class="card-labels">{label_chips}</div>
            <footer>
                <span>"v" {item.version}</span>
                <span>{item.comment_count} " comments"</span>
                {claim}
                <span>{item.updated_at}</span>
            </footer>
        </article>
    }
}
