use crate::{
    frontend::{
        components::{
            ActivePage, TopBar, TopBarAutomation, WorkItemStatesContext, cached_query,
            claim_elapsed_timer, claim_source_label, encode_path, format_label, preview,
            provide_work_item_states_context, run_status_class, selected_project_signal,
            workspace_dock_height,
        },
        crudkit::work_items_crudkit_config_for_view,
        live_events::{
            board_items_event_matches, item_event_matches, refetch_on_live_event,
            run_log_event_matches,
        },
        pages::{ItemDetailContent, ItemPage, RunLogContent, RunLogPage, infer_dispatch_run_id},
        services::{board_service, item_service, project_cache, run_service},
        work_item_creation::{
            CreateItemOpenRequest, CreateItemStateOption, default_state_identifier,
            state_identifier_from_lane_filter, state_options_for_open_request,
            state_options_from_project_states,
        },
    },
    shared::view_models::{
        AUTOMATION_BLOCKED_LABEL_KEY, AgentRunStatus, AutomationStatusView,
        CodexAppServerStatusView, FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView, ProjectView,
        SwimLaneItemOrder, SwimLaneView, UiEvent, WorkItemStateView, WorkItemView, WorkspaceMode,
    },
};
use crudkit_leptos::{
    crud_instance::CrudInstanceContext,
    crud_instance_config::{CrudActionsPlacement, CrudBuiltinViewControls},
    crud_instance_mgr::CrudInstanceMgr,
    crudkit_core::condition::{
        Condition, ConditionClause, ConditionClauseValue, ConditionElement, Operator,
    },
    crudkit_web::view::CrudView,
    prelude::*,
};
use leptonic::components::prelude::{
    Drawer, DrawerSide, Icon, Modal, ModalBody, ModalFooter, ModalHeader, ModalTitle, Toasts,
};
#[cfg(not(feature = "ssr"))]
use leptonic::components::prelude::{Toast, ToastTimeout, ToastVariant};
use leptonic::prelude::icondata;
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::{
    NavigateOptions,
    hooks::{use_navigate, use_query_map},
};
use leptos_use::{use_interval_fn, use_media_query};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[cfg(not(feature = "ssr"))]
use time::OffsetDateTime;
#[cfg(not(feature = "ssr"))]
use uuid::Uuid;

const BOARD_ITEMS_REFRESH_INTERVAL_MS: u64 = 30_000;
const BOARD_DRAWER_DEFAULT_WIDTH_PERCENT: f64 = 46.0;
const BOARD_DRAWER_MIN_WIDTH_PERCENT: f64 = 30.0;
const BOARD_DRAWER_MAX_WIDTH_PERCENT: f64 = 70.0;
const BOARD_LANE_GAP_PX: f64 = 8.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BoardDrawerSelection {
    item_id: i64,
    run_id: Option<i64>,
}

#[derive(Clone, Debug)]
enum BoardDrawerAction {
    Select(Option<BoardDrawerSelection>),
    OpenFullPage(String),
}

#[derive(Clone, Debug)]
enum BoardDrawerData {
    Item(ItemPage),
    Run {
        item: ItemPage,
        run: Box<RunLogPage>,
    },
}

struct BoardDrawerControls {
    set_item_editor_context: WriteSignal<Option<CrudInstanceContext>>,
    attempt_action: Callback<(BoardDrawerAction, Option<Callback<()>>)>,
    open_drawer: Callback<BoardDrawerSelection>,
    close_drawer: Callback<()>,
    on_item_editor_return: Callback<()>,
    refresh: Callback<()>,
}

fn parse_board_drawer_selection(
    item: Option<&str>,
    run: Option<&str>,
) -> Result<Option<BoardDrawerSelection>, &'static str> {
    let Some(item) = item else {
        return if run.is_some() {
            Err("A run drawer requires an item id.")
        } else {
            Ok(None)
        };
    };
    let item_id = item
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)
        .ok_or("The Board drawer item id is invalid.")?;
    let run_id = run
        .map(|run| {
            run.parse::<i64>()
                .ok()
                .filter(|id| *id > 0)
                .ok_or("The Board drawer run id is invalid.")
        })
        .transpose()?;
    Ok(Some(BoardDrawerSelection { item_id, run_id }))
}

fn board_drawer_href(project: &str, selection: Option<BoardDrawerSelection>) -> String {
    let base = format!("/?project={}", encode_path(project));
    match selection {
        Some(BoardDrawerSelection {
            item_id,
            run_id: Some(run_id),
        }) => format!("{base}&item={item_id}&run={run_id}"),
        Some(BoardDrawerSelection {
            item_id,
            run_id: None,
        }) => format!("{base}&item={item_id}"),
        None => base,
    }
}

fn board_drawer_needs_navigation(
    current: Option<BoardDrawerSelection>,
    requested: BoardDrawerSelection,
) -> bool {
    current != Some(requested)
}

fn board_drawer_event_matches(
    event: &UiEvent,
    project: &str,
    selection: BoardDrawerSelection,
    item_deletion_requested: bool,
) -> bool {
    let selected_item_changed = matches!(
        event,
        UiEvent::WorkItemChanged {
            project: changed_project,
            item_id,
            ..
        } if changed_project == project && *item_id == selection.item_id
    );
    if item_deletion_requested && selected_item_changed {
        return false;
    }

    item_event_matches(event, Some(project), Some(selection.item_id))
        || selection
            .run_id
            .is_some_and(|run_id| run_log_event_matches(event, Some(project), Some(run_id)))
}

fn is_unmodified_primary_click(
    button: i16,
    ctrl: bool,
    meta: bool,
    shift: bool,
    alt: bool,
) -> bool {
    button == 0 && !ctrl && !meta && !shift && !alt
}

fn intercepts_board_drawer_click(event: &leptos::ev::MouseEvent) -> bool {
    is_unmodified_primary_click(
        event.button(),
        event.ctrl_key(),
        event.meta_key(),
        event.shift_key(),
        event.alt_key(),
    )
}

fn clamp_board_drawer_width_percent(width: f64) -> f64 {
    width.clamp(
        BOARD_DRAWER_MIN_WIDTH_PERCENT,
        BOARD_DRAWER_MAX_WIDTH_PERCENT,
    )
}

fn board_drawer_width_from_pointer(
    pointer_x: f64,
    layout_left: f64,
    layout_width: f64,
) -> Option<f64> {
    if !layout_width.is_finite() || layout_width <= 0.0 {
        return None;
    }
    let layout_right = layout_left + layout_width;
    Some(clamp_board_drawer_width_percent(
        (layout_right - pointer_x) / layout_width * 100.0,
    ))
}

#[cfg(target_arch = "wasm32")]
fn set_board_drawer_pointer_capture(event: &leptos::ev::PointerEvent, capture: bool) {
    use wasm_bindgen::JsCast;

    let Some(target) = event.current_target() else {
        return;
    };
    let Ok(element) = target.dyn_into::<web_sys::Element>() else {
        return;
    };
    if capture {
        let _ = element.set_pointer_capture(event.pointer_id());
    } else {
        let _ = element.release_pointer_capture(event.pointer_id());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn set_board_drawer_pointer_capture(_event: &leptos::ev::PointerEvent, _capture: bool) {}

#[cfg(target_arch = "wasm32")]
fn browser_history_back() -> bool {
    web_sys::window()
        .and_then(|window| window.history().ok())
        .is_some_and(|history| history.back().is_ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_history_back() -> bool {
    false
}

#[cfg(target_arch = "wasm32")]
fn browser_history_forward() -> bool {
    web_sys::window()
        .and_then(|window| window.history().ok())
        .is_some_and(|history| history.forward().is_ok())
}

#[cfg(not(target_arch = "wasm32"))]
fn browser_history_forward() -> bool {
    false
}

#[cfg(not(feature = "ssr"))]
fn show_board_drawer_error(toasts: Option<&Toasts>, message: String) {
    if let Some(toasts) = toasts {
        toasts.push(Toast {
            id: Uuid::new_v4(),
            created_at: OffsetDateTime::now_utc(),
            variant: ToastVariant::Error,
            header: ViewFn::from(|| "Drawer unavailable"),
            body: ViewFn::from(move || message.clone()),
            timeout: ToastTimeout::DefaultDelay,
        });
    }
}

#[cfg(feature = "ssr")]
fn show_board_drawer_error(_toasts: Option<&Toasts>, _message: String) {}

#[cfg(target_arch = "wasm32")]
fn focus_board_item(item_id: i64) {
    use wasm_bindgen::JsCast;

    request_animation_frame(move || {
        let Some(document) = web_sys::window().and_then(|window| window.document()) else {
            return;
        };
        let selector = format!("[data-board-item-id=\"{item_id}\"]");
        if let Ok(Some(element)) = document.query_selector(&selector)
            && let Ok(element) = element.dyn_into::<web_sys::HtmlElement>()
        {
            let _ = element.focus();
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
fn focus_board_item(_item_id: i64) {}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub selected_project: Option<String>,
    pub selected_project_view: Option<ProjectView>,
    pub automation_status: Option<AutomationStatusView>,
    pub automation_running: bool,
    pub items: Vec<BoardItemView>,
    pub swim_lanes: Vec<SwimLaneView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub label_suggestions: Vec<ProjectLabelView>,
    pub misconfigured_item_count: i64,
    pub api_base_url: String,
    pub codex_status: CodexAppServerStatusView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardItemsSection {
    pub items: Vec<BoardItemView>,
    pub swim_lanes: Vec<SwimLaneView>,
    pub work_item_states: Vec<WorkItemStateView>,
    pub misconfigured_item_count: i64,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardItemView {
    pub item: WorkItemView,
    pub run_count: usize,
    pub recent_runs: Vec<BoardRunPreview>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BoardRunPreview {
    pub id: i64,
    pub status: AgentRunStatus,
    pub result_summary: String,
    pub created_at: String,
}

#[component]
pub fn PageBoard() -> impl IntoView {
    let dock_height = workspace_dock_height();
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
        .and_then(|page| {
            page.selected_project_view
                .map(|project| project.auto_commit)
        })
        .or_else(|| {
            let selected = selected_project.get_untracked();
            cache.projects().with_untracked(|projects| {
                selected
                    .as_ref()
                    .and_then(|selected| projects.iter().find(|project| project.name == *selected))
                    .map(|project| project.auto_commit)
            })
        })
        .unwrap_or(false);
    let (auto_commit, set_auto_commit) = signal(initial_auto_commit);
    Effect::new(move |_| {
        let page_setting = result.value.get().and_then(|page| {
            page.selected_project_view
                .map(|project| project.auto_commit)
        });
        let cached_setting = cache.projects().with(|projects| {
            let selected = selected_project.get();
            selected
                .as_ref()
                .and_then(|selected| projects.iter().find(|project| project.name == *selected))
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
            .or_else(|| selected_project.get())?;
        let cached_project = cache
            .projects()
            .get()
            .into_iter()
            .find(|candidate| candidate.name == project);
        let workspace_mode = page
            .as_ref()
            .and_then(|page| page.selected_project_view.as_ref())
            .map(|project| project.workspace_mode)
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
    let topbar = view! {
        <TopBar
            active_project_names
            selected_project=selected_project.into()
            active=ActivePage::Board
            automation
            codex_status
        />
    };
    view! {
        <Title text="Dispatch"/>
        <div
            class="board-page"
            style=move || dock_height
                .get()
                .map(|height| format!("--workspace-dock-height: {height}px;"))
                .unwrap_or_default()
        >
            {topbar}
            <main class="page-shell">
                <For
                    each=move || result.value.get()
                    key=|page| page.selected_project.clone()
                    children=|page| view! { <BoardContent page/> }
                />
            </main>
        </div>
    }
}

#[component]
fn BoardContent(page: BoardPage) -> impl IntoView {
    let BoardPage {
        projects: _,
        active_project_names: _,
        selected_project,
        selected_project_view,
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

    if let (Some(project), Some(project_view)) = (selected_project.clone(), selected_project_view) {
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
        let create_item = view! {
            <CreateItemModal
                api_base_url=api_base_url.clone()
                project_id=admin_project_id
                show_when=show_create_item_modal
                set_show_when=set_show_create_item_modal
                state_options=create_item_state_options
                selected_state=create_item_state
                label_suggestions=create_item_label_suggestions
            />
        };
        view! {
            <>
                    {board}
                    {create_item}
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
    initial_items: Vec<BoardItemView>,
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

    let query = use_query_map();
    let initial_query = query.read_untracked();
    let initial_selection = parse_board_drawer_selection(
        initial_query.get("item").as_deref(),
        initial_query.get("run").as_deref(),
    )
    .unwrap_or(None);
    let (drawer_selection, set_drawer_selection) = signal(initial_selection);
    let (item_editor_context, set_item_editor_context) = signal(None::<CrudInstanceContext>);
    let drawer_was_pushed = RwSignal::new(false);
    let navigate = use_navigate();

    let project_for_execute = project.clone();
    let navigate_for_execute = navigate.clone();
    let execute_drawer_action = Callback::new(move |action: BoardDrawerAction| match action {
        BoardDrawerAction::Select(selection) => {
            set_item_editor_context.set(None);
            set_drawer_selection.set(selection);
        }
        BoardDrawerAction::OpenFullPage(href) => {
            navigate_for_execute(
                &href,
                NavigateOptions {
                    scroll: false,
                    ..NavigateOptions::default()
                },
            );
        }
    });

    let attempt_drawer_action = Callback::new(
        move |(action, on_cancel): (BoardDrawerAction, Option<Callback<()>>)| {
            if let Some(context) = item_editor_context.get_untracked() {
                let action_on_approved = action.clone();
                context.navigation.attempt(
                    move || execute_drawer_action.run(action_on_approved.clone()),
                    move || {
                        if let Some(on_cancel) = on_cancel {
                            on_cancel.run(());
                        }
                    },
                );
            } else {
                execute_drawer_action.run(action);
            }
        },
    );

    let navigate_for_open = navigate.clone();
    let project_for_open = project.clone();
    let open_drawer = Callback::new(move |selection: BoardDrawerSelection| {
        let current = drawer_selection.get_untracked();
        if !board_drawer_needs_navigation(current, selection) {
            return;
        }

        let replace = current.is_some();
        if !replace {
            drawer_was_pushed.set(true);
        }
        navigate_for_open(
            &board_drawer_href(&project_for_open, Some(selection)),
            NavigateOptions {
                replace,
                scroll: false,
                ..NavigateOptions::default()
            },
        );
    });

    let navigate_for_query_cancel = navigate.clone();
    let project_for_query = project.clone();
    let toasts = use_context::<Toasts>();
    Effect::new(move |_| {
        let query = query.read();
        let requested = match parse_board_drawer_selection(
            query.get("item").as_deref(),
            query.get("run").as_deref(),
        ) {
            Ok(selection) => selection,
            Err(message) => {
                show_board_drawer_error(toasts.as_ref(), message.to_owned());
                navigate_for_query_cancel(
                    &board_drawer_href(&project_for_query, None),
                    NavigateOptions {
                        replace: true,
                        scroll: false,
                        ..NavigateOptions::default()
                    },
                );
                return;
            }
        };
        let current = drawer_selection.get_untracked();
        if requested == current {
            return;
        }

        let restore_href = board_drawer_href(&project_for_query, current);
        let requested_is_close = requested.is_none();
        let navigate_for_cancel = navigate_for_query_cancel.clone();
        let on_cancel = Callback::new(move |()| {
            if !(requested_is_close
                && drawer_was_pushed.get_untracked()
                && browser_history_forward())
            {
                navigate_for_cancel(
                    &restore_href,
                    NavigateOptions {
                        replace: true,
                        scroll: false,
                        ..NavigateOptions::default()
                    },
                );
            }
        });
        attempt_drawer_action.run((BoardDrawerAction::Select(requested), Some(on_cancel)));
    });

    let navigate_for_exit = navigate.clone();
    let project_for_exit = project.clone();
    let on_item_editor_return = Callback::new(move |()| {
        set_item_editor_context.set(None);
        set_drawer_selection.set(None);
        navigate_for_exit(
            &board_drawer_href(&project_for_exit, None),
            NavigateOptions {
                replace: true,
                scroll: false,
                ..NavigateOptions::default()
            },
        );
    });

    let navigate_for_close = navigate.clone();
    let project_for_close = project.clone();
    let close_drawer = Callback::new(move |()| {
        if !(drawer_was_pushed.get_untracked() && browser_history_back()) {
            navigate_for_close(
                &board_drawer_href(&project_for_close, None),
                NavigateOptions {
                    replace: true,
                    scroll: false,
                    ..NavigateOptions::default()
                },
            );
        }
    });

    let navigate_for_invalid = navigate.clone();
    let project_for_invalid = project.clone();
    let on_invalid_drawer = Callback::new(move |message: String| {
        show_board_drawer_error(toasts.as_ref(), message);
        set_item_editor_context.set(None);
        set_drawer_selection.set(None);
        navigate_for_invalid(
            &board_drawer_href(&project_for_invalid, None),
            NavigateOptions {
                replace: true,
                scroll: false,
                ..NavigateOptions::default()
            },
        );
    });

    let board_inspector_layout = NodeRef::<leptos::html::Div>::new();
    let drawer_width_percent = RwSignal::new(BOARD_DRAWER_DEFAULT_WIDTH_PERCENT);
    let drawer_resizing = RwSignal::new(false);

    view! {
        <div
            class="board-inspector-layout"
            class:resizing=move || drawer_resizing.get()
            node_ref=board_inspector_layout
            style=move || format!(
                "--board-inspector-width: {:.3}%;",
                drawer_width_percent.get(),
            )
        >
            <div class="board-inspector-board">
                {move || {
                    view! {
                        <BoardView
                            project=project.clone()
                            items=items.get()
                            swim_lanes=swim_lanes.get()
                            work_item_states=work_item_states.get()
                            misconfigured_item_count=misconfigured_item_count.get()
                            open_create_item
                            open_drawer
                        />
                    }
                }}
            </div>
            <div class="board-inspector-slot">
                <BoardInspectorDrawer
                    project=project_for_execute
                    selection=drawer_selection
                    item_editor_context
                    set_item_editor_context=set_item_editor_context
                    attempt_action=attempt_drawer_action
                    open_drawer=open_drawer
                    close_drawer=close_drawer
                    on_item_editor_return=on_item_editor_return
                    on_invalid=on_invalid_drawer
                    layout_ref=board_inspector_layout
                    drawer_width_percent=drawer_width_percent
                    drawer_resizing=drawer_resizing
                />
            </div>
        </div>
    }
}

#[component]
fn BoardInspectorDrawer(
    project: String,
    selection: ReadSignal<Option<BoardDrawerSelection>>,
    item_editor_context: ReadSignal<Option<CrudInstanceContext>>,
    set_item_editor_context: WriteSignal<Option<CrudInstanceContext>>,
    attempt_action: Callback<(BoardDrawerAction, Option<Callback<()>>)>,
    open_drawer: Callback<BoardDrawerSelection>,
    close_drawer: Callback<()>,
    on_item_editor_return: Callback<()>,
    on_invalid: Callback<String>,
    layout_ref: NodeRef<leptos::html::Div>,
    drawer_width_percent: RwSignal<f64>,
    drawer_resizing: RwSignal<bool>,
) -> impl IntoView + 'static {
    #[cfg(feature = "ssr")]
    let _ = on_invalid;
    let item_service = item_service();
    let run_service = run_service();
    let project_for_resource = project.clone();
    let drawer_data = LocalResource::new(move || {
        let project = project_for_resource.clone();
        let selection = selection.get();
        let item_service = item_service.clone();
        let run_service = run_service.clone();
        async move {
            let Some(selection) = selection else {
                return Ok(None);
            };
            let item = item_service
                .load_page(Some(project.clone()), Some(selection.item_id))
                .await?;
            match selection.run_id {
                Some(run_id) => {
                    let run = run_service.load_log(Some(project), Some(run_id)).await?;
                    if run.run_log.run.work_item_id != Some(selection.item_id) {
                        return Err(ServerFnError::new(format!(
                            "Run #{run_id} is not linked to item #{}.",
                            selection.item_id
                        )));
                    }
                    Ok(Some(BoardDrawerData::Run {
                        item,
                        run: Box::new(run),
                    }))
                }
                None => Ok(Some(BoardDrawerData::Item(item))),
            }
        }
    });

    let drawer_refresh = Callback::new(move |()| drawer_data.refetch());
    let project_for_events = project.clone();
    refetch_on_live_event(drawer_refresh, move |event| {
        let Some(selection) = selection.get_untracked() else {
            return false;
        };
        let item_deletion_requested = item_editor_context
            .get_untracked()
            .is_some_and(|context| context.deletion_request.get_untracked().is_some());
        board_drawer_event_matches(
            event,
            project_for_events.as_str(),
            selection,
            item_deletion_requested,
        )
    });

    #[cfg(not(feature = "ssr"))]
    {
        let last_failed_selection = RwSignal::new(None::<BoardDrawerSelection>);
        Effect::new(move |_| {
            let failure = drawer_data
                .map(|result| result.as_ref().err().map(ToString::to_string))
                .flatten();
            let Some(message) = failure else {
                last_failed_selection.set(None);
                return;
            };
            let Some(failed_selection) = selection.get_untracked() else {
                return;
            };
            if last_failed_selection.get_untracked() == Some(failed_selection) {
                return;
            }
            last_failed_selection.set(Some(failed_selection));
            on_invalid.run(message);
        });
    }

    let drawer_focus = NodeRef::<leptos::html::Div>::new();
    let last_focused_item = RwSignal::new(selection.get_untracked().map(|value| value.item_id));
    Effect::new(move |_| match selection.get() {
        Some(selection) => {
            last_focused_item.set(Some(selection.item_id));
            if let Some(element) = drawer_focus.get() {
                let _ = element.focus();
            }
        }
        None => {
            if let Some(item_id) = last_focused_item.get_untracked() {
                focus_board_item(item_id);
            }
        }
    });

    let shown = Signal::derive(move || selection.get().is_some());
    let narrow = use_media_query("(max-width: 899px)");
    let resize_from_pointer = Callback::new(move |pointer_x: f64| {
        let Some(layout) = layout_ref.get() else {
            return;
        };
        let bounds = layout.get_bounding_client_rect();
        if let Some(width) =
            board_drawer_width_from_pointer(pointer_x, bounds.left(), bounds.width())
        {
            drawer_width_percent.set(width);
        }
    });
    let close_on_backdrop = close_drawer;
    let close_on_escape = close_drawer;
    view! {
        <button
            type="button"
            class="board-drawer-backdrop"
            class:shown=move || shown.get()
            aria-label="Close item drawer"
            tabindex="-1"
            on:click=move |_| close_on_backdrop.run(())
        ></button>
        <Drawer
            side=DrawerSide::Right
            shown=shown
            attr:data-board-inspector="true"
            attr:role="dialog"
            attr:aria-modal=move || narrow.get().to_string()
            attr:aria-label="Board item inspector"
        >
            <button
                type="button"
                class="board-drawer-resize-handle"
                class:dragging=move || drawer_resizing.get()
                role="separator"
                aria-label="Resize details drawer"
                aria-orientation="vertical"
                aria-valuemin=BOARD_DRAWER_MIN_WIDTH_PERCENT.to_string()
                aria-valuemax=BOARD_DRAWER_MAX_WIDTH_PERCENT.to_string()
                aria-valuenow=move || format!("{:.0}", drawer_width_percent.get())
                title="Drag to resize details drawer"
                on:pointerdown=move |event| {
                    if event.button() != 0 || narrow.get_untracked() {
                        return;
                    }
                    event.prevent_default();
                    drawer_resizing.set(true);
                    resize_from_pointer.run(event.client_x() as f64);
                    set_board_drawer_pointer_capture(&event, true);
                }
                on:pointermove=move |event| {
                    if drawer_resizing.get_untracked() {
                        event.prevent_default();
                        resize_from_pointer.run(event.client_x() as f64);
                    }
                }
                on:pointerup=move |event| {
                    if drawer_resizing.get_untracked() {
                        event.prevent_default();
                        resize_from_pointer.run(event.client_x() as f64);
                        drawer_resizing.set(false);
                        set_board_drawer_pointer_capture(&event, false);
                    }
                }
                on:pointercancel=move |event| {
                    drawer_resizing.set(false);
                    set_board_drawer_pointer_capture(&event, false);
                }
                on:keydown=move |event| {
                    let next = match event.key().as_str() {
                        "ArrowLeft" => Some(drawer_width_percent.get_untracked() + 2.0),
                        "ArrowRight" => Some(drawer_width_percent.get_untracked() - 2.0),
                        "Home" => Some(BOARD_DRAWER_MIN_WIDTH_PERCENT),
                        "End" => Some(BOARD_DRAWER_MAX_WIDTH_PERCENT),
                        _ => None,
                    };
                    if let Some(next) = next {
                        event.prevent_default();
                        drawer_width_percent.set(clamp_board_drawer_width_percent(next));
                    }
                }
            ></button>
            <div
                class="board-drawer-focus"
                tabindex="-1"
                node_ref=drawer_focus
                on:keydown=move |event| {
                    if event.key() == "Escape" && !event.default_prevented() {
                        event.prevent_default();
                        close_on_escape.run(());
                    }
                }
            >
                <Suspense fallback=move || view! {
                    <div class="board-drawer-loading" role="status">"Loading details…"</div>
                }>
                    {move || {
                        drawer_data.get().map(|result| match result {
                            Ok(Some(data)) => board_drawer_data_view(
                                project.clone(),
                                data,
                                BoardDrawerControls {
                                    set_item_editor_context,
                                    attempt_action,
                                    open_drawer,
                                    close_drawer,
                                    on_item_editor_return,
                                    refresh: drawer_refresh,
                                },
                            ),
                            Ok(None) => ().into_any(),
                            Err(error) => view! {
                                <p class="board-drawer-error" role="alert">{error.to_string()}</p>
                            }
                            .into_any(),
                        })
                    }}
                </Suspense>
            </div>
        </Drawer>
    }
}

fn board_drawer_data_view(
    project: String,
    data: BoardDrawerData,
    controls: BoardDrawerControls,
) -> AnyView {
    let BoardDrawerControls {
        set_item_editor_context,
        attempt_action,
        open_drawer,
        close_drawer,
        on_item_editor_return,
        refresh,
    } = controls;
    match data {
        BoardDrawerData::Item(page) => {
            let item_id = page.item.id;
            let title = format!("#{} {}", item_id, page.item.title);
            let full_href = format!("/projects/{}/items/{item_id}", encode_path(&project));
            let full_href_for_click = full_href.clone();
            let on_run_click = Callback::new(move |(event, run_id)| {
                if intercepts_board_drawer_click(&event) {
                    event.prevent_default();
                    open_drawer.run(BoardDrawerSelection {
                        item_id,
                        run_id: Some(run_id),
                    });
                }
            });
            let (interactive, _) = signal(true);
            let item_detail = view! {
                <ItemDetailContent
                    page
                    refresh
                    interactive
                    on_return=on_item_editor_return
                    on_context_created=Callback::new(move |context| {
                        set_item_editor_context.set(Some(context));
                    })
                    on_run_click=Some(on_run_click)
                />
            };
            view! {
                <header class="board-drawer-header item-drawer-header">
                    <a
                        class="board-drawer-title"
                        href=full_href
                        title="Open full item"
                        on:click=move |event| {
                            if intercepts_board_drawer_click(&event) {
                                event.prevent_default();
                                attempt_action.run((
                                    BoardDrawerAction::OpenFullPage(full_href_for_click.clone()),
                                    None,
                                ));
                            }
                        }
                    >
                        <h1>{title}</h1>
                    </a>
                    <button
                        type="button"
                        class="secondary icon-button board-drawer-close"
                        title="Close"
                        aria-label="Close"
                        on:click=move |_| close_drawer.run(())
                    >
                        <Icon icon=icondata::BsX/>
                    </button>
                </header>
                <div class="board-drawer-body item-page">{item_detail}</div>
            }
            .into_any()
        }
        BoardDrawerData::Run { item, run } => {
            let item_id = item.item.id;
            let run_id = run.run_log.run.id;
            let full_href = format!(
                "/projects/{}/automation/runs/{run_id}/log",
                encode_path(&project)
            );
            let show_thinking_history = RwSignal::new(false);
            let toggle_thinking_history = Callback::new(move |()| {
                show_thinking_history.update(|show| *show = !*show);
            });
            let back_to_item = open_drawer;
            let run_for_detail = StoredValue::new(*run);
            view! {
                <header class="board-drawer-header run-drawer-header">
                    <div>
                        <button
                            type="button"
                            class="link-button board-drawer-back"
                            on:click=move |_| {
                                back_to_item.run(BoardDrawerSelection {
                                    item_id,
                                    run_id: None,
                                })
                            }
                        >
                            "Back to item"
                        </button>
                        <h1>"Run #" {run_id}</h1>
                    </div>
                    <div class="board-drawer-header-actions">
                        <a class="secondary-link" href=full_href>"Open full run"</a>
                        <button
                            type="button"
                            class="secondary icon-button board-drawer-close"
                            title="Close"
                            aria-label="Close"
                            on:click=move |_| close_drawer.run(())
                        >
                            <Icon icon=icondata::BsX/>
                        </button>
                    </div>
                </header>
                <div class="board-drawer-body run-log">
                    {move || view! {
                        <RunLogContent
                            page=run_for_detail.get_value()
                            show_thinking_history=show_thinking_history.get()
                            toggle_thinking_history
                        />
                    }}
                </div>
            }
            .into_any()
        }
    }
}

#[component]
fn CreateItemModal(
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
    let attempt_close = Callback::new(move |()| {
        if let Some(context) = context.get_untracked() {
            context.navigation.return_from_current();
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
    let attempt_close_on_escape = attempt_close;
    let attempt_close_on_backdrop = attempt_close;
    let attempt_close_on_header = attempt_close;
    let attempt_close_on_footer = attempt_close;
    view! {
        <Modal
            id="new-item-modal"
            class="new-item-modal"
            show_when=show_when
            on_escape=move || attempt_close_on_escape.run(())
            on_backdrop_interaction=move || attempt_close_on_backdrop.run(())
        >
            <ModalHeader>
                <ModalTitle>"New item"</ModalTitle>
                <button
                    type="button"
                    class="secondary icon-button modal-close-button"
                    title="Close"
                    aria-label="Close"
                    on:click=move |_| attempt_close_on_header.run(())
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
                    view! {
                        <div class="new-item-controls crudkit-new-item" data-crudkit-leptos="work-item-create">
                            <CrudInstanceMgr>
                                <CrudInstance
                                    name="work-item-create"
                                    config=work_items_crudkit_config_for_view(
                                        api_base_url.clone(),
                                        project_id,
                                        CrudView::create(),
                                        CrudBuiltinViewControls::embedded_single_entity()
                                            .with_create_actions_placement(CrudActionsPlacement::External),
                                        default_create_state,
                                        Some(crud_state_options),
                                        label_suggestions,
                                    )
                                    on_context_created=Callback::new(move |context: CrudInstanceContext| {
                                        context
                                            .navigation
                                            .return_with(move || close_modal_for_exit.run(()));
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
                    on:click=move |_| attempt_close_on_footer.run(())
                >
                    "Cancel"
                </button>
                <CrudActionsOutlet context=context action_slot=CrudActionSlot::CreatePrimary />
            </ModalFooter>
        </Modal>
    }
}

#[component]
fn BoardView(
    project: String,
    items: Vec<BoardItemView>,
    swim_lanes: Vec<SwimLaneView>,
    work_item_states: Vec<WorkItemStateView>,
    misconfigured_item_count: i64,
    open_create_item: Callback<CreateItemOpenRequest>,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    let _ = work_item_states;
    let lane_count = swim_lanes.len().max(1) as f64;
    let lane_width = format!(
        "--board-lane-width: calc({:.6}cqw - {:.3}px);",
        100.0 / lane_count,
        BOARD_LANE_GAP_PX * (lane_count - 1.0) / lane_count,
    );
    let lanes = swim_lanes
        .into_iter()
        .map(|lane| {
            let label = lane.name.clone();
            let mut lane_items = items
                .iter()
                .filter(|item| item_matches_condition(&item.item, &lane.filter))
                .cloned()
                .collect::<Vec<_>>();
            sort_lane_items(&mut lane_items, lane.item_order);
            let count = lane_items.len();
            let cards = lane_cards(project.clone(), lane_items, open_drawer);
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
            <section class="board" style=lane_width>{lanes}</section>
            {warning}
        </div>
    }
}

fn lane_cards(
    project: String,
    items: Vec<BoardItemView>,
    open_drawer: Callback<BoardDrawerSelection>,
) -> Vec<AnyView> {
    let mut rendered_groups = BTreeSet::new();
    let mut cards = Vec::new();
    for item in &items {
        let Some(group) = item.item.work_group.clone() else {
            cards.push(
                view! {
                    <ItemCard
                        project=project.clone()
                        board_item=item.clone()
                        open_drawer
                    />
                }
                .into_any(),
            );
            continue;
        };
        if !rendered_groups.insert(group.id) {
            continue;
        }
        let grouped_items = items
            .iter()
            .filter(|candidate| {
                candidate.item.work_group.as_ref().map(|group| group.id) == Some(group.id)
            })
            .cloned()
            .collect::<Vec<_>>();
        let group_count = grouped_items.len();
        let group_cards = grouped_items
            .into_iter()
            .map(|item| {
                view! {
                    <ItemCard
                        project=project.clone()
                        board_item=item
                        open_drawer
                    />
                }
            })
            .collect::<Vec<_>>();
        cards.push(
            view! {
                <section class="work-item-card-group" data-work-group-key=group.key.clone()>
                    <header>
                        <div>
                            <strong>{group.name}</strong>
                            <code>{group.key.clone()}</code>
                        </div>
                        <span>{format!("{group_count} in this lane")}</span>
                    </header>
                    <div class="work-item-card-group-items">{group_cards}</div>
                </section>
            }
            .into_any(),
        );
    }
    cards
}

fn lane_edit_href(project: &str, lane_id: i64) -> String {
    format!(
        "/project?project={}&edit_swim_lane={}#swim-lanes",
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

fn sort_lane_items(items: &mut [BoardItemView], item_order: SwimLaneItemOrder) {
    match item_order {
        SwimLaneItemOrder::UpdatedAsc => items.sort_by(|left, right| {
            left.item
                .updated_at
                .cmp(&right.item.updated_at)
                .then_with(|| left.item.id.cmp(&right.item.id))
        }),
        SwimLaneItemOrder::CreatedDesc => items.sort_by(|left, right| {
            right
                .item
                .created_at
                .cmp(&left.item.created_at)
                .then_with(|| right.item.id.cmp(&left.item.id))
        }),
        SwimLaneItemOrder::CreatedAsc => items.sort_by(|left, right| {
            left.item
                .created_at
                .cmp(&right.item.created_at)
                .then_with(|| left.item.id.cmp(&right.item.id))
        }),
        SwimLaneItemOrder::IdDesc => items.sort_by_key(|item| std::cmp::Reverse(item.item.id)),
        SwimLaneItemOrder::IdAsc => items.sort_by_key(|item| item.item.id),
        SwimLaneItemOrder::TitleAsc => items.sort_by(|left, right| {
            left.item
                .title
                .to_lowercase()
                .cmp(&right.item.title.to_lowercase())
                .then_with(|| left.item.id.cmp(&right.item.id))
        }),
        SwimLaneItemOrder::TitleDesc => items.sort_by(|left, right| {
            right
                .item
                .title
                .to_lowercase()
                .cmp(&left.item.title.to_lowercase())
                .then_with(|| right.item.id.cmp(&left.item.id))
        }),
        SwimLaneItemOrder::UpdatedDesc => items.sort_by(|left, right| {
            right
                .item
                .updated_at
                .cmp(&left.item.updated_at)
                .then_with(|| right.item.id.cmp(&left.item.id))
        }),
    }
}

#[component]
fn ItemCard(
    project: String,
    board_item: BoardItemView,
    open_drawer: Callback<BoardDrawerSelection>,
) -> impl IntoView + 'static {
    let BoardItemView {
        item,
        run_count,
        recent_runs,
    } = board_item;
    let item_id = item.id;
    let href = format!("/projects/{}/items/{}", encode_path(&project), item.id);
    let description = preview(&item.description);
    let claimed = item.claimed_by.is_some();
    let active_claim_run_id = item
        .claim_source
        .as_ref()
        .map(|source| source.run_id)
        .or_else(|| item.claimed_by.as_deref().and_then(infer_dispatch_run_id));
    let active_claim_source = claim_source_label(item.claim_source.as_ref());
    let active_claimed_at = item.claimed_at.clone();
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
    let run_links = recent_runs
        .into_iter()
        .map(|run| {
            let run_id = run.id;
            let href = format!(
                "/projects/{}/automation/runs/{run_id}/log",
                encode_path(&project)
            );
            let status = run.status.to_string();
            let status_class = run_status_class(run.status);
            let summary = (!run.result_summary.trim().is_empty()).then_some(run.result_summary);
            let claim_context = (active_claim_run_id == Some(run_id)
                && (active_claim_source.is_some() || active_claimed_at.is_some()))
            .then(|| {
                let source = active_claim_source.clone();
                let elapsed = claim_elapsed_timer(active_claimed_at.clone());
                view! {
                    <span class="card-run-context" title="Active claim">
                        {source.map(|source| view! {
                            <span class="card-run-source" title="Automation source">{source}</span>
                        })}
                        {elapsed}
                    </span>
                }
            });
            view! {
                <a
                    class=format!("card-run-preview {status_class}")
                    href=href
                    on:click=move |event| {
                        if intercepts_board_drawer_click(&event) {
                            event.prevent_default();
                            open_drawer.run(BoardDrawerSelection {
                                item_id,
                                run_id: Some(run_id),
                            });
                        }
                    }
                >
                    <strong>"#" {run_id}</strong>
                    <span class="card-run-status">{status}</span>
                    {summary.map(|summary| view! {
                        <span class="card-run-summary">{summary}</span>
                    })}
                    {claim_context}
                </a>
            }
        })
        .collect::<Vec<_>>();
    let runs = (run_count > 0).then(|| {
        let label = if run_count == 1 {
            "Run 1".to_owned()
        } else {
            format!("Runs {run_count}")
        };
        view! {
            <section class="card-runs" aria-label=label.clone()>
                <div class="card-run-count">{label.clone()}</div>
                <div class="card-run-previews">{run_links}</div>
            </section>
        }
    });

    view! {
        <article class="card" class:claimed=claimed>
            <a
                class="card-main-link"
                href=href
                data-board-item-id=item_id
                on:click=move |event| {
                    if intercepts_board_drawer_click(&event) {
                        event.prevent_default();
                        open_drawer.run(BoardDrawerSelection {
                            item_id,
                            run_id: None,
                        });
                    }
                }
            >
                <h3>{item.title}</h3>
                <p>{description}</p>
                <div class="card-labels">{label_chips}</div>
            </a>
            {runs}
            <footer>
                <span class="card-item-id">"#" {item_id}</span>
                <span>{item.comment_count} " comments"</span>
                <span>{item.updated_at}</span>
            </footer>
        </article>
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BOARD_DRAWER_MAX_WIDTH_PERCENT, BOARD_DRAWER_MIN_WIDTH_PERCENT, BoardDrawerSelection,
        board_drawer_event_matches, board_drawer_href, board_drawer_needs_navigation,
        board_drawer_width_from_pointer, is_unmodified_primary_click, parse_board_drawer_selection,
    };
    use assertr::prelude::*;
    use dispatch_types::UiEvent;

    #[test]
    fn parses_and_formats_board_drawer_query_state() {
        assert_that!(parse_board_drawer_selection(None, None)).is_equal_to(Ok(None));
        assert_that!(parse_board_drawer_selection(Some("42"), None)).is_equal_to(Ok(Some(
            BoardDrawerSelection {
                item_id: 42,
                run_id: None,
            },
        )));
        assert_that!(parse_board_drawer_selection(Some("42"), Some("17"))).is_equal_to(Ok(Some(
            BoardDrawerSelection {
                item_id: 42,
                run_id: Some(17),
            },
        )));
        assert_that!(parse_board_drawer_selection(None, Some("17")).is_err()).is_true();
        assert_that!(parse_board_drawer_selection(Some("0"), None).is_err()).is_true();
        assert_that!(parse_board_drawer_selection(Some("42"), Some("nope")).is_err()).is_true();

        assert_that!(board_drawer_href("demo", None)).is_equal_to("/?project=demo");
        assert_that!(board_drawer_href(
            "demo project",
            Some(BoardDrawerSelection {
                item_id: 42,
                run_id: Some(17),
            })
        ))
        .is_equal_to("/?project=demo%20project&item=42&run=17");
    }

    #[test]
    fn intercepts_only_unmodified_primary_clicks() {
        assert_that!(is_unmodified_primary_click(0, false, false, false, false)).is_true();
        assert_that!(is_unmodified_primary_click(1, false, false, false, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, true, false, false, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, false, true, false, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, false, false, true, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, false, false, false, true)).is_false();
    }

    #[test]
    fn reselecting_the_open_board_drawer_is_a_noop() {
        let item = BoardDrawerSelection {
            item_id: 42,
            run_id: None,
        };
        let run = BoardDrawerSelection {
            item_id: 42,
            run_id: Some(17),
        };

        assert_that!(board_drawer_needs_navigation(Some(item), item)).is_false();
        assert_that!(board_drawer_needs_navigation(Some(run), run)).is_false();
        assert_that!(board_drawer_needs_navigation(Some(run), item)).is_true();
        assert_that!(board_drawer_needs_navigation(None, item)).is_true();
    }

    #[test]
    fn confirmed_item_deletion_does_not_reload_the_deleted_drawer_item() {
        let selection = BoardDrawerSelection {
            item_id: 42,
            run_id: None,
        };
        let item_changed = UiEvent::WorkItemChanged {
            sequence: 1,
            timestamp: "2026-07-16T12:00:00Z".to_owned(),
            project: "demo".to_owned(),
            item_id: 42,
        };
        let comment_changed = UiEvent::CommentChanged {
            sequence: 2,
            timestamp: "2026-07-16T12:00:01Z".to_owned(),
            project: "demo".to_owned(),
            item_id: 42,
        };

        assert_that!(board_drawer_event_matches(
            &item_changed,
            "demo",
            selection,
            false,
        ))
        .is_true();
        assert_that!(board_drawer_event_matches(
            &item_changed,
            "demo",
            selection,
            true,
        ))
        .is_false();
        assert_that!(board_drawer_event_matches(
            &comment_changed,
            "demo",
            selection,
            true,
        ))
        .is_true();
    }

    #[test]
    fn drawer_resize_uses_the_layout_width_and_clamps_to_limits() {
        assert_that!(board_drawer_width_from_pointer(800.0, 0.0, 1_000.0))
            .is_equal_to(Some(BOARD_DRAWER_MIN_WIDTH_PERCENT));
        assert_that!(board_drawer_width_from_pointer(450.0, 100.0, 1_000.0))
            .is_equal_to(Some(65.0));
        assert_that!(board_drawer_width_from_pointer(0.0, 0.0, 1_000.0))
            .is_equal_to(Some(BOARD_DRAWER_MAX_WIDTH_PERCENT));
        assert_that!(board_drawer_width_from_pointer(0.0, 0.0, 0.0)).is_equal_to(None);
    }
}
