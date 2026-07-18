use crate::{
    frontend::{
        components::{
            ActivePage, RunOutput, TopBar, cached_query, encode_path, recorded_field,
            run_commit_outcome_label, run_origin_label, run_result_summary, run_status_class,
            run_token_usage_text, run_work_item_link,
        },
        live_events::{refetch_on_live_event, run_log_event_matches},
        pages::memory_event_ref_label,
        services::{project_cache, run_service},
    },
    shared::view_models::{CodexAppServerStatusView, ProjectView, RunLogView},
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params_map;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RunLogPage {
    pub projects: Vec<ProjectView>,
    pub active_project_names: Vec<String>,
    pub project: String,
    pub run_log: RunLogView,
    pub codex_status: CodexAppServerStatusView,
}

#[component]
pub fn PageRunLog() -> impl IntoView {
    let params = use_params_map();
    let project = params.read_untracked().get("project");
    let run_id = params
        .read_untracked()
        .get("run_id")
        .and_then(|value| value.parse::<i64>().ok());
    let project_for_loader = project.clone();
    let project_for_events = project.clone();
    let service = run_service();
    let initial = service.cached_log_untracked(&project, run_id);
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || (project_for_loader.clone(), run_id),
        move |(project, run_id)| service_for_cache.cached_log(project, *run_id),
        move |(project, run_id)| {
            let service = service_for_load.clone();
            let project = project.clone();
            async move { service.load_log(project, run_id).await }
        },
    );
    project_cache().track(result.value, |page| &page.projects);
    refetch_on_live_event(result.refresh, move |event| {
        run_log_event_matches(event, project_for_events.as_deref(), run_id)
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
            selected_project=Signal::derive({
                let project = project.clone();
                move || project.clone()
            })
            active=ActivePage::Board
            automation=Signal::derive(|| None)
            codex_status
        />
    };
    let board_href = project
        .as_deref()
        .map(|project| format!("/?project={}", encode_path(project)))
        .unwrap_or_else(|| "/".to_owned());
    let title = run_id
        .map(|run_id| format!("Run #{run_id}"))
        .unwrap_or_else(|| "Run log".to_owned());
    let show_thinking_history = RwSignal::new(false);
    let toggle_thinking_history = Callback::new(move |()| {
        show_thinking_history.update(|show| *show = !*show);
    });
    view! {
        <Title text="Run log"/>
        <div>
            {topbar}
            <main class="page-shell run-log">
                <section class="item-header">
                    <a href=board_href>"Board"</a>
                    <h1>{title}</h1>
                </section>
                {move || {
                    let show_thinking_history = show_thinking_history.get();
                    result.value.get().map(|page| view! {
                        <RunLogContent
                            page
                            show_thinking_history
                            toggle_thinking_history
                        />
                    })
                }}
            </main>
        </div>
    }
}

#[component]
pub(crate) fn RunLogContent(
    page: RunLogPage,
    show_thinking_history: bool,
    toggle_thinking_history: Callback<()>,
) -> impl IntoView {
    let RunLogPage {
        projects: _,
        active_project_names: _,
        project,
        run_log,
        codex_status: _,
    } = page;
    let summary = run_result_summary(&run_log.run);
    let origin = run_origin_label(&run_log.run);
    let work_item = run_work_item_link(&project, run_log.run.work_item_id);
    let command = recorded_field(&run_log.run.command);
    let working_dir = recorded_field(&run_log.run.working_dir);
    let status_class = run_status_class(run_log.run.status);
    let memory_event = run_log.memory_event.as_ref().map(memory_event_ref_label);
    let token_usage = run_token_usage_text(&run_log.run);
    let commit_outcome = run_commit_outcome_label(&run_log.run);
    let trigger_revision = run_log.run.trigger_revision_id;
    let personality_revision = run_log.run.personality_revision_id;
    let system_prompt_event = run_log.run.system_prompt_event_id;
    let input_hash = run_log.run.effective_input_sha256.clone();
    let timeout_seconds = run_log.run.effective_timeout_seconds;
    let concurrency_group = run_log.run.effective_concurrency_group.clone();
    let semantic_status = run_log.run.semantic_postcondition_status.as_storage();
    let semantic_failures = run_log.run.semantic_postcondition_failures.clone();
    let created_items = run_item_links(&project, run_log.created_items.clone());
    let modified_items = run_item_links(&project, run_log.modified_items.clone());
    let cancel_action = if run_log.active {
        Some(view! {
            <CancelRunButton project=project.clone() run_id=run_log.run.id/>
        })
    } else {
        None
    };
    let pr_url = run_log.run.pr_url.clone().map(|pr_url| {
        let href = pr_url.clone();
        view! {
            <>
                <dt>"pull request"</dt>
                <dd><a href=href>{pr_url}</a></dd>
            </>
        }
    });
    let output = view! {
        <RunOutput
            output=run_log.output.clone()
            active=run_log.active
            show_thinking_history
            toggle_thinking_history
        />
    };
    let developer_instructions = run_log
        .developer_instructions
        .unwrap_or_else(|| "No developer instructions have been written.".to_owned());
    let user_prompt = run_log
        .user_prompt
        .unwrap_or_else(|| "No user prompt has been written.".to_owned());

    view! {
        <>
                <section class="run-log-state">
                    <p>
                        {run_log.run.status.to_string()}
                        " · "
                        {summary.clone()}
                    </p>
                    <div class="run-log-actions">{cancel_action}</div>
                </section>
                <section>
                    <h2>"Run"</h2>
                    <dl>
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
                        <dt>"result"</dt>
                        <dd class=format!("run-result-inline {status_class}")>{summary}</dd>
                        <dt>"mutability"</dt>
                        <dd>{run_log.run.mutability.to_string()}</dd>
                        <dt>"command"</dt>
                        <dd>{command}</dd>
                        <dt>"working dir"</dt>
                        <dd>{working_dir}</dd>
                        <dt>"cleanup"</dt>
                        <dd>{run_log.run.cleanup_status.to_string()}</dd>
                        <dt>"tokens"</dt>
                        <dd>{token_usage}</dd>
                        <dt>"commit"</dt>
                        <dd>{commit_outcome}</dd>
                        <dt>"semantic postconditions"</dt>
                        <dd>{semantic_status}</dd>
                        {trigger_revision.map(|id| view! {
                            <><dt>"trigger revision"</dt><dd>"#" {id}</dd></>
                        })}
                        {personality_revision.map(|id| view! {
                            <><dt>"personality revision"</dt><dd>"#" {id}</dd></>
                        })}
                        {system_prompt_event.map(|id| view! {
                            <><dt>"system prompt event"</dt><dd>"#" {id}</dd></>
                        })}
                        {input_hash.map(|hash| view! {
                            <><dt>"effective input SHA-256"</dt><dd><code>{hash}</code></dd></>
                        })}
                        {timeout_seconds.map(|seconds| view! {
                            <><dt>"timeout"</dt><dd>{seconds} " seconds"</dd></>
                        })}
                        {concurrency_group.map(|group| view! {
                            <><dt>"concurrency group"</dt><dd>{group}</dd></>
                        })}
                        {memory_event.map(|memory_event| view! {
                            <>
                                <dt>"memory"</dt>
                                <dd>{memory_event}</dd>
                            </>
                        })}
                        {pr_url}
                    </dl>
                </section>
                {(!semantic_failures.is_empty()).then(|| view! {
                    <section class="semantic-postcondition-failures">
                        <h2>"Semantic postcondition failures"</h2>
                        <ul>
                            {semantic_failures.into_iter().map(|failure| view! {
                                <li>
                                    "Outcome " {failure.outcome_index} ": "
                                    {failure.assertion} " expected " {failure.expected}
                                    ", found " {failure.actual}
                                </li>
                            }).collect::<Vec<_>>()}
                        </ul>
                    </section>
                })}
                {(!created_items.is_empty()).then(|| view! {
                    <section><h2>"Created items"</h2><ul>{created_items}</ul></section>
                })}
                {(!modified_items.is_empty()).then(|| view! {
                    <section><h2>"Modified items"</h2><ul>{modified_items}</ul></section>
                })}
                <section>
                    <h2>"Developer instructions"</h2>
                    <pre>{developer_instructions}</pre>
                </section>
                <section>
                    <h2>"User prompt"</h2>
                    <pre>{user_prompt}</pre>
                </section>
                <section>
                    <h2>"Output"</h2>
                    {output}
                </section>
        </>
    }
    .into_any()
}

#[component]
fn CancelRunButton(project: String, run_id: i64) -> impl IntoView {
    let service = run_service();
    let (pending, set_pending) = signal(false);
    let cancel = move |_| {
        if pending.get_untracked() {
            return;
        }
        set_pending.set(true);
        let service = service.clone();
        let project = project.clone();
        leptos::task::spawn_local(async move {
            if service.cancel_run(project, run_id).await.is_err() {
                set_pending.set(false);
            }
        });
    };

    view! {
        <button
            type="button"
            class="danger"
            disabled=move || pending.get()
            on:click=cancel
        >
            "Cancel run"
        </button>
    }
}

fn run_item_links(
    project: &str,
    items: Vec<crate::shared::view_models::WorkItemSummaryView>,
) -> Vec<AnyView> {
    items
        .into_iter()
        .map(|item| {
            let href = format!("/projects/{}/items/{}", encode_path(project), item.id);
            view! { <li><a href=href>"#" {item.id} " " {item.title}</a></li> }.into_any()
        })
        .collect()
}
