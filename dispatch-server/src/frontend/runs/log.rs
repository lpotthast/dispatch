use crate::frontend::queries::query_resource;
use crate::frontend::runs::types::RunDetail;
use crate::frontend::{
    items::components::encode_path,
    live_events::{refetch_on_live_event, run_log_event_matches},
    runs::{
        output::RunOutput,
        panels::{
            recorded_field, run_commit_outcome_label, run_origin_label, run_result_summary,
            run_status_class, run_token_usage_text, run_work_item_link,
        },
        service::run_service,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_router::hooks::use_params;

#[component]
pub fn PageRunLog() -> impl IntoView {
    let params = use_params::<crate::frontend::routes::RunLogParams>();
    let project = Memo::new(move |_| params.get().ok().and_then(|params| params.project));
    let run_id = Memo::new(move |_| {
        params
            .get()
            .ok()
            .and_then(|params| params.run_id)
            .filter(|id| *id > 0)
    });
    let store = crate::frontend::runs::store::run_store();
    let initial = store.cached_log_untracked(&project.get_untracked(), run_id.get_untracked());
    let store_for_cache = store.clone();
    let store_for_load = store.clone();
    let store_for_refresh = store.clone();
    let store_for_seed = store.clone();
    let result = query_resource(
        initial,
        move || (project.get(), run_id.get()),
        move |(project, run_id)| store_for_cache.cached_log(project, *run_id),
        move |(project, run_id)| {
            let store = store_for_load.clone();
            let project = project.clone();
            async move { store.load_log(project, run_id).await }
        },
        move |(project, id), value| store_for_seed.seed_log(project, id, value),
        move |(project, id)| store_for_refresh.invalidate_log(project, id),
    );
    refetch_on_live_event(result.refresh, move |event| {
        run_log_event_matches(event, project.get().as_deref(), run_id.get())
    });
    let board_href = move || {
        project
            .get()
            .as_deref()
            .map(|project| format!("/?project={}", encode_path(project)))
            .unwrap_or_else(|| "/".to_owned())
    };
    let title = move || {
        run_id
            .get()
            .map(|run_id| format!("Run #{run_id}"))
            .unwrap_or_else(|| "Run log".to_owned())
    };
    let show_thinking_history = RwSignal::new(false);
    Effect::new(move |_| {
        project.track();
        run_id.track();
        show_thinking_history.set(false);
    });
    let toggle_thinking_history = Callback::new(move |()| {
        show_thinking_history.update(|show| *show = !*show);
    });
    view! {
            <Title text="Run log"/>
            <div>
                <main class="page-shell run-log">
                    <section class="item-header">
                        <a href=board_href>"Board"</a>
                        <h1>{title}</h1>
                    </section>
                    <Transition>
                    <For each=move || result.value.get() key=|page| (page.project.clone(), page.run_log.run.id) children=move |initial| {
                        let identity = (initial.project.clone(), initial.run_log.run.id);
                        let page = Signal::derive(move || result.value.get().filter(|page| (page.project.clone(), page.run_log.run.id) == identity).unwrap_or_else(|| initial.clone()));
                        view! { <RunLogContent page show_thinking_history=show_thinking_history.into() toggle_thinking_history/> }
                    }/>
                </Transition>
    <crate::frontend::components::QueryFeedback pending=result.pending error=result.error refresh=result.refresh/>
                </main>
            </div>
        }
}

#[component]
pub(crate) fn RunLogContent(
    page: Signal<RunDetail>,
    show_thinking_history: Signal<bool>,
    toggle_thinking_history: Callback<()>,
) -> impl IntoView {
    view! {
        {move || view! { <RunLogMetadata page=page.get()/> }}
        <section><h2>"Output"</h2>
            <RunOutput output=Signal::derive(move || page.get().run_log.output) active=Signal::derive(move || page.get().run_log.active) show_thinking_history toggle_thinking_history/>
        </section>
    }
}

#[component]
fn RunLogMetadata(page: RunDetail) -> impl IntoView {
    let RunDetail { project, run_log } = page;
    let knowledge_job_link = run_log.run.knowledge_job_id.map(|id|view!{<a href=format!("/knowledge?project={}&job={id}",urlencoding::encode(&project))>{format!("Knowledge job #{id}")}</a>});
    let summary = run_result_summary(&run_log.run);
    let origin = run_origin_label(&run_log.run);
    let work_item = run_work_item_link(&project, run_log.run.work_item_id);
    let command = recorded_field(&run_log.run.command);
    let working_dir = recorded_field(&run_log.run.working_dir);
    let status_class = run_status_class(run_log.run.status);
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
                    <div class="run-log-actions">{knowledge_job_link}{cancel_action}</div>
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

        </>
    }
    .into_any()
}

#[component]
fn CancelRunButton(project: String, run_id: i64) -> impl IntoView {
    let service = run_service();
    let cancel_action = Action::new(move |_: &()| {
        let service = service.clone();
        let project = project.clone();
        async move { service.cancel_run(project, run_id).await }
    });
    let pending = cancel_action.pending();
    let cancel = move |_| {
        if pending.get_untracked() {
            return;
        }
        cancel_action.dispatch(());
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
