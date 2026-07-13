use crate::{
    frontend::services::{project_cache, project_service},
    shared::view_models::{CodexAppServerStatusView, WorkspaceMode},
};
use leptonic::components::prelude::Select;
use leptos::prelude::*;
use leptos_router::{NavigateOptions, hooks::use_navigate};

use super::encode_path;

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum ActivePage {
    Board,
    Triggers,
    Runs,
    Codex,
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

pub(crate) fn top_bar(
    active_project_names: Signal<Vec<String>>,
    selected_project: Signal<Option<String>>,
    active: ActivePage,
    automation: Signal<Option<TopBarAutomation>>,
    codex_status: Signal<CodexAppServerStatusView>,
) -> impl IntoView + 'static {
    let projects = project_cache().projects();
    let navigate = use_navigate();
    let effective_selected_project = Memo::new(move |_| {
        selected_project
            .get()
            .or_else(|| projects.get().first().map(|project| project.name.clone()))
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
    let triggers_href = Memo::new(move |_| format!("/automation{}", selected_query.get()));
    let runs_href = Memo::new(move |_| format!("/runs{}", selected_query.get()));
    let codex_href = Memo::new(move |_| format!("/codex{}", selected_query.get()));
    let projects_href = Memo::new(move |_| format!("/projects{}", selected_query.get()));
    let api_href = Memo::new(move |_| format!("/api/docs{}", selected_query.get()));
    let board_class = active_class(active, ActivePage::Board);
    let triggers_class = active_class(active, ActivePage::Triggers);
    let runs_class = active_class(active, ActivePage::Runs);
    let projects_class = active_class(active, ActivePage::Projects);
    let api_class = active_class(active, ActivePage::Api);

    let project_switcher = move || {
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
            .or_else(|| project_options.first())
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
                            set_selected_option.set(option.clone());
                            navigate(
                                &format!("/?project={}", encode_path(&option.name)),
                                NavigateOptions::default(),
                            );
                        }
                    />
                </div>
            }
            .into_any()
        } else {
            view! {
                <div class="project-switcher project-switcher-empty">
                    <span class="project-switcher-label">"Project"</span>
                    <span class="project-empty">"No projects"</span>
                </div>
            }
            .into_any()
        }
    };

    view! {
        <header class="app-topbar">
            <a class="brand" href=move || board_href.get()>
                <img
                    class="brand-icon"
                    src="/branding/dispatch-icon-64.png"
                    alt=""
                    aria-hidden="true"
                />
                "Dispatch"
            </a>
            <nav class="top-nav" aria-label="Primary">
                <a class=board_class href=move || board_href.get()>"Board"</a>
                <a class=triggers_class href=move || triggers_href.get()>"Automation"</a>
                <a class=runs_class href=move || runs_href.get()>"Runs"</a>
                <a class=projects_class href=move || projects_href.get()>"Projects"</a>
                <a class=api_class href=move || api_href.get()>"API"</a>
            </nav>
            <div class="topbar-actions">
                {move || {
                    top_bar_codex_status(
                        codex_status.get(),
                        codex_href.get(),
                        active == ActivePage::Codex,
                    )
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
                        .map(top_bar_automation_control)
                }}
            </div>
            {project_switcher}
        </header>
    }
}

fn top_bar_codex_status(status: CodexAppServerStatusView, href: String, active: bool) -> AnyView {
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
        <a class=class href=href title=title aria-label=aria_label>
            <span class="topbar-codex-dot" aria-hidden="true"></span>
            <span class="topbar-codex-name">"Codex"</span>
            <strong class="topbar-codex-state">{label}</strong>
        </a>
    }
    .into_any()
}

fn top_bar_automation_control(control: TopBarAutomation) -> AnyView {
    let auto_commit_control = top_bar_auto_commit_control(&control);
    if control.running {
        let stop_action = format!(
            "/projects/{}/automation/stop",
            encode_path(&control.project)
        );
        view! {
            <div class="topbar-automation-group">
                {auto_commit_control}
                <form class="topbar-automation" method="post" action=stop_action>
                    <span class="automation-status running">"Running"</span>
                    <button type="submit" class="danger">"Stop"</button>
                </form>
            </div>
        }
        .into_any()
    } else {
        let start_action = format!(
            "/projects/{}/automation/start",
            encode_path(&control.project)
        );
        view! {
            <div class="topbar-automation-group">
                {auto_commit_control}
                <form class="topbar-automation" method="post" action=start_action>
                    <span class="automation-status stopped">"Stopped"</span>
                    <button type="submit">"Start"</button>
                </form>
            </div>
        }
        .into_any()
    }
}

fn top_bar_auto_commit_control(control: &TopBarAutomation) -> Option<AnyView> {
    if control.workspace_mode != WorkspaceMode::CurrentBranch {
        return None;
    }
    let action = format!(
        "/projects/{}/settings/auto-commit",
        encode_path(&control.project)
    );
    let auto_commit = control.auto_commit;
    let set_auto_commit = control.set_auto_commit;
    let project = control.project.clone();
    let service = project_service();
    let (pending, set_pending) = signal(false);
    let (failed, set_failed) = signal(false);
    let submit = move |event: leptos::ev::SubmitEvent| {
        event.prevent_default();
        if pending.get_untracked() {
            return;
        }
        let previous = auto_commit.get_untracked();
        let next = !previous;
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
    };

    Some(
        view! {
            <form class="topbar-auto-commit-form" method="post" action=action on:submit=submit>
                <input type="hidden" name="enabled" value=move || (!auto_commit.get()).to_string()/>
                <button
                    type="submit"
                    class=move || {
                        let mut class = if auto_commit.get() {
                            "topbar-auto-commit enabled".to_owned()
                        } else {
                            "topbar-auto-commit".to_owned()
                        };
                        if pending.get() {
                            class.push_str(" pending");
                        }
                        if failed.get() {
                            class.push_str(" failed");
                        }
                        class
                    }
                    role="switch"
                    aria-checked=move || auto_commit.get().to_string()
                    title=move || {
                        if pending.get() {
                            "Saving Auto-Commit setting".to_owned()
                        } else if auto_commit.get() {
                            "Turn Auto-Commit off".to_owned()
                        } else {
                            "Turn Auto-Commit on".to_owned()
                        }
                    }
                    disabled=move || pending.get()
                >
                    <span class="auto-commit-label">"Auto-Commit"</span>
                    <span class="auto-commit-track" aria-hidden="true">
                        <span class="auto-commit-thumb"></span>
                    </span>
                    <strong>{move || if auto_commit.get() { "On" } else { "Off" }}</strong>
                </button>
            </form>
        }
        .into_any(),
    )
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
