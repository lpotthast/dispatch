use std::collections::HashMap;

use crate::shared::view_models::{AgentRunOutputKind, AgentRunOutputPiece};
use leptos::prelude::*;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const TOOL_OUTPUT_PREVIEW_LINES: usize = 5;
const METADATA_PREVIEW_CHARS: usize = 320;

pub(crate) fn run_output_view(output: Vec<AgentRunOutputPiece>) -> AnyView {
    let entries = compact_run_output(output);
    if entries.is_empty() {
        return view! { <p class="muted">"No output has been written yet."</p> }.into_any();
    }
    let pieces = entries
        .into_iter()
        .map(run_output_entry_view)
        .collect::<Vec<_>>();
    view! { <div class="run-output">{pieces}</div> }.into_any()
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct RunOutputEntry {
    pub(super) start: Option<AgentRunOutputPiece>,
    pub(super) piece: AgentRunOutputPiece,
}

pub(super) fn compact_run_output(output: Vec<AgentRunOutputPiece>) -> Vec<RunOutputEntry> {
    let mut entries = Vec::new();
    let mut open_items = HashMap::new();

    for piece in output {
        if should_hide_output_piece(&piece) {
            continue;
        }

        if let Some(item_id) = piece.item_id.clone() {
            if output_piece_is_started(&piece) {
                open_items.insert(item_id, entries.len());
                entries.push(RunOutputEntry { start: None, piece });
                continue;
            }

            if let Some(index) = open_items.remove(&item_id) {
                let start = entries[index]
                    .start
                    .clone()
                    .or_else(|| Some(entries[index].piece.clone()));
                entries[index] = RunOutputEntry { start, piece };
                continue;
            }
        }

        entries.push(RunOutputEntry { start: None, piece });
    }

    entries
}

fn should_hide_output_piece(piece: &AgentRunOutputPiece) -> bool {
    if piece.kind != AgentRunOutputKind::System {
        return false;
    }

    matches!(
        piece.title.as_str(),
        "Codex app-server" | "thread started" | "turn started" | "turn completed" | "user message"
    )
}

fn run_output_entry_view(entry: RunOutputEntry) -> AnyView {
    if entry.piece.kind == AgentRunOutputKind::Reasoning {
        return reasoning_output_entry_view(entry);
    }

    let kind = entry.piece.kind;
    let kind_class = kind.as_storage().replace('_', "-");
    let title = output_entry_title(&entry);
    let body = entry.piece.body.clone();
    let metadata = entry.piece.metadata.clone();
    let badges = output_entry_badges(&entry);
    let header = output_entry_should_show_header(&entry).then(|| {
        view! {
            <header class="output-piece-header">
                <strong>{title}</strong>
                {badges}
            </header>
        }
    });
    let body_view = output_piece_body(kind, body);
    let tool_output = if kind == AgentRunOutputKind::ToolCall {
        tool_output_text(&metadata).map(tool_output_view)
    } else {
        None
    };
    let arguments = if kind == AgentRunOutputKind::ToolCall {
        metadata_value_text(&metadata, "arguments")
            .filter(|value| !value.trim().is_empty())
            .map(|value| expandable_metadata_view("arguments", value))
    } else {
        None
    };

    view! {
        <article class=format!("output-piece output-{kind_class}")>
            {header}
            {body_view}
            {arguments}
            {tool_output}
        </article>
    }
    .into_any()
}

fn reasoning_output_entry_view(entry: RunOutputEntry) -> AnyView {
    let title = output_entry_title(&entry);
    let body = entry.piece.body.clone();
    let body_view = output_piece_body(AgentRunOutputKind::Reasoning, body);

    view! {
        <article class="output-piece output-reasoning">
            <details class="reasoning-output-details">
                <summary>{title}</summary>
                {body_view}
            </details>
        </article>
    }
    .into_any()
}

fn output_entry_should_show_header(entry: &RunOutputEntry) -> bool {
    entry.piece.kind != AgentRunOutputKind::ModelMessage
        || !matches!(entry.piece.title.as_str(), "model output" | "final answer")
}

pub(super) fn output_entry_title(entry: &RunOutputEntry) -> String {
    match entry.piece.kind {
        AgentRunOutputKind::Reasoning => output_entry_duration(entry)
            .map(|duration| format!("thought for {duration}"))
            .unwrap_or_else(|| "thought".to_owned()),
        AgentRunOutputKind::ToolCall => tool_call_entry_title(entry),
        AgentRunOutputKind::FileChange => timed_title(
            entry,
            "file change running",
            "file change completed",
            "file change failed",
        ),
        AgentRunOutputKind::ModelMessage
        | AgentRunOutputKind::Error
        | AgentRunOutputKind::System
        | AgentRunOutputKind::Legacy => entry.piece.title.clone(),
    }
}

fn tool_call_entry_title(entry: &RunOutputEntry) -> String {
    match metadata_scalar(&entry.piece.metadata, "tool_type").as_deref() {
        Some("command") => timed_title(
            entry,
            "running command",
            "command completed",
            "command failed",
        ),
        Some("mcp") => timed_title(
            entry,
            "running MCP tool",
            "MCP tool completed",
            "MCP tool failed",
        ),
        Some("dynamic") => {
            let tool = metadata_scalar(&entry.piece.metadata, "tool")
                .filter(|tool| !tool.trim().is_empty())
                .unwrap_or_else(|| "dynamic tool".to_owned());
            timed_title(
                entry,
                &format!("running {tool}"),
                &format!("{tool} completed"),
                &format!("{tool} failed"),
            )
        }
        Some("collaboration") => timed_title(
            entry,
            "running collaboration tool",
            "collaboration tool completed",
            "collaboration tool failed",
        ),
        _ => timed_title(entry, "running tool", "tool completed", "tool failed"),
    }
}

fn timed_title(entry: &RunOutputEntry, running: &str, completed: &str, failed: &str) -> String {
    let base = if output_piece_is_started(&entry.piece) {
        running
    } else if output_piece_failed(&entry.piece) {
        failed
    } else {
        completed
    };

    match output_entry_duration(entry) {
        Some(duration) if !output_piece_is_started(&entry.piece) => {
            format!("{base} in {duration}")
        }
        _ => base.to_owned(),
    }
}

fn output_entry_badges(entry: &RunOutputEntry) -> Vec<AnyView> {
    let mut badges = Vec::new();
    if let Some(status) = output_status_label(&entry.piece) {
        badges.push(view! { <span class="output-piece-badge">{status}</span> }.into_any());
    }
    if let Some(exit_code) = metadata_scalar(&entry.piece.metadata, "exit_code") {
        badges.push(
            view! { <span class="output-piece-badge">{"exit "}{exit_code}</span> }.into_any(),
        );
    }
    badges
}

fn output_status_label(piece: &AgentRunOutputPiece) -> Option<&'static str> {
    let status = normalized_output_status(piece)?;
    match status.as_str() {
        "started" | "inprogress" => Some("running"),
        "failed" => Some("failed"),
        "declined" => Some("declined"),
        _ => None,
    }
}

fn output_piece_is_started(piece: &AgentRunOutputPiece) -> bool {
    matches!(
        normalized_output_status(piece).as_deref(),
        Some("started" | "inprogress")
    )
}

fn output_piece_failed(piece: &AgentRunOutputPiece) -> bool {
    matches!(normalized_output_status(piece).as_deref(), Some("failed"))
}

fn normalized_output_status(piece: &AgentRunOutputPiece) -> Option<String> {
    metadata_scalar(&piece.metadata, "status").map(|status| {
        status
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    })
}

fn output_entry_duration(entry: &RunOutputEntry) -> Option<String> {
    output_entry_duration_seconds(entry).map(format_output_duration)
}

pub(super) fn output_entry_duration_seconds(entry: &RunOutputEntry) -> Option<i64> {
    metadata_duration_seconds(&entry.piece.metadata).or_else(|| {
        let start = entry.start.as_ref()?;
        timestamp_duration_seconds(&start.timestamp, &entry.piece.timestamp)
    })
}

fn metadata_duration_seconds(metadata: &serde_json::Value) -> Option<i64> {
    let milliseconds = metadata
        .get("duration_ms")
        .or_else(|| metadata.get("durationMs"))?
        .as_i64()
        .or_else(|| {
            metadata
                .get("duration_ms")
                .or_else(|| metadata.get("durationMs"))?
                .as_u64()
                .and_then(|value| i64::try_from(value).ok())
        })?;
    Some(milliseconds.saturating_add(999).max(0) / 1000)
}

fn timestamp_duration_seconds(start: &str, end: &str) -> Option<i64> {
    let start = OffsetDateTime::parse(start, &Rfc3339).ok()?;
    let end = OffsetDateTime::parse(end, &Rfc3339).ok()?;
    Some((end - start).whole_seconds().max(0))
}

pub(super) fn format_output_duration(total_seconds: i64) -> String {
    let total_seconds = total_seconds.max(0);
    if total_seconds == 0 {
        return "<1s".to_owned();
    }

    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn output_piece_body(kind: AgentRunOutputKind, body: String) -> AnyView {
    if body.trim().is_empty() {
        return ().into_any();
    }
    let class = match kind {
        AgentRunOutputKind::ModelMessage => "output-piece-body model-output",
        AgentRunOutputKind::Reasoning => "output-piece-body reasoning-output",
        AgentRunOutputKind::ToolCall | AgentRunOutputKind::FileChange => {
            "output-piece-body tool-call-body"
        }
        AgentRunOutputKind::Error => "output-piece-body error-output",
        AgentRunOutputKind::System | AgentRunOutputKind::Legacy => {
            "output-piece-body system-output"
        }
    };
    view! { <div class=class>{body}</div> }.into_any()
}

fn metadata_scalar(metadata: &serde_json::Value, key: &str) -> Option<String> {
    match metadata.get(key)? {
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Bool(value) => Some(value.to_string()),
        serde_json::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn tool_output_text(metadata: &serde_json::Value) -> Option<String> {
    ["output", "result", "content_items", "error"]
        .into_iter()
        .filter_map(|key| metadata_value_text(metadata, key))
        .find(|value| !value.trim().is_empty())
}

fn metadata_value_text(metadata: &serde_json::Value, key: &str) -> Option<String> {
    let value = metadata.get(key)?;
    match value {
        serde_json::Value::Null => None,
        serde_json::Value::String(value) => Some(value.clone()),
        serde_json::Value::Array(values) if values.is_empty() => None,
        serde_json::Value::Object(values) if values.is_empty() => None,
        value => serde_json::to_string_pretty(value).ok(),
    }
}

fn tool_output_view(output: String) -> AnyView {
    let (preview, truncated) = abbreviate_lines(&output, TOOL_OUTPUT_PREVIEW_LINES);
    let output_label = if looks_like_diff(&output) {
        "diff"
    } else {
        "output"
    };
    if truncated {
        let preview = formatted_output_pre("tool-output-preview", preview);
        let output = formatted_output_pre("tool-output-full", output);
        view! {
            <details class="tool-output-block">
                <summary>
                    <span class="tool-output-label">{output_label}</span>
                    {preview}
                </summary>
                {output}
            </details>
        }
        .into_any()
    } else {
        let output = formatted_output_pre("tool-output-full", output);
        view! {
            <div class="tool-output-block expanded">
                <span class="tool-output-label">{output_label}</span>
                {output}
            </div>
        }
        .into_any()
    }
}

fn expandable_metadata_view(label: &'static str, value: String) -> AnyView {
    let (preview, truncated) = abbreviate_chars(&value, METADATA_PREVIEW_CHARS);
    if truncated {
        view! {
            <details class="tool-metadata-block">
                <summary>
                    <span>{label}</span>
                    <span>{preview}</span>
                </summary>
                <pre>{value}</pre>
            </details>
        }
        .into_any()
    } else {
        view! {
            <div class="tool-metadata-block compact">
                <span>{label}</span>
                <code>{value}</code>
            </div>
        }
        .into_any()
    }
}

fn formatted_output_pre(class: &'static str, output: String) -> AnyView {
    if looks_like_diff(&output) {
        let lines = diff_line_views(&output);
        view! { <pre class=format!("{class} diff-output")>{lines}</pre> }.into_any()
    } else {
        view! { <pre class=class>{output}</pre> }.into_any()
    }
}

fn diff_line_views(output: &str) -> Vec<AnyView> {
    output
        .lines()
        .map(|line| {
            let class = diff_line_class(line);
            let line = if line.is_empty() { " " } else { line }.to_owned();
            view! { <span class=class>{line}</span> }.into_any()
        })
        .collect()
}

pub(super) fn diff_line_class(line: &str) -> &'static str {
    if line.starts_with('+') && !line.starts_with("+++") {
        "diff-line diff-added"
    } else if line.starts_with('-') && !line.starts_with("---") {
        "diff-line diff-removed"
    } else if line.starts_with("@@") {
        "diff-line diff-hunk"
    } else {
        "diff-line diff-context"
    }
}

pub(super) fn looks_like_diff(output: &str) -> bool {
    let mut has_diff_marker = false;
    let mut has_changed_line = false;

    for line in output.lines() {
        if line.starts_with("diff --git") || line.starts_with("@@") {
            has_diff_marker = true;
        }
        if (line.starts_with('+') && !line.starts_with("+++"))
            || (line.starts_with('-') && !line.starts_with("---"))
        {
            has_changed_line = true;
        }
    }

    has_diff_marker && has_changed_line
}

pub(super) fn abbreviate_lines(value: &str, max_lines: usize) -> (String, bool) {
    let mut lines = value.lines();
    let mut preview = lines
        .by_ref()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    let truncated = lines.next().is_some();
    if truncated {
        preview.push_str("\n...");
    }
    (preview, truncated)
}

fn abbreviate_chars(value: &str, max_chars: usize) -> (String, bool) {
    let mut chars = value.chars();
    let mut preview = chars.by_ref().take(max_chars).collect::<String>();
    let truncated = chars.next().is_some();
    if truncated {
        preview.push_str("...");
    }
    (preview, truncated)
}
