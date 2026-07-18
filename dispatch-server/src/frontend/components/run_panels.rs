use std::collections::HashSet;

use crate::{
    frontend::{
        live_events::{
            refetch_on_live_event, runs_section_event_matches, trigger_runs_event_matches,
        },
        pages::{BoardRunSessionView, format_number},
        services::{automation_service, run_service},
    },
    shared::view_models::{
        AgentCommitOutcome, AgentRunStatus, AgentRunTokenUsageView, AgentRunView,
        AutomationStatusView,
    },
};
use leptos::prelude::*;
use leptos_router::{
    NavigateOptions,
    hooks::{use_navigate, use_query_map},
};

use super::{RunOutput, cached_query, encode_path};

#[component]
pub(crate) fn LiveRunsSection(
    project: String,
    initial_status: AutomationStatusView,
    initial_running: bool,
    initial_run_sessions: Vec<BoardRunSessionView>,
) -> impl IntoView + 'static {
    let (automation_status, set_automation_status) = signal(initial_status);
    let (automation_running, set_automation_running) = signal(initial_running);
    let (run_sessions, set_run_sessions) = signal(initial_run_sessions);
    let project_for_loader = project.clone();
    let service = run_service();
    let initial = service.cached_section_untracked(&project);
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let section = cached_query(
        initial,
        move || project_for_loader.clone(),
        move |project| service_for_cache.cached_section(project),
        move |project| {
            let service = service_for_load.clone();
            let project = project.clone();
            async move { service.load_section(project).await }
        },
    );
    let project_for_events = project.clone();
    refetch_on_live_event(section.refresh, move |event| {
        runs_section_event_matches(event, project_for_events.as_str())
    });

    Effect::new(move |_| {
        if let Some(section) = section.value.get() {
            set_automation_status.set(section.automation_status);
            set_automation_running.set(section.automation_running);
            set_run_sessions.set(section.run_sessions);
        }
    });

    let status_note = Signal::derive(move || {
        let status = automation_status.get();
        let running_runs = status.running_runs;
        let mutating = status.running_mutating_runs;
        let read_only = status.running_read_only_runs;
        let controller = if automation_running.get() {
            "controller running"
        } else {
            "controller stopped"
        };
        Some(format!(
            "{running_runs} running ({mutating} mutating, {read_only} read-only), {controller}"
        ))
    });

    view! {
        <RunSessionsPanel
            project=project
            title="Runs"
            status_note=status_note
            run_sessions=run_sessions
            sync_selection_with_url=true
            empty_message="No runs yet."
        />
    }
}

#[component]
pub(crate) fn TriggerRunsPanel(
    project: String,
    selected_trigger_id: Memo<Option<i64>>,
) -> impl IntoView + 'static {
    let service = automation_service();
    let project_for_loader = project.clone();
    let project_for_view = project.clone();
    let initial = selected_trigger_id.get_untracked().and_then(|trigger_id| {
        service
            .cached_trigger_runs_untracked(&project, trigger_id)
            .map(Some)
    });
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let trigger_runs = cached_query(
        initial,
        move || (project_for_loader.clone(), selected_trigger_id.get()),
        move |(project, trigger_id)| {
            trigger_id.and_then(|trigger_id| {
                service_for_cache
                    .cached_trigger_runs(project, trigger_id)
                    .map(Some)
            })
        },
        move |(project, trigger_id)| {
            let service = service_for_load.clone();
            let project = project.clone();
            async move {
                match trigger_id {
                    Some(trigger_id) => service
                        .load_trigger_runs(project, trigger_id)
                        .await
                        .map(Some),
                    None => Ok(None),
                }
            }
        },
    );
    let project_for_events = project.clone();
    refetch_on_live_event(trigger_runs.refresh, move |event| {
        trigger_runs_event_matches(event, project_for_events.as_str())
            && selected_trigger_id.get().is_some()
    });
    let (run_sessions, set_run_sessions) = signal(Vec::<BoardRunSessionView>::new());
    Effect::new(move |_| {
        if let Some(result) = trigger_runs.value.get() {
            match result {
                Some(sessions) => set_run_sessions.set(sessions),
                None => set_run_sessions.set(Vec::new()),
            }
        }
    });

    view! {
        {move || {
            if selected_trigger_id.get().is_some() {
                let selected_trigger_id = selected_trigger_id.get();
                view! {
                    {selected_trigger_id.map(|trigger_id| {
                        view! {
                            <section class="automation trigger-actions panel">
                                <div class="panel-heading">
                                    <h2>"Selected automation"</h2>
                                </div>
                                <QueueAutomationEvaluation
                                    project=project_for_view.clone()
                                    trigger_id
                                    refresh=trigger_runs.refresh
                                />
                            </section>
                        }
                    })}
                    <RunSessionsPanel
                        project=project_for_view.clone()
                        title="Runs for selected automation"
                        status_note=Signal::derive(|| None::<String>)
                        run_sessions=run_sessions
                        sync_selection_with_url=false
                        empty_message="No runs for this automation yet."
                    />
                }.into_any()
            } else {
                view! {
                <section class="automation trigger-runs">
                    <div class="panel-heading">
                        <h2>"Runs"</h2>
                        <p class="muted">"Edit or inspect an automation to filter this panel."</p>
                    </div>
                    <p class="muted">"No automation selected."</p>
                </section>
                }.into_any()
            }
        }}
    }
}

#[component]
fn QueueAutomationEvaluation(
    project: String,
    trigger_id: i64,
    refresh: Callback<()>,
) -> impl IntoView {
    let service = automation_service();
    let (pending, set_pending) = signal(false);
    let queue = move |_| {
        if pending.get_untracked() {
            return;
        }
        set_pending.set(true);
        let service = service.clone();
        let project = project.clone();
        leptos::task::spawn_local(async move {
            if service
                .schedule_trigger_evaluation(project, trigger_id)
                .await
                .is_ok()
            {
                refresh.run(());
            }
            set_pending.set(false);
        });
    };

    view! {
        <button type="button" disabled=move || pending.get() on:click=queue>
            "Queue evaluation"
        </button>
    }
}

#[component]
fn RunSessionsPanel(
    project: String,
    title: &'static str,
    #[prop(into)] status_note: Signal<Option<String>>,
    #[prop(into)] run_sessions: ReadSignal<Vec<BoardRunSessionView>>,
    sync_selection_with_url: bool,
    empty_message: &'static str,
) -> impl IntoView + 'static {
    let query = use_query_map();
    let initial_selected_run_id = if sync_selection_with_url {
        query
            .read_untracked()
            .get("run")
            .and_then(|value| value.parse::<i64>().ok())
    } else {
        None
    };
    let (selected_run_id, set_selected_run_id) = signal(initial_selected_run_id);
    let shown_thinking_history = RwSignal::new(HashSet::<i64>::new());
    Effect::new(move |_| {
        if !sync_selection_with_url {
            return;
        }
        let query_selected = query
            .read()
            .get("run")
            .and_then(|value| value.parse::<i64>().ok());
        if let Some(run_id) = query_selected
            && selected_run_id.get_untracked() != Some(run_id)
        {
            set_selected_run_id.set(Some(run_id));
        }
    });
    Effect::new(move |_| {
        let sessions = run_sessions.get();
        let selected = selected_run_id.get_untracked();
        let selected_still_exists = selected
            .map(|run_id| sessions.iter().any(|session| session.run.id == run_id))
            .unwrap_or(false);
        let next = if sessions.is_empty() {
            None
        } else if selected_still_exists {
            selected
        } else {
            sessions.first().map(|session| session.run.id)
        };
        if selected != next {
            set_selected_run_id.set(next);
        }
    });

    let navigate = use_navigate();
    let selection_project = project.clone();
    let select_run = Callback::new(move |run_id: i64| {
        set_selected_run_id.set(Some(run_id));
        if sync_selection_with_url {
            let href = format!(
                "/runs?project={}&run={run_id}",
                encode_path(&selection_project)
            );
            navigate(
                &href,
                NavigateOptions {
                    replace: true,
                    scroll: false,
                    ..NavigateOptions::default()
                },
            );
        }
    });

    let run_items = move || {
        let sessions = run_sessions.get();
        if sessions.is_empty() {
            return view! { <p class="muted">{empty_message}</p> }.into_any();
        }
        let sessions = sessions
            .into_iter()
            .map(|session| {
                let run_id = session.run.id;
                let is_active = session.active;
                let summary = run_result_summary(&session.run);
                let origin = run_origin_label(&session.run);
                let item = run_item_label(&session.run);
                let tokens = session.run.token_usage.map(run_token_usage_label);
                let status_class = run_status_class(session.run.status);
                let selected_signal = selected_run_id;
                view! {
                    <button
                        type="button"
                        class=move || {
                            let selected = if selected_signal.get() == Some(run_id) {
                                " selected"
                            } else {
                                ""
                            };
                            format!("run-session {status_class}{selected}")
                        }
                        aria-pressed=move || selected_signal.get() == Some(run_id)
                        on:click=move |_| select_run.run(run_id)
                    >
                        <div class="session-head">
                            <strong>"#" {run_id}</strong>
                            <span>{session.run.status.to_string()}</span>
                            {item.map(|item| view! { <span>{item}</span> })}
                            {origin.map(|origin| view! { <span>{origin}</span> })}
                            {tokens.map(|tokens| view! { <span>{tokens}</span> })}
                            {is_active.then(|| view! { <span class="live-badge">"active"</span> })}
                        </div>
                        <p>{summary}</p>
                    </button>
                }
            })
            .collect::<Vec<_>>();
        view! { <div class="run-session-list">{sessions}</div> }.into_any()
    };
    let detail_project = project.clone();
    let run_detail = move || {
        let detail_sessions = run_sessions.get();
        let selected = selected_run_id
            .get()
            .and_then(|run_id| {
                detail_sessions
                    .iter()
                    .find(|session| session.run.id == run_id)
                    .cloned()
            })
            .or_else(|| detail_sessions.first().cloned());
        match selected {
            Some(session) => {
                let run_id = session.run.id;
                let show_thinking_history = shown_thinking_history.get().contains(&run_id);
                let toggle_thinking_history = Callback::new(move |()| {
                    shown_thinking_history.update(|shown| {
                        if !shown.remove(&run_id) {
                            shown.insert(run_id);
                        }
                    });
                });
                run_session_detail(
                    &detail_project,
                    session,
                    show_thinking_history,
                    toggle_thinking_history,
                )
            }
            None => view! { <p class="muted">"No run selected."</p> }.into_any(),
        }
    };

    view! {
        <section class="automation">
            <div class="panel-heading">
                <h2>{title}</h2>
                {move || status_note.get().map(|note| view! { <p class="muted">{note}</p> })}
            </div>
            <div class="run-session-shell">
                {run_items}
                <aside class="run-session-detail">
                    {run_detail}
                </aside>
            </div>
        </section>
    }
}

pub(crate) fn run_status_class(status: AgentRunStatus) -> String {
    format!("status-{}", status.as_storage())
}

pub(crate) fn run_result_summary(run: &AgentRunView) -> String {
    if run.result_summary.trim().is_empty() {
        "No summary yet.".to_owned()
    } else {
        run.result_summary.clone()
    }
}

pub(crate) fn run_commit_outcome_label(run: &AgentRunView) -> String {
    let requirement = if run.commit_required {
        "required"
    } else {
        "not required"
    };
    let base = match run.commit_outcome {
        AgentCommitOutcome::NotEvaluated => "not evaluated".to_owned(),
        AgentCommitOutcome::NotRequired => "not required by policy".to_owned(),
        AgentCommitOutcome::Committed => {
            if run.commit_shas.is_empty() {
                "committed".to_owned()
            } else {
                let shas = run
                    .commit_shas
                    .iter()
                    .map(|sha| sha.chars().take(12).collect::<String>())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("committed {shas}")
            }
        }
        AgentCommitOutcome::SkippedNoChanges => "skipped: no changes".to_owned(),
        AgentCommitOutcome::SkippedNoGitRepo => "skipped: no git repository".to_owned(),
        AgentCommitOutcome::MissingRequired => "missing required commit".to_owned(),
        AgentCommitOutcome::Unknown => "unknown".to_owned(),
    };
    format!("{base} ({requirement})")
}

pub(crate) fn run_token_usage_text(run: &AgentRunView) -> String {
    run.token_usage
        .map(run_token_usage_label)
        .unwrap_or_else(|| "not reported".to_owned())
}

pub(crate) fn run_token_usage_label(usage: AgentRunTokenUsageView) -> String {
    format!(
        "{} total ({} input, {} cached input, {} output)",
        format_number(usage.total_tokens),
        format_number(usage.input_tokens),
        format_number(usage.cached_input_tokens),
        format_number(usage.output_tokens)
    )
}

pub(crate) fn run_origin_label(run: &AgentRunView) -> Option<String> {
    run.trigger_id.map(|trigger_id| {
        let trigger_name = run
            .trigger_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty());
        match trigger_name {
            Some(trigger_name) => format!("trigger #{trigger_id} {trigger_name}"),
            None => format!("trigger #{trigger_id}"),
        }
    })
}

fn run_item_label(run: &AgentRunView) -> Option<String> {
    run.work_item_id.map(|item_id| format!("item #{item_id}"))
}

pub(crate) fn run_work_item_link(project: &str, item_id: Option<i64>) -> Option<AnyView> {
    item_id.map(|item_id| {
        let href = format!("/projects/{}/items/{}", encode_path(project), item_id);
        view! {
            <a class="run-item-link" href=href>"Item #" {item_id}</a>
        }
        .into_any()
    })
}

pub(crate) fn recorded_field(value: &str) -> String {
    if value.trim().is_empty() {
        "not recorded".to_owned()
    } else {
        value.to_owned()
    }
}

fn run_session_detail(
    project: &str,
    session: BoardRunSessionView,
    show_thinking_history: bool,
    toggle_thinking_history: Callback<()>,
) -> AnyView {
    let href = format!(
        "/projects/{}/automation/runs/{}/log",
        encode_path(project),
        session.run.id
    );
    let model = session
        .run
        .agent_model
        .clone()
        .unwrap_or_else(|| "default".to_owned());
    let reasoning = session
        .run
        .agent_reasoning_effort
        .map(|effort| effort.to_string())
        .unwrap_or_else(|| "default".to_owned());
    let memory_event = session
        .run
        .memory_event_id
        .map(|event_id| format!("MemoryChanged #{event_id}"));
    let token_usage = run_token_usage_text(&session.run);
    let summary = run_result_summary(&session.run);
    let origin = run_origin_label(&session.run);
    let work_item = run_work_item_link(project, session.run.work_item_id);
    let command = recorded_field(&session.run.command);
    let working_dir = recorded_field(&session.run.working_dir);
    let status_class = run_status_class(session.run.status);
    let output = view! {
        <RunOutput
            output=session.output.clone()
            active=session.active
            show_thinking_history
            toggle_thinking_history
        />
    };
    let developer_instructions = session
        .developer_instructions
        .unwrap_or_else(|| "No developer instructions have been written yet.".to_owned());
    let user_prompt = session
        .user_prompt
        .unwrap_or_else(|| "No user prompt has been written yet.".to_owned());

    view! {
        <article>
            <header class="run-detail-header">
                <div>
                    <h3>"Run #" {session.run.id}</h3>
                    <p>
                        {session.run.status.to_string()}
                        " · "
                        "cleanup "
                        {session.run.cleanup_status.to_string()}
                    </p>
                </div>
                <a class="button-link secondary-link" href=href>"Open"</a>
            </header>
            <dl class="run-detail-meta">
                {origin.map(|origin| view! {
                    <>
                        <dt>"source"</dt>
                        <dd>{origin}</dd>
                    </>
                })}
                {work_item.map(|work_item| view! {
                    <>
                        <dt>"item"</dt>
                        <dd>{work_item}</dd>
                    </>
                })}
                <dt>"model"</dt>
                <dd>{model}</dd>
                <dt>"reasoning"</dt>
                <dd>{reasoning}</dd>
                <dt>"tokens"</dt>
                <dd>{token_usage}</dd>
                {memory_event.map(|memory_event| view! {
                    <>
                        <dt>"memory"</dt>
                        <dd>{memory_event}</dd>
                    </>
                })}
                <dt>"command"</dt>
                <dd>{command}</dd>
                <dt>"working dir"</dt>
                <dd>{working_dir}</dd>
            </dl>
            <div class=format!("run-result {status_class}")>
                <h4>"Result"</h4>
                <p>{summary}</p>
            </div>
            <div class="run-detail-section">
                <h4>"Developer instructions"</h4>
                <pre>{developer_instructions}</pre>
            </div>
            <div class="run-detail-section">
                <h4>"User prompt"</h4>
                <pre>{user_prompt}</pre>
            </div>
            <div class="run-detail-section">
                <h4>"Output"</h4>
                {output}
            </div>
        </article>
    }
    .into_any()
}
