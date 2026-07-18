use std::{collections::HashMap, path::Path};

use crate::shared::view_models::{AgentRunOutputKind, AgentRunOutputPiece};
use leptonic::components::prelude::{Toggle, ToggleSize};
use leptos::prelude::*;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

const TOOL_OUTPUT_PREVIEW_LINES: usize = 2;

#[component]
pub(crate) fn RunOutput(
    output: Vec<AgentRunOutputPiece>,
    active: bool,
    show_thinking_history: bool,
    toggle_thinking_history: Callback<()>,
) -> AnyView {
    let current_reasoning_sequence = current_reasoning_sequence(&output, active);
    let entries = compact_run_output(output);
    if entries.is_empty() {
        return view! { <p class="muted">"No output has been written yet."</p> }.into_any();
    }

    let historical_reasoning_count = entries
        .iter()
        .filter(|entry| {
            entry.piece.kind == AgentRunOutputKind::Reasoning
                && Some(entry.piece.sequence) != current_reasoning_sequence
        })
        .count();
    let visible_entries = entries
        .into_iter()
        .filter(|entry| {
            entry.piece.kind != AgentRunOutputKind::Reasoning
                || Some(entry.piece.sequence) == current_reasoning_sequence
                || show_thinking_history
        })
        .collect::<Vec<_>>();
    let toggle = (historical_reasoning_count > 0).then(|| {
        let action = if show_thinking_history {
            "Hide thinking history"
        } else {
            "Show thinking history"
        };
        view! {
            <label class="thinking-history-toggle" title=action>
                <Toggle
                    state=Signal::derive(move || show_thinking_history)
                    set_state=Callback::new(move |_: bool| toggle_thinking_history.run(()))
                    size=ToggleSize::Small
                    attr:aria-label=action
                />
                <span>{format!("Thinking ({historical_reasoning_count})")}</span>
            </label>
        }
    });
    let pieces = visible_entries
        .into_iter()
        .map(|entry| {
            let current_reasoning = Some(entry.piece.sequence) == current_reasoning_sequence;
            run_output_entry_view(entry, current_reasoning)
        })
        .collect::<Vec<_>>();
    let empty = pieces
        .is_empty()
        .then(|| view! { <p class="muted run-output-empty">"No visible output yet."</p> });

    view! {
        <div class="run-output-controls">{toggle}</div>
        <div class="run-output">{empty}{pieces}</div>
    }
    .into_any()
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

pub(super) fn current_reasoning_sequence(
    output: &[AgentRunOutputPiece],
    active: bool,
) -> Option<u64> {
    if !active {
        return None;
    }
    let latest = output.iter().max_by_key(|piece| piece.sequence)?;
    (latest.kind == AgentRunOutputKind::Reasoning && output_piece_is_started(latest))
        .then_some(latest.sequence)
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

fn run_output_entry_view(entry: RunOutputEntry, current_reasoning: bool) -> AnyView {
    if entry.piece.kind == AgentRunOutputKind::Reasoning {
        return reasoning_output_entry_view(entry, current_reasoning);
    }
    if entry.piece.kind == AgentRunOutputKind::ToolCall
        && metadata_scalar(&entry.piece.metadata, "tool_type").as_deref() == Some("command")
    {
        return command_output_entry_view(entry);
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
    let details = if kind == AgentRunOutputKind::ToolCall {
        metadata_value_text(&metadata, "arguments")
            .filter(|value| !value.trim().is_empty())
            .map(|value| metadata_details_view("Details", value))
    } else {
        None
    };

    view! {
        <article class=format!("output-piece output-{kind_class}")>
            {header}
            {body_view}
            {details}
            {tool_output}
        </article>
    }
    .into_any()
}

fn reasoning_output_entry_view(entry: RunOutputEntry, current: bool) -> AnyView {
    if current {
        return view! {
            <article class="output-piece output-reasoning output-reasoning-current">
                <header class="output-piece-header">
                    <strong>"Thinking..."</strong>
                    <span class="thinking-live-indicator" aria-hidden="true"></span>
                </header>
            </article>
        }
        .into_any();
    }

    let title = output_entry_title(&entry);
    let body = entry.piece.body.clone();
    let body_view = reasoning_body_view(body);
    view! {
        <article class="output-piece output-reasoning output-reasoning-history">
            <header class="output-piece-header"><strong>{title}</strong></header>
            {body_view}
        </article>
    }
    .into_any()
}

fn reasoning_body_view(body: String) -> Option<AnyView> {
    if body.trim().is_empty() {
        return None;
    }
    let total_lines = body.lines().count().max(1);
    let (preview, truncated) = abbreviate_lines(&body, TOOL_OUTPUT_PREVIEW_LINES);
    if truncated {
        let remaining = total_lines.saturating_sub(TOOL_OUTPUT_PREVIEW_LINES);
        Some(
            view! {
                <details class="reasoning-history-details expandable-output">
                    <summary>
                        <span class="reasoning-output reasoning-output-preview">{preview}</span>
                        <span class="reasoning-output reasoning-output-full">{body}</span>
                        <span class="output-expand-hint">{format!("+{remaining} lines")}</span>
                    </summary>
                </details>
            }
            .into_any(),
        )
    } else {
        Some(view! { <div class="reasoning-output">{body}</div> }.into_any())
    }
}

fn command_output_entry_view(entry: RunOutputEntry) -> AnyView {
    let metadata = entry.piece.metadata.clone();
    let command = metadata_scalar(&metadata, "command").unwrap_or(entry.piece.body.clone());
    let presentation = command_presentation(&command);
    let title = match presentation.exploring_file.as_ref() {
        Some(path) => format!("Exploring {path}..."),
        None if output_piece_is_started(&entry.piece) => {
            format!("Running {}...", presentation.display)
        }
        None => format!("Ran {}", presentation.display),
    };
    let title_class = if presentation.exploring_file.is_some() {
        "command-summary command-exploring"
    } else {
        "command-summary"
    };
    let badges = output_entry_badges(&entry);
    let duration = output_entry_duration(&entry)
        .filter(|_| !output_piece_is_started(&entry.piece))
        .map(|duration| view! { <span class="output-piece-duration">{duration}</span> });
    let output = tool_output_text(&metadata).map(tool_output_view);

    view! {
        <article class="output-piece output-tool-call output-command">
            <header class="output-piece-header command-output-header">
                <code class=title_class title=command>{title}</code>
                {duration}
                {badges}
            </header>
            {output}
        </article>
    }
    .into_any()
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CommandPresentation {
    pub(super) display: String,
    pub(super) exploring_file: Option<String>,
}

pub(super) fn command_presentation(command: &str) -> CommandPresentation {
    let inner = unwrap_shell_command(command).unwrap_or_else(|| command.trim().to_owned());
    let display = inner.replace('\n', " ↵ ");
    let exploring_file = shlex::split(&inner).and_then(|tokens| exploring_file(&tokens));
    CommandPresentation {
        display,
        exploring_file,
    }
}

fn unwrap_shell_command(command: &str) -> Option<String> {
    let tokens = shlex::split(command)?;
    if tokens.len() != 3 || tokens[1] != "-lc" {
        return None;
    }
    let shell = Path::new(&tokens[0]).file_name()?.to_str()?;
    matches!(shell, "sh" | "bash" | "zsh").then(|| tokens[2].trim().to_owned())
}

fn exploring_file(tokens: &[String]) -> Option<String> {
    if tokens.iter().any(|token| shell_control_token(token)) {
        return None;
    }
    let command = Path::new(tokens.first()?).file_name()?.to_str()?;
    let candidate = match command {
        "cat" => match tokens {
            [_, path] => Some(path.as_str()),
            [_, separator, path] if separator == "--" => Some(path.as_str()),
            _ => None,
        },
        "sed" => match tokens {
            [_, quiet, _, path] if quiet == "-n" || quiet == "--silent" => Some(path.as_str()),
            [_, quiet, _, separator, path]
                if (quiet == "-n" || quiet == "--silent") && separator == "--" =>
            {
                Some(path.as_str())
            }
            _ => None,
        },
        "head" | "tail" => head_or_tail_file(&tokens[1..]),
        _ => None,
    }?;
    concrete_file_path(candidate).then(|| candidate.to_owned())
}

fn head_or_tail_file(arguments: &[String]) -> Option<&str> {
    match arguments {
        [path] => Some(path),
        [separator, path] if separator == "--" => Some(path),
        [option, _, path] if option == "-n" || option == "-c" => Some(path),
        [option, path]
            if option.strip_prefix('-').is_some_and(|value| {
                !value.is_empty() && value.chars().all(|c| c.is_ascii_digit())
            }) =>
        {
            Some(path)
        }
        [option, path] if option.starts_with("--lines=") || option.starts_with("--bytes=") => {
            Some(path)
        }
        _ => None,
    }
}

fn shell_control_token(token: &str) -> bool {
    matches!(
        token,
        "|" | "||" | "&&" | ";" | "&" | ">" | ">>" | "<" | "<<"
    ) || token.starts_with("2>")
}

fn concrete_file_path(path: &str) -> bool {
    !path.is_empty()
        && path != "-"
        && !path.starts_with('-')
        && !path.contains([
            '$', '`', '*', '?', '[', ']', '{', '}', '|', '&', ';', '>', '<',
        ])
        && !path.contains("$(")
}

fn output_entry_should_show_header(entry: &RunOutputEntry) -> bool {
    entry.piece.kind != AgentRunOutputKind::ModelMessage
        || !matches!(entry.piece.title.as_str(), "model output" | "final answer")
}

pub(super) fn output_entry_title(entry: &RunOutputEntry) -> String {
    match entry.piece.kind {
        AgentRunOutputKind::Reasoning => output_entry_duration(entry)
            .map(|duration| format!("Thought for {duration}"))
            .unwrap_or_else(|| "Thought".to_owned()),
        AgentRunOutputKind::ToolCall => tool_call_entry_title(entry),
        AgentRunOutputKind::FileChange => timed_title(
            entry,
            "Changing files",
            "Changed files",
            "File change failed",
        ),
        AgentRunOutputKind::ModelMessage
        | AgentRunOutputKind::Error
        | AgentRunOutputKind::System
        | AgentRunOutputKind::Legacy => entry.piece.title.clone(),
    }
}

fn tool_call_entry_title(entry: &RunOutputEntry) -> String {
    match metadata_scalar(&entry.piece.metadata, "tool_type").as_deref() {
        Some("command") => timed_title(entry, "Running command", "Ran command", "Command failed"),
        Some("mcp") => timed_title(entry, "Running MCP tool", "Ran MCP tool", "MCP tool failed"),
        Some("dynamic") => {
            let tool = metadata_scalar(&entry.piece.metadata, "tool")
                .filter(|tool| !tool.trim().is_empty())
                .unwrap_or_else(|| "dynamic tool".to_owned());
            timed_title(
                entry,
                &format!("Running {tool}"),
                &format!("Ran {tool}"),
                &format!("{tool} failed"),
            )
        }
        Some("collaboration") => timed_title(
            entry,
            "Running collaboration tool",
            "Ran collaboration tool",
            "Collaboration tool failed",
        ),
        _ => timed_title(entry, "Running tool", "Ran tool", "Tool failed"),
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
        Some(duration) if !output_piece_is_started(&entry.piece) => format!("{base} in {duration}"),
        _ => base.to_owned(),
    }
}

fn output_entry_badges(entry: &RunOutputEntry) -> Vec<AnyView> {
    let mut badges = Vec::new();
    if let Some(status) = output_status_label(&entry.piece) {
        badges.push(view! { <span class="output-piece-badge">{status}</span> }.into_any());
    }
    if let Some(exit_code) = metadata_scalar(&entry.piece.metadata, "exit_code")
        && exit_code != "0"
    {
        badges.push(
            view! { <span class="output-piece-badge output-piece-badge-failed">{"exit "}{exit_code}</span> }
                .into_any(),
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
    let total_lines = output.lines().count().max(1);
    let (preview, truncated) = abbreviate_lines(&output, TOOL_OUTPUT_PREVIEW_LINES);
    let diff = looks_like_diff(&output);
    if truncated {
        let remaining = total_lines.saturating_sub(TOOL_OUTPUT_PREVIEW_LINES);
        let preview = formatted_output_pre("tool-output-preview", preview, diff);
        let output = formatted_output_pre("tool-output-full", output, diff);
        view! {
            <details class="tool-output-block expandable-output">
                <summary>
                    {preview}
                    {output}
                    <span class="output-expand-hint">{format!("+{remaining} lines")}</span>
                </summary>
            </details>
        }
        .into_any()
    } else {
        let output = formatted_output_pre("tool-output-full", output, diff);
        view! { <div class="tool-output-block expanded">{output}</div> }.into_any()
    }
}

fn metadata_details_view(label: &'static str, value: String) -> AnyView {
    view! {
        <details class="tool-metadata-block compact-output-details">
            <summary>{label}</summary>
            <pre>{value}</pre>
        </details>
    }
    .into_any()
}

fn formatted_output_pre(class: &'static str, output: String, diff: bool) -> AnyView {
    if diff {
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
    let preview = lines
        .by_ref()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");
    let truncated = lines.next().is_some();
    (preview, truncated)
}
