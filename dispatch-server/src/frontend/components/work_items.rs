use crate::{
    frontend::{pages::infer_dispatch_run_id, rich_text::rich_text_plain_text},
    shared::view_models::{WorkItemClaimSourceView, WorkItemStateView, WorkItemView},
};
use leptos::prelude::*;
use leptos_use::use_interval_fn;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[derive(Clone, Copy)]
pub(crate) struct WorkItemStatesContext {
    pub(crate) states: ReadSignal<Vec<WorkItemStateView>>,
    pub(crate) set_states: WriteSignal<Vec<WorkItemStateView>>,
}

pub(crate) fn provide_work_item_states_context(
    initial_states: Vec<WorkItemStateView>,
) -> WorkItemStatesContext {
    let (states, set_states) = signal(initial_states);
    let context = WorkItemStatesContext { states, set_states };
    provide_context(context);
    context
}

pub(crate) fn encode_path(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}

pub(crate) fn item_href(project: &str, item_id: i64) -> String {
    format!("/projects/{}/items/{}", encode_path(project), item_id)
}

pub(crate) fn state_label(item: &WorkItemView) -> &str {
    item.state.as_deref().unwrap_or("(no state)")
}

pub(crate) fn claim_badge(
    project: &str,
    agent: String,
    status: &'static str,
    claimed_at: Option<String>,
) -> AnyView {
    claim_badge_with_source(project, agent, status, claimed_at, None)
}

pub(crate) fn claim_badge_with_source(
    project: &str,
    agent: String,
    status: &'static str,
    claimed_at: Option<String>,
    claim_source: Option<WorkItemClaimSourceView>,
) -> AnyView {
    let elapsed = claim_elapsed_timer(claimed_at);
    let source_label = claim_source_label(claim_source.as_ref());
    let run_id = claim_source
        .as_ref()
        .map(|source| source.run_id)
        .or_else(|| infer_dispatch_run_id(&agent));
    if let Some(run_id) = run_id {
        let href = format!(
            "/projects/{}/automation/runs/{}/log",
            encode_path(project),
            run_id
        );
        return view! {
            <a class="claim-badge" href=href>
                <span class="claim-dot" aria-hidden="true"></span>
                <span>{status}</span>
                <span class="claim-agent">{agent}</span>
                {source_label.map(|source| view! {
                    <span class="claim-source" title="Automation source">{source}</span>
                })}
                {elapsed}
            </a>
        }
        .into_any();
    }

    view! {
        <div class="claim-badge">
            <span class="claim-dot" aria-hidden="true"></span>
            <span>{status}</span>
            <span class="claim-agent">{agent}</span>
            {source_label.map(|source| view! {
                <span class="claim-source" title="Automation source">{source}</span>
            })}
            {elapsed}
        </div>
    }
    .into_any()
}

fn claim_source_label(source: Option<&WorkItemClaimSourceView>) -> Option<String> {
    source.map(|source| {
        source
            .trigger_name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(|name| format!("via {name}"))
            .unwrap_or_else(|| "via direct run".to_owned())
    })
}

fn claim_elapsed_timer(claimed_at: Option<String>) -> AnyView {
    let Some(claimed_at) = claimed_at else {
        return ().into_any();
    };
    if claim_elapsed_seconds(&claimed_at).is_none() {
        return ().into_any();
    }

    let (tick, set_tick) = signal(0_u64);
    let _poll = use_interval_fn(
        move || {
            set_tick.update(|tick| *tick = tick.saturating_add(1));
        },
        1000,
    );
    view! {
        <span class="claim-elapsed" title="Time in progress">
            {move || {
                let _ = tick.get();
                claim_elapsed_label(&claimed_at).unwrap_or_default()
            }}
        </span>
    }
    .into_any()
}

fn claim_elapsed_label(claimed_at: &str) -> Option<String> {
    claim_elapsed_seconds(claimed_at).map(format_claim_elapsed_seconds)
}

fn claim_elapsed_seconds(claimed_at: &str) -> Option<i64> {
    claim_elapsed_seconds_at(claimed_at, OffsetDateTime::now_utc())
}

pub(super) fn claim_elapsed_seconds_at(claimed_at: &str, now: OffsetDateTime) -> Option<i64> {
    let claimed_at = OffsetDateTime::parse(claimed_at, &Rfc3339).ok()?;
    Some((now - claimed_at).whole_seconds().max(0))
}

pub(super) fn format_claim_elapsed_seconds(total_seconds: i64) -> String {
    let total_seconds = total_seconds.max(0);
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

pub(crate) fn format_label(key: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{key}={value}"),
        None => key.to_owned(),
    }
}

pub(crate) fn preview(value: &str) -> String {
    const MAX_PREVIEW_CHARS: usize = 140;
    let value = rich_text_plain_text(value);
    if value.chars().count() <= MAX_PREVIEW_CHARS {
        return value;
    }

    value.chars().take(MAX_PREVIEW_CHARS).collect::<String>() + "..."
}
