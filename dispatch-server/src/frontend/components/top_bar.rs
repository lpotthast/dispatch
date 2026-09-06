use crate::{
    frontend::services::{automation_service, codex_service, project_cache, project_service},
    shared::view_models::{CodexAppServerStatusView, WorkspaceMode},
};
use crudkit_leptos::crud_instance_mgr::CrudInstanceMgrContext;
use leptonic::components::prelude::{Select, Toggle};
use leptos::prelude::*;
use leptos_router::{
    NavigateOptions,
    hooks::{use_location, use_navigate},
};

use super::{cached_query, encode_path, selected_project_signal};
use crate::frontend::live_events::{refetch_on_live_event, top_bar_event_matches};

#[derive(Clone, Copy, Eq, PartialEq)]
enum ActivePage {
    Board,
    Project,
    Knowledge,
    Triggers,
    Runs,
    System,
    Metrics,
    Projects,
    Api,
}

impl ActivePage {
    fn from_path(path: &str) -> Self {
        match path.trim_end_matches('/') {
            "/project" => Self::Project,
            "/knowledge" => Self::Knowledge,
            "/automation" => Self::Triggers,
            "/runs" => Self::Runs,
            "/system" | "/codex" => Self::System,
            "/metrics" => Self::Metrics,
            "/projects" => Self::Projects,
            "/api/docs" => Self::Api,
            _ => Self::Board,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct ProjectSelectOption {
    name: String,
    display_name: String,
    active: bool,
}

#[component]
pub(crate) fn TopBar() -> impl IntoView + 'static {
    let selected_project = selected_project_signal();
    let location = use_location();
    let active = Memo::new(move |_| ActivePage::from_path(&location.pathname.get()));
    let service = project_service();
    let initial = service.cached_page_untracked();
    let cache_service = service.clone();
    let result = cached_query(
        initial,
        || (),
        move |()| cache_service.cached_page(),
        move |()| {
            let service = service.clone();
            async move { service.load_page().await }
        },
    );
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, top_bar_event_matches);
    let active_project_names = Memo::new(move |_| {
        result
            .value
            .with(|page| page.as_ref().map(|page| page.active_project_names.clone()))
            .unwrap_or_default()
    });
    let attempt_link_navigation = use_link_navigation_attempt();
    let projects = project_cache().projects();
    let effective_selected_project = Memo::new(move |_| {
        selected_project.get().filter(|selected| {
            projects.with(|projects| projects.iter().any(|project| project.name == *selected))
        })
    });
    let selected_query = Memo::new(move |_| {
        selected_project
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
    let knowledge_href = Memo::new(move |_| format!("/knowledge{}", selected_query.get()));
    let triggers_href = Memo::new(move |_| format!("/automation{}", selected_query.get()));
    let runs_href = Memo::new(move |_| format!("/runs{}", selected_query.get()));
    let system_href = Memo::new(move |_| format!("/system{}", selected_query.get()));
    let metrics_href = Memo::new(move |_| format!("/metrics{}", selected_query.get()));
    let projects_href = Memo::new(move |_| format!("/projects{}", selected_query.get()));
    let api_href = Memo::new(move |_| format!("/api/docs{}", selected_query.get()));
    let codex = codex_service();
    let shared_codex_status = Memo::new(move |_| codex.cached_status().unwrap_or_default());

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
                    class=move || active_class(active.get(), ActivePage::Board)
                    href=move || board_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, board_href.get_untracked()));
                    }
                >"Board"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Project)
                    href=move || project_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, project_href.get_untracked()));
                    }
                >"Project"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Knowledge)
                    href=move || knowledge_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, knowledge_href.get_untracked()));
                    }
                >"Knowledge"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Triggers)
                    href=move || triggers_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, triggers_href.get_untracked()));
                    }
                >"Automation"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Runs)
                    href=move || runs_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, runs_href.get_untracked()));
                    }
                >"Runs"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Projects)
                    href=move || projects_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, projects_href.get_untracked()));
                    }
                >"Projects"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::System)
                    href=move || system_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, system_href.get_untracked()));
                    }
                >"System"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Metrics)
                    href=move || metrics_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, metrics_href.get_untracked()));
                    }
                >"Metrics"</a>
                <a
                    class=move || active_class(active.get(), ActivePage::Api)
                    href=move || api_href.get()
                    on:click=move |event| {
                        attempt_link_navigation.run((event, api_href.get_untracked()));
                    }
                >"API"</a>
            </nav>
            <div class="topbar-actions">
                <TopBarCodexStatus
                    status=shared_codex_status
                    href=system_href
                    active=Memo::new(move |_| active.get() == ActivePage::System)
                />
                <For
                    each=move || effective_selected_project.get()
                    key=|project| project.clone()
                    children=move |project| {
                        let project_for_view = project.clone();
                        let project_view = Memo::new(move |_| projects.with(|projects| {
                            projects.iter().find(|candidate| candidate.name == project_for_view).cloned()
                        }));
                        let running_project = project.clone();
                        let running = Memo::new(move |_| active_project_names.with(|names| names.contains(&running_project)));
                        view! { <TopBarAutomationControl project project_view running/> }
                    }
                />
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
    active_project_names: Memo<Vec<String>>,
    projects: Signal<Vec<crate::shared::view_models::ProjectView>>,
    effective_selected_project: Memo<Option<String>>,
    active: Memo<ActivePage>,
) -> impl IntoView {
    let project_options = Memo::new(move |_| {
        let active_project_names = active_project_names.get();
        projects.with(|projects| {
            projects
                .iter()
                .map(|project| {
                    let active = active_project_names.contains(&project.name);
                    ProjectSelectOption {
                        name: project.name.clone(),
                        display_name: project.display_name.clone(),
                        active,
                    }
                })
                .collect::<Vec<_>>()
        })
    });
    let selected_option = Memo::new(move |_| {
        let selected = effective_selected_project.get();
        project_options.with(|options| {
            options
                .iter()
                .find(|option| Some(option.name.as_str()) == selected.as_deref())
                .cloned()
        })
    });

    view! {
        <Show
            when=move || selected_option.get().is_some()
            fallback=move || view! {
                <Show
                    when=move || !project_options.read().is_empty()
                    fallback=|| view! { <EmptyProjectSwitcher/> }
                >
                    <ChooseProjectSwitcher project_options active/>
                </Show>
            }
        >
            <SelectedProjectSwitcher project_options selected_option active/>
        </Show>
    }
}

#[component]
fn SelectedProjectSwitcher(
    project_options: Memo<Vec<ProjectSelectOption>>,
    selected_option: Memo<Option<ProjectSelectOption>>,
    active: Memo<ActivePage>,
) -> impl IntoView {
    let navigate = use_navigate();
    let navigation_scope = expect_context::<CrudInstanceMgrContext>().navigation_scope();
    let selected = Signal::derive(move || {
        selected_option
            .get()
            .expect("selected project switcher requires a selected project")
    });

    view! {
        <div class="project-switcher">
            <span class="project-switcher-label">"Project"</span>
            <Select
                options=Signal::derive(move || project_options.get())
                search_text_provider=move |option: ProjectSelectOption| {
                    format!("{} {}", option.display_name, option.name)
                }
                render_option=project_select_option
                selected
                set_selected=move |option: ProjectSelectOption| {
                    if selected_option
                        .with_untracked(|selected| selected.as_ref().map(|selected| &selected.name) == Some(&option.name))
                    {
                        return;
                    }
                    let href = project_selection_href(active.get_untracked(), &option.name);
                    let navigate = navigate.clone();
                    navigation_scope.attempt(
                        move || navigate(&href, NavigateOptions::default()),
                        || {},
                    );
                }
            />
        </div>
    }
}

#[component]
fn ChooseProjectSwitcher(
    project_options: Memo<Vec<ProjectSelectOption>>,
    active: Memo<ActivePage>,
) -> impl IntoView {
    let navigate = use_navigate();
    let navigation_scope = expect_context::<CrudInstanceMgrContext>().navigation_scope();
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
                        let href = project_selection_href(active.get_untracked(), &project);
                        let navigate = navigate.clone();
                        navigation_scope.attempt(
                            move || navigate(&href, NavigateOptions::default()),
                            move || set_selected_choice.set(String::new()),
                        );
                    }
                }
            >
                <option value="" selected disabled>"Choose a project"</option>
                <For
                    each=move || project_options.get()
                    key=|project| project.name.clone()
                    children=|project| {
                        let label = if project.active {
                            format!("{} (running)", project.display_name)
                        } else {
                            project.display_name
                        };
                        view! { <option value=project.name>{label}</option> }
                    }
                />
            </select>
        </div>
    }
}

#[component]
fn EmptyProjectSwitcher() -> impl IntoView {
    view! {
        <div class="project-switcher project-switcher-empty">
            <span class="project-switcher-label">"Project"</span>
            <span class="project-empty">"No projects"</span>
        </div>
    }
}

#[component]
fn TopBarCodexStatus(
    status: Memo<CodexAppServerStatusView>,
    href: Memo<String>,
    active: Memo<bool>,
) -> impl IntoView {
    let attempt_link_navigation = use_link_navigation_attempt();
    let readiness = Memo::new(move |_| {
        status.with(|status| {
            if status.usable {
                ("ready", "Ready")
            } else if status.available {
                ("blocked", "Blocked")
            } else if status.checked_at.is_empty() {
                ("unavailable", "Checking…")
            } else {
                ("unavailable", "Unavailable")
            }
        })
    });
    view! {
        <a
            class=move || format!("topbar-codex codex-readiness-{}{}", readiness.get().0, if active.get() { " active" } else { "" })
            href=move || href.get()
            title=move || status.with(|status| status.message.clone())
            aria-label=move || format!("Codex automation readiness: {}", readiness.get().1)
            on:click=move |event| attempt_link_navigation.run((event, href.get_untracked()))
        >
            <span class="topbar-codex-dot" aria-hidden="true"></span>
            <span class="topbar-codex-name">"Codex"</span>
            <strong class="topbar-codex-state">{move || readiness.get().1}</strong>
        </a>
    }
}

#[component]
fn TopBarAutomationControl(
    project: String,
    project_view: Memo<Option<crate::shared::view_models::ProjectView>>,
    running: Memo<bool>,
) -> impl IntoView {
    let server_running = running;
    let (running, set_running) = signal(server_running.get_untracked());
    Effect::new(move |_| set_running.set(server_running.get()));
    let (pending, set_pending) = signal(false);
    let service = automation_service();
    let toggle_project = project.clone();
    let toggle_running = move |_| {
        if pending.get_untracked() {
            return;
        }
        let previous = running.get_untracked();
        let next = !previous;
        set_running.set(next);
        set_pending.set(true);
        let project = toggle_project.clone();
        let service = service.clone();
        leptos::task::spawn_local(async move {
            if service.set_running(project, next).await.is_err() {
                set_running.try_set(previous);
            }
            set_pending.try_set(false);
        });
    };

    view! {
        <div class="topbar-automation-group">
            <TopBarAutoCommitControl
                project
                project_view
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
                    title="Start or stop project automation"
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
    project_view: Memo<Option<crate::shared::view_models::ProjectView>>,
) -> impl IntoView {
    let saved = Memo::new(move |_| {
        project_view.with(|project| project.as_ref().is_some_and(|project| project.auto_commit))
    });
    let workspace_mode = Memo::new(move |_| {
        project_view.with(|project| project.as_ref().map(|project| project.workspace_mode))
    });
    let (auto_commit, set_auto_commit) = signal(saved.get_untracked());
    Effect::new(move |_| set_auto_commit.set(saved.get()));
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
                set_pending.try_set(false);
            } else {
                set_auto_commit.try_set(previous);
                set_pending.try_set(false);
                set_failed.try_set(true);
            }
        });
    });

    view! {
        <Show when=move || workspace_mode.get() == Some(WorkspaceMode::CurrentBranch)>
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
        </Show>
    }
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
        ActivePage::Knowledge => "/knowledge",
        ActivePage::Triggers => "/automation",
        ActivePage::Runs => "/runs",
        ActivePage::System => "/system",
        ActivePage::Metrics => "/metrics",
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
