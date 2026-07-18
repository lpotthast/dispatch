use crate::{
    frontend::services::{automation_service, project_cache, project_service},
    shared::view_models::{CodexAppServerStatusView, WorkspaceMode},
};
use crudkit_leptos::crud_instance_mgr::CrudInstanceMgrContext;
use leptonic::components::prelude::{Select, Toggle};
use leptos::prelude::*;
use leptos_router::{NavigateOptions, hooks::use_navigate};

use super::encode_path;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ActivePage {
    Board,
    Project,
    Triggers,
    Runs,
    System,
    Projects,
    Api,
}

#[derive(Clone)]
pub(crate) struct TopBarAutomation {
    pub(crate) project: String,
    pub(crate) running: bool,
    pub(crate) workspace_mode: WorkspaceMode,
    pub(crate) auto_commit: ReadSignal<bool>,
    pub(crate) set_auto_commit: WriteSignal<bool>,
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectSelectOption {
    name: String,
    display_name: String,
    active: bool,
}

#[component]
pub(crate) fn TopBar(
    active_project_names: Signal<Vec<String>>,
    selected_project: Signal<Option<String>>,
    active: ActivePage,
    automation: Signal<Option<TopBarAutomation>>,
    codex_status: Signal<CodexAppServerStatusView>,
) -> impl IntoView + 'static {
    let attempt_link_navigation = use_link_navigation_attempt();
    let projects = project_cache().projects();
    let effective_selected_project = Memo::new(move |_| {
        selected_project.get().filter(|selected| {
            projects
                .get()
                .iter()
                .any(|project| project.name == *selected)
        })
    });
    let initial_auto_commit = projects.with_untracked(|projects| {
        effective_selected_project
            .get_untracked()
            .and_then(|selected| {
                projects
                    .iter()
                    .find(|project| project.name == selected)
                    .map(|project| project.auto_commit)
            })
            .unwrap_or(false)
    });
    let (cached_auto_commit, set_cached_auto_commit) = signal(initial_auto_commit);
    Effect::new(move |_| {
        let selected = effective_selected_project.get();
        if let Some(auto_commit) = projects.with(|projects| {
            selected.as_ref().and_then(|selected| {
                projects
                    .iter()
                    .find(|project| project.name == *selected)
                    .map(|project| project.auto_commit)
            })
        }) {
            set_cached_auto_commit.set(auto_commit);
        }
    });
    let selected_query = Memo::new(move |_| {
        effective_selected_project
            .get()
            .as_ref()
            .map(|project| format!("?project={}", encode_path(project)))
            .unwrap_or_default()
    });
    let board_href = Memo::new(move |_| {
        let selected_query = selected_query.get();
        if selected_query.is_empty() {
            "/".to_owned()
        } else {
            format!("/{selected_query}")
        }
    });
    let project_href = Memo::new(move |_| format!("/project{}", selected_query.get()));
    let triggers_href = Memo::new(move |_| format!("/automation{}", selected_query.get()));
    let runs_href = Memo::new(move |_| format!("/runs{}", selected_query.get()));
    let system_href = Memo::new(move |_| format!("/system{}", selected_query.get()));
    let projects_href = Memo::new(move |_| format!("/projects{}", selected_query.get()));
    let api_href = Memo::new(move |_| format!("/api/docs{}", selected_query.get()));
    let board_class = active_class(active, ActivePage::Board);
    let project_class = active_class(active, ActivePage::Project);
    let triggers_class = active_class(active, ActivePage::Triggers);
    let runs_class = active_class(active, ActivePage::Runs);
    let projects_class = active_class(active, ActivePage::Projects);
    let system_class = active_class(active, ActivePage::System);
    let api_class = active_class(active, ActivePage::Api);

    view! {
        <header class="app-topbar">
            <a
                class="brand"
                href=move || board_href.get()
                on:click=move |event| {
                    attempt_link_navigation.run((event, board_href.get_untracked()));
                }
            >
                <img
                    class="brand-icon"
                    src="/branding/dispatch-icon-64.png"
                    alt=""
                    aria-hidden="true"
                />
                "Dispatch"
            </a>
            <nav class="top-nav" aria-label="Primary">
                <a
                    class=board_class
                    href=move || board_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, board_href.get_untracked()));
                    }
                >"Board"</a>
                <a
                    class=project_class
                    href=move || project_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, project_href.get_untracked()));
                    }
                >"Project"</a>
                <a
                    class=triggers_class
                    href=move || triggers_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, triggers_href.get_untracked()));
                    }
                >"Automation"</a>
                <a
                    class=runs_class
                    href=move || runs_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, runs_href.get_untracked()));
                    }
                >"Runs"</a>
                <a
                    class=projects_class
                    href=move || projects_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, projects_href.get_untracked()));
                    }
                >"Projects"</a>
                <a
                    class=system_class
                    href=move || system_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, system_href.get_untracked()));
                    }
                >"System"</a>
                <a
                    class=api_class
                    href=move || api_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, api_href.get_untracked()));
                    }
                >"API"</a>
            </nav>
            <div class="topbar-actions">
                {move || {
                    view! {
                        <TopBarCodexStatus
                            status=codex_status.get()
                            href=system_href.get()
                            active=active == ActivePage::System
                        />
                    }
                }}
                {move || {
                    automation
                        .get()
                        .or_else(|| {
                            let project = effective_selected_project.get()?;
                            let workspace_mode = projects
                                .get()
                                .into_iter()
                                .find(|candidate| candidate.name == project)
                                .map(|project| project.workspace_mode)
                                .unwrap_or(WorkspaceMode::CurrentBranch);
                            Some(TopBarAutomation {
                                project,
                                running: false,
                                workspace_mode,
                                auto_commit: cached_auto_commit,
                                set_auto_commit: set_cached_auto_commit,
                            })
                        })
                        .map(|control| view! { <TopBarAutomationControl control/> })
                }}
            </div>
            <ProjectSwitcher
                active_project_names
                projects
                effective_selected_project
                active
            />
        </header>
    }
}

#[component]
fn ProjectSwitcher(
    active_project_names: Signal<Vec<String>>,
    projects: Signal<Vec<crate::shared::view_models::ProjectView>>,
    effective_selected_project: Memo<Option<String>>,
    active: ActivePage,
) -> impl IntoView {
    let navigate = use_navigate();
    let navigation_scope = expect_context::<CrudInstanceMgrContext>().navigation_scope();
    move || {
        let active_project_names = active_project_names.get();
        let project_options = projects
            .get()
            .into_iter()
            .map(|project| {
                let active = active_project_names.contains(&project.name);
                ProjectSelectOption {
                    name: project.name,
                    display_name: project.display_name,
                    active,
                }
            })
            .collect::<Vec<_>>();
        let initial_project = project_options
            .iter()
            .find(|project| {
                effective_selected_project.get().as_deref() == Some(project.name.as_str())
            })
            .cloned();

        if let Some(initial_project) = initial_project {
            let (selected_option, set_selected_option) = signal(initial_project);
            let project_options_for_select = project_options.clone();
            let navigate = navigate.clone();
            view! {
                <div class="project-switcher">
                    <span class="project-switcher-label">"Project"</span>
                    <Select
                        options=Signal::derive(move || project_options_for_select.clone())
                        search_text_provider=move |option: ProjectSelectOption| {
                            format!("{} {}", option.display_name, option.name)
                        }
                        render_option=project_select_option
                        selected=selected_option
                        set_selected=move |option: ProjectSelectOption| {
                            if selected_option.get_untracked().name == option.name {
                                return;
                            }
                            let href = project_selection_href(active, &option.name);
                            let approved_option = option.clone();
                            let navigate = navigate.clone();
                            navigation_scope.attempt(
                                move || {
                                    set_selected_option.set(approved_option.clone());
                                    navigate(&href, NavigateOptions::default());
                                },
                                || {},
                            );
                        }
                    />
                </div>
            }
            .into_any()
        } else if project_options.is_empty() {
            view! {
                <div class="project-switcher project-switcher-empty">
                    <span class="project-switcher-label">"Project"</span>
                    <span class="project-empty">"No projects"</span>
                </div>
            }
            .into_any()
        } else {
            let navigate = navigate.clone();
            let (selected_choice, set_selected_choice) = signal(String::new());
            view! {
                <div class="project-switcher project-switcher-empty">
                    <label class="project-switcher-label" for="project-switcher-choice">
                        "Project"
                    </label>
                    <select
                        id="project-switcher-choice"
                        aria-label="Choose active project"
                        prop:value=move || selected_choice.get()
                        on:change=move |event| {
                            let project = event_target_value(&event);
                            if !project.is_empty() {
                                set_selected_choice.set(project.clone());
                                let href = project_selection_href(active, &project);
                                let navigate = navigate.clone();
                                navigation_scope.attempt(
                                    move || navigate(&href, NavigateOptions::default()),
                                    move || set_selected_choice.set(String::new()),
                                );
                            }
                        }
                    >
                        <option value="" selected disabled>"Choose a project"</option>
                        {project_options
                            .into_iter()
                            .map(|project| {
                                let label = if project.active {
                                    format!("{} (running)", project.display_name)
                                } else {
                                    project.display_name
                                };
                                view! { <option value=project.name>{label}</option> }
                            })
                            .collect_view()}
                    </select>
                </div>
            }
            .into_any()
        }
    }
}

#[component]
fn TopBarCodexStatus(
    status: CodexAppServerStatusView,
    href: String,
    active: bool,
) -> impl IntoView {
    let attempt_link_navigation = use_link_navigation_attempt();
    let (tone, label) = if status.usable {
        ("ready", "Ready")
    } else if status.available {
        ("blocked", "Blocked")
    } else {
        ("unavailable", "Unavailable")
    };
    let active_class = if active { " active" } else { "" };
    let class = format!("topbar-codex codex-readiness-{tone}{active_class}");
    let title = status.message;
    let aria_label = format!("Codex automation readiness: {label}");

    view! {
        <a
            class=class
            href=href.clone()
            title=title
            aria-label=aria_label
            on:click=move |event| attempt_link_navigation.run((event, href.clone()))
        >
            <span class="topbar-codex-dot" aria-hidden="true"></span>
            <span class="topbar-codex-name">"Codex"</span>
            <strong class="topbar-codex-state">{label}</strong>
        </a>
    }
}

#[component]
fn TopBarAutomationControl(control: TopBarAutomation) -> impl IntoView {
    let (running, set_running) = signal(control.running);
    let (pending, set_pending) = signal(false);
    let service = automation_service();
    let project = control.project.clone();
    let toggle_running = move |_| {
        if pending.get_untracked() {
            return;
        }
        let previous = running.get_untracked();
        let next = !previous;
        set_running.set(next);
        set_pending.set(true);
        let project = project.clone();
        let service = service.clone();
        leptos::task::spawn_local(async move {
            if service.set_running(project, next).await.is_err() {
                set_running.set(previous);
            }
            set_pending.set(false);
        });
    };

    view! {
        <div class="topbar-automation-group">
            <TopBarAutoCommitControl
                project=control.project
                workspace_mode=control.workspace_mode
                auto_commit=control.auto_commit
                set_auto_commit=control.set_auto_commit
            />
            <div class="topbar-automation">
                <span
                    class="automation-status"
                    class:running=move || running.get()
                    class:stopped=move || !running.get()
                >
                    {move || if running.get() { "Running" } else { "Stopped" }}
                </span>
                <button
                    type="button"
                    class:danger=move || running.get()
                    disabled=move || pending.get()
                    on:click=toggle_running
                >
                    {move || if running.get() { "Stop" } else { "Start" }}
                </button>
            </div>
        </div>
    }
}

#[component]
fn TopBarAutoCommitControl(
    project: String,
    workspace_mode: WorkspaceMode,
    auto_commit: ReadSignal<bool>,
    set_auto_commit: WriteSignal<bool>,
) -> impl IntoView {
    if workspace_mode != WorkspaceMode::CurrentBranch {
        return ().into_any();
    }
    let service = project_service();
    let (pending, set_pending) = signal(false);
    let (failed, set_failed) = signal(false);
    let update = Callback::new(move |next: bool| {
        if pending.get_untracked() {
            return;
        }
        let previous = auto_commit.get_untracked();
        set_auto_commit.set(next);
        set_pending.set(true);
        set_failed.set(false);

        let project = project.clone();
        let service = service.clone();
        leptos::task::spawn_local(async move {
            if service.update_auto_commit(project, next).await.is_ok() {
                set_pending.set(false);
            } else {
                set_auto_commit.set(previous);
                set_pending.set(false);
                set_failed.set(true);
            }
        });
    });

    view! {
        <div
            class="topbar-auto-commit"
            class:enabled=move || auto_commit.get()
            class:pending=move || pending.get()
            class:failed=move || failed.get()
            title=move || {
                if pending.get() {
                    "Saving Auto-Commit setting"
                } else if auto_commit.get() {
                    "Turn Auto-Commit off"
                } else {
                    "Turn Auto-Commit on"
                }
            }
        >
            <span class="auto-commit-label">"Auto-Commit"</span>
            <Toggle
                state=auto_commit
                set_state=update
                disabled=Signal::derive(move || pending.get())
                attr:aria-label="Auto-Commit"
            />
        </div>
    }
    .into_any()
}

fn project_select_option(option: ProjectSelectOption) -> AnyView {
    view! {
        <span class="project-option">
            <span
                class="project-option-dot"
                class:active=option.active
                aria-hidden="true"
            ></span>
            <span class="project-option-name">{option.display_name}</span>
        </span>
    }
    .into_any()
}

fn active_class(active: ActivePage, page: ActivePage) -> &'static str {
    if active == page { "active" } else { "" }
}

fn project_selection_href(active: ActivePage, project: &str) -> String {
    let path = match active {
        ActivePage::Board => "/",
        ActivePage::Project => "/project",
        ActivePage::Triggers => "/automation",
        ActivePage::Runs => "/runs",
        ActivePage::System => "/system",
        ActivePage::Projects => "/projects",
        ActivePage::Api => "/api/docs",
    };
    format!("{path}?project={}", encode_path(project))
}

fn use_link_navigation_attempt() -> Callback<(leptos::ev::MouseEvent, String)> {
    let navigation_scope = expect_context::<CrudInstanceMgrContext>().navigation_scope();
    let navigate = use_navigate();
    Callback::new(move |(event, href): (leptos::ev::MouseEvent, String)| {
        if !is_unmodified_primary_click(
            event.button(),
            event.ctrl_key(),
            event.meta_key(),
            event.shift_key(),
            event.alt_key(),
        ) {
            return;
        }
        event.prevent_default();
        let navigate = navigate.clone();
        navigation_scope.attempt(move || navigate(&href, NavigateOptions::default()), || {});
    })
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

#[cfg(test)]
mod tests {
    use super::is_unmodified_primary_click;
    use assertr::prelude::*;

    #[test]
    fn link_navigation_attempt_intercepts_only_plain_primary_clicks() {
        assert_that!(is_unmodified_primary_click(0, false, false, false, false)).is_true();
        assert_that!(is_unmodified_primary_click(1, false, false, false, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, true, false, false, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, false, true, false, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, false, false, true, false)).is_false();
        assert_that!(is_unmodified_primary_click(0, false, false, false, true)).is_false();
    }
}
