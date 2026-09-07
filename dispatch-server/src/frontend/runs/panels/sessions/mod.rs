use crate::frontend::queries::query_resource;
use dispatch_types::RunSummaryView;
mod selection;

use selection::RunSelection;

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use leptos::prelude::*;
use leptos_router::{
    NavigateOptions,
    hooks::{use_navigate, use_query_map},
};

use super::{
    run_item_label, run_origin_label, run_result_summary, run_session_detail, run_status_class,
    run_token_usage_label,
};
use crate::frontend::{
    items::components::encode_path,
    live_events::{refetch_on_live_event, run_detail_event_matches},
};

#[component]
pub(super) fn RunSessionsPanel(
    project: String,
    title: &'static str,
    #[prop(into)] status_note: Signal<Option<String>>,
    #[prop(into)] runs: Signal<Vec<RunSummaryView>>,
    sync_selection_with_url: bool,
    empty_message: &'static str,
) -> impl IntoView + 'static {
    let query = use_query_map();
    let route_selected_run_id = Memo::new(move |_| {
        query
            .read()
            .get("run")
            .and_then(|value| value.parse::<i64>().ok())
    });
    // URL selection is derived directly from the router. Local selection is only
    // used by embedded panels, which do not own the page's query string.
    let (local_run_id, set_local_run_id) = signal(None::<i64>);
    let run_ids = Memo::new(move |_| {
        runs.with(|runs| {
            runs.iter()
                .map(|summary| summary.run.id)
                .collect::<Vec<_>>()
        })
    });
    let selection = Memo::new(move |previous| {
        let requested = if sync_selection_with_url {
            route_selected_run_id.get()
        } else {
            local_run_id.get()
        };
        run_ids.with(|ids| RunSelection::resolve(requested, previous, ids))
    });
    let selected_run_id = Memo::new(move |_| selection.get().selected);
    let shown_thinking_history = RwSignal::new(HashSet::<i64>::new());

    let store = crate::frontend::runs::store::run_store();
    let initial_detail = match selected_run_id.get_untracked() {
        Some(run_id) => store.cached_detail_untracked(&project, run_id).map(Some),
        None => Some(None),
    };
    let project_for_detail_input = project.clone();
    let store_for_detail_cache = store.clone();
    let store_for_detail_load = store.clone();
    let store_for_refresh = store.clone();
    let store_for_seed = store.clone();
    let detail = query_resource(
        initial_detail,
        move || (project_for_detail_input.clone(), selected_run_id.get()),
        move |(project, run_id)| match run_id {
            Some(run_id) => store_for_detail_cache
                .cached_detail(project, *run_id)
                .map(Some),
            None => Some(None),
        },
        move |(project, run_id)| {
            let store = store_for_detail_load.clone();
            async move {
                match run_id {
                    Some(run_id) => store.load_detail(project, run_id).await.map(Some),
                    None => Ok(None),
                }
            }
        },
        move |(project, id), value| {
            if let (Some(id), Some(value)) = (id, value) {
                store_for_seed.seed_detail(project, id, value);
            }
        },
        move |(project, id)| {
            if let Some(id) = id {
                store_for_refresh.invalidate_detail(project, id);
            }
        },
    );
    let project_for_detail_events = project.clone();
    refetch_on_live_event(detail.refresh, move |event| {
        run_detail_event_matches(
            event,
            Some(project_for_detail_events.as_str()),
            selected_run_id.get_untracked(),
        )
    });

    let navigate = use_navigate();
    let selection_project = project.clone();
    let select_run = Callback::new(move |run_id: i64| {
        if selected_run_id.get_untracked() == Some(run_id) {
            return;
        }
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
        } else {
            set_local_run_id.set(Some(run_id));
        }
    });

    // Preserve row identity and use one indexed snapshot for O(1) row lookups.
    // Each row memo compares its own summary; changing one run does not rebuild
    // every button or move keyboard focus to a different run.
    let summaries = Memo::new(move |_| {
        runs.get()
            .into_iter()
            .map(|summary| (summary.run.id, Arc::new(summary)))
            .collect::<HashMap<_, _>>()
    });
    let detail_project = project.clone();
    let run_detail = move || {
        let selected_run_id = selected_run_id.get();
        let selected = detail
            .value
            .get()
            .flatten()
            .filter(|detail| selected_run_id == Some(detail.run.id));
        match (selected_run_id, selected) {
            (Some(run_id), Some(detail)) => {
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
                    detail,
                    show_thinking_history,
                    toggle_thinking_history,
                )
            }
            (Some(_), None) => view! { <p class="muted">"Loading run…"</p> }.into_any(),
            (None, _) => view! { <p class="muted">"No run selected."</p> }.into_any(),
        }
    };

    view! {
        <section class="automation">
            <div class="panel-heading">
                <h2>{title}</h2>
                {move || status_note.get().map(|note| view! { <p class="muted">{note}</p> })}
            </div>
            <div class="run-session-shell">
                <div class="run-session-list">
                    <Show when=move || run_ids.read().is_empty()>
                        <p class="muted">{empty_message}</p>
                    </Show>
                    <For
                        each=move || run_ids.get()
                        key=|run_id| *run_id
                        children=move |run_id| {
                            let summary = Memo::new(move |_| {
                                summaries.with(|summaries| summaries.get(&run_id).cloned())
                            });
                            view! { <RunSessionButton run_id summary selected_run_id select_run/> }
                        }
                    />
                </div>
                <aside class="run-session-detail">
                    {run_detail}
                </aside>
            </div>
        </section>
    }
}

#[component]
fn RunSessionButton(
    run_id: i64,
    summary: Memo<Option<Arc<RunSummaryView>>>,
    selected_run_id: Memo<Option<i64>>,
    select_run: Callback<i64>,
) -> impl IntoView {
    let selected = Memo::new(move |_| selected_run_id.get() == Some(run_id));
    view! {
        <button
            type="button"
            class=move || summary.with(|summary| {
                let status = summary.as_ref().map(|summary| run_status_class(summary.run.status)).unwrap_or_default();
                format!("run-session {status}{}", if selected.get() { " selected" } else { "" })
            })
            aria-pressed=move || selected.get()
            on:click=move |_| select_run.run(run_id)
        >
            <div class="session-head">
                <strong>"#" {run_id}</strong>
                <span>{move || summary.with(|summary| summary.as_ref().map(|summary| summary.run.status.to_string()))}</span>
                {move || summary.with(|summary| summary.as_ref().and_then(|summary| run_item_label(&summary.run)))
                    .map(|item| view! { <span>{item}</span> })}
                {move || summary.with(|summary| summary.as_ref().and_then(|summary| run_origin_label(&summary.run)))
                    .map(|origin| view! { <span>{origin}</span> })}
                {move || summary.with(|summary| summary.as_ref().and_then(|summary| summary.run.token_usage.map(run_token_usage_label)))
                    .map(|tokens| view! { <span>{tokens}</span> })}
                <Show when=move || summary.with(|summary| summary.as_ref().is_some_and(|summary| summary.active))>
                    <span class="live-badge">"active"</span>
                </Show>
            </div>
            <p>{move || summary.with(|summary| summary.as_ref().map(|summary| run_result_summary(&summary.run)))}</p>
        </button>
    }
}
