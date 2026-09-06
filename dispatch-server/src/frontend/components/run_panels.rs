mod sessions;

use sessions::RunSessionsPanel;

use super::{RunOutput, cached_query, encode_path};
use crate::{
    frontend::{
        live_events::{
            refetch_on_live_event, runs_section_event_matches, trigger_runs_event_matches,
        },
        pages::format_number,
        services::{automation_service, run_service},
    },
    shared::view_models::{
        AgentCommitOutcome, AgentRunStatus, AgentRunTokenUsageView, AgentRunView, RunLogView,
    },
};
use leptos::prelude::*;

#[component]
pub(crate) fn LiveRunsSection(project: String) -> impl IntoView + 'static {
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

    let status_note = Signal::derive(move || {
        section.value.with(|section| {
            section.as_ref().map(|section| {
                let controller = if section.automation_running {
                    "controller running"
                } else {
                    "controller stopped"
                };
                format!(
                    "{} running ({} mutating, {} read-only), {controller}",
                    section.running_runs,
                    section.running_mutating_runs,
                    section.running_read_only_runs,
                )
            })
        })
    });
    let purpose = RwSignal::new("all".to_owned());
    let runs = Memo::new(move |_| {
        section
            .value
            .with(|section| section.as_ref().map(|section| section.runs.clone()))
            .unwrap_or_default()
            .into_iter()
            .filter(|summary| match purpose.get().as_str() {
                "knowledge" => summary.run.knowledge_job_id.is_some(),
                "tasks" => summary.run.knowledge_job_id.is_none(),
                _ => true,
            })
            .collect::<Vec<_>>()
    });

    view! {
        <label>"Purpose "<select aria-label="Run purpose" on:change=move |e|purpose.set(event_target_value(&e))><option value="all">"All runs"</option><option value="tasks">"Work items"</option><option value="knowledge">"Knowledge jobs"</option></select></label>
        <RunSessionsPanel
            project=project
            title="Runs"
            status_note=status_note
            runs
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
    let runs = Memo::new(move |_| {
        trigger_runs
            .value
            .with(|result| result.as_ref().and_then(Clone::clone))
            .unwrap_or_default()
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
                        runs
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
    if let Some(job_id) = run.knowledge_job_id {
        return Some(format!("knowledge job #{job_id}"));
    }
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
    detail: RunLogView,
    show_thinking_history: bool,
    toggle_thinking_history: Callback<()>,
) -> AnyView {
    let href = format!(
        "/projects/{}/automation/runs/{}/log",
        encode_path(project),
        detail.run.id
    );
    let model = detail
        .run
        .agent_model
        .clone()
        .unwrap_or_else(|| "default".to_owned());
    let reasoning = detail
        .run
        .agent_reasoning_effort
        .map(|effort| effort.to_string())
        .unwrap_or_else(|| "default".to_owned());
    let token_usage = run_token_usage_text(&detail.run);
    let summary = run_result_summary(&detail.run);
    let origin = run_origin_label(&detail.run);
    let work_item = run_work_item_link(project, detail.run.work_item_id);
    let command = recorded_field(&detail.run.command);
    let working_dir = recorded_field(&detail.run.working_dir);
    let status_class = run_status_class(detail.run.status);
    let output = view! {
        <RunOutput
            output=detail.output.clone()
            active=detail.active
            show_thinking_history
            toggle_thinking_history
        />
    };
    let developer_instructions = detail
        .developer_instructions
        .unwrap_or_else(|| "No developer instructions have been written yet.".to_owned());
    let user_prompt = detail
        .user_prompt
        .unwrap_or_else(|| "No user prompt has been written yet.".to_owned());

    view! {
        <article>
            <header class="run-detail-header">
                <div>
                    <h3>"Run #" {detail.run.id}</h3>
                    <p>
                        {detail.run.status.to_string()}
                        " · "
                        "cleanup "
                        {detail.run.cleanup_status.to_string()}
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
