use std::{fs, path::Path};

use codex_app_server_sdk::{ReviewModeItem, ThreadEvent, ThreadItem};
use rootcause::{Result, prelude::*};

use crate::{
    backend::{process_sessions::ProcessSessionRegistry, storage::utc_now},
    shared::view_models::{
        AgentRunOutputKind, AgentRunOutputLog, AgentRunOutputPiece, AgentRunTokenUsageView,
    },
};

const MAX_AGENT_OUTPUT_BYTES: usize = 1024 * 1024;

pub(crate) struct OutputPieceDraft {
    pub(crate) kind: AgentRunOutputKind,
    pub(crate) item_id: Option<String>,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) metadata: serde_json::Value,
}

pub(crate) async fn read_run_output(path: Option<&str>) -> Result<Vec<AgentRunOutputPiece>> {
    let Some(body) = read_optional_text(path).await? else {
        return Ok(Vec::new());
    };
    if let Ok(log) = serde_json::from_str::<AgentRunOutputLog>(&body) {
        return Ok(log.pieces);
    }
    Ok(vec![new_output_piece(
        1,
        AgentRunOutputKind::Legacy,
        None,
        "legacy log",
        body,
        serde_json::json!({ "format": "plain_text" }),
    )])
}

pub(crate) async fn read_run_token_usage(path: Option<&str>) -> Option<AgentRunTokenUsageView> {
    let path = path?;
    let Ok(body) = tokio::fs::read_to_string(path).await else {
        return None;
    };
    let Ok(log) = serde_json::from_str::<AgentRunOutputLog>(&body) else {
        return None;
    };
    token_usage_from_output_pieces(&log.pieces)
}

pub(crate) fn write_run_output_log(path: &Path, pieces: &[AgentRunOutputPiece]) -> Result<()> {
    let log = AgentRunOutputLog {
        schema_version: 1,
        pieces: pieces.to_vec(),
    };
    let body = serde_json::to_string_pretty(&log).context("failed to encode automation output")?;
    Ok(fs::write(path, body).context_with(|| format!("failed to write {}", path.display()))?)
}

pub(crate) async fn push_codex_output_piece(
    sessions: &Option<ProcessSessionRegistry>,
    run_id: i64,
    output: &mut Vec<AgentRunOutputPiece>,
    draft: OutputPieceDraft,
) {
    let piece = new_output_piece(
        output.last().map(|piece| piece.sequence + 1).unwrap_or(1),
        draft.kind,
        draft.item_id,
        draft.title,
        draft.body,
        draft.metadata,
    );
    output.push(piece.clone());
    trim_output_pieces(output, MAX_AGENT_OUTPUT_BYTES);
    if let Some(registry) = sessions {
        registry.append_output_piece(run_id, piece).await;
    }
}

pub(crate) fn new_output_piece(
    sequence: u64,
    kind: AgentRunOutputKind,
    item_id: Option<String>,
    title: impl Into<String>,
    body: impl Into<String>,
    metadata: serde_json::Value,
) -> AgentRunOutputPiece {
    AgentRunOutputPiece {
        sequence,
        timestamp: utc_now(),
        kind,
        source: "codex".to_owned(),
        item_id,
        title: title.into(),
        body: body.into(),
        metadata,
    }
}

pub(crate) fn thread_event_output_piece(event: &ThreadEvent) -> Option<OutputPieceDraft> {
    match event {
        ThreadEvent::ThreadStarted { thread_id } => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: None,
            title: "thread started".to_owned(),
            body: thread_id.clone(),
            metadata: serde_json::json!({ "thread_id": thread_id }),
        }),
        ThreadEvent::TurnStarted => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: None,
            title: "turn started".to_owned(),
            body: String::new(),
            metadata: serde_json::json!({}),
        }),
        ThreadEvent::TurnCompleted { usage } => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: None,
            title: "turn completed".to_owned(),
            body: usage
                .as_ref()
                .map(|usage| {
                    format!(
                        "input={} cached_input={} output={}",
                        usage.input_tokens, usage.cached_input_tokens, usage.output_tokens
                    )
                })
                .unwrap_or_default(),
            metadata: match usage {
                Some(usage) => serde_json::json!({
                    "usage": {
                        "input_tokens": usage.input_tokens,
                        "cached_input_tokens": usage.cached_input_tokens,
                        "output_tokens": usage.output_tokens,
                    }
                }),
                None => serde_json::json!({}),
            },
        }),
        ThreadEvent::TurnFailed { error } => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::Error,
            item_id: None,
            title: "turn failed".to_owned(),
            body: error.message.clone(),
            metadata: serde_json::json!({ "message": &error.message }),
        }),
        ThreadEvent::Error { message } => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::Error,
            item_id: None,
            title: "stream error".to_owned(),
            body: message.clone(),
            metadata: serde_json::json!({ "message": message }),
        }),
        ThreadEvent::ItemStarted { item } => started_thread_item_piece(item),
        ThreadEvent::ItemUpdated { .. } => None,
        ThreadEvent::ItemCompleted { item } => completed_thread_item_piece(item),
    }
}

pub(crate) fn update_response_candidates(
    item: &ThreadItem,
    final_answer: &mut Option<String>,
    fallback_answer: &mut Option<String>,
) {
    let ThreadItem::AgentMessage(message) = item else {
        return;
    };
    if message.is_final_answer() {
        *final_answer = Some(message.text.clone());
    } else {
        *fallback_answer = Some(message.text.clone());
    }
}

async fn read_optional_text(path: Option<&str>) -> Result<Option<String>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let body = match tokio::fs::read_to_string(path).await {
        Ok(body) => body,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context_with(|| format!("failed to read {}", path))?,
    };
    Ok(Some(body))
}

fn token_usage_from_output_pieces(
    pieces: &[AgentRunOutputPiece],
) -> Option<AgentRunTokenUsageView> {
    pieces
        .iter()
        .rev()
        .find_map(|piece| token_usage_from_metadata(&piece.metadata))
}

fn token_usage_from_metadata(metadata: &serde_json::Value) -> Option<AgentRunTokenUsageView> {
    let usage = metadata.get("usage")?;
    let input_tokens = usage_i64(usage, &["input_tokens", "inputTokens"])?;
    let cached_input_tokens =
        usage_i64(usage, &["cached_input_tokens", "cachedInputTokens"]).unwrap_or_default();
    let output_tokens = usage_i64(usage, &["output_tokens", "outputTokens"])?;
    Some(AgentRunTokenUsageView {
        input_tokens,
        cached_input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
    })
}

fn usage_i64(value: &serde_json::Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|key| value.get(*key)?.as_i64())
}

fn trim_output_pieces(pieces: &mut Vec<AgentRunOutputPiece>, max_bytes: usize) {
    while pieces.len() > 1 && output_pieces_size(pieces) > max_bytes {
        pieces.remove(0);
    }
}

fn output_pieces_size(pieces: &[AgentRunOutputPiece]) -> usize {
    pieces.iter().map(output_piece_size).sum()
}

fn output_piece_size(piece: &AgentRunOutputPiece) -> usize {
    piece.timestamp.len()
        + piece.source.len()
        + piece.item_id.as_deref().map(str::len).unwrap_or_default()
        + piece.title.len()
        + piece.body.len()
        + piece.metadata.to_string().len()
}

fn started_thread_item_piece(item: &ThreadItem) -> Option<OutputPieceDraft> {
    match item {
        ThreadItem::CommandExecution(command) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(command.id.clone()),
            title: "command started".to_owned(),
            body: command.command.clone(),
            metadata: serde_json::json!({
                "tool_type": "command",
                "status": "started",
                "command": &command.command,
            }),
        }),
        ThreadItem::McpToolCall(call) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(call.id.clone()),
            title: format!("mcp tool started: {}/{}", call.server, call.tool),
            body: format!("{}/{}", call.server, call.tool),
            metadata: serde_json::json!({
                "tool_type": "mcp",
                "status": "started",
                "server": &call.server,
                "tool": &call.tool,
                "arguments": &call.arguments,
            }),
        }),
        ThreadItem::DynamicToolCall(call) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(call.id.clone()),
            title: format!("dynamic tool started: {}", call.tool),
            body: call.tool.clone(),
            metadata: serde_json::json!({
                "tool_type": "dynamic",
                "status": "started",
                "tool": &call.tool,
                "arguments": &call.arguments,
            }),
        }),
        ThreadItem::CollabToolCall(call) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(call.id.clone()),
            title: format!("collaboration tool started: {}", call.tool),
            body: call.tool.clone(),
            metadata: serde_json::json!({
                "tool_type": "collaboration",
                "status": "started",
                "tool": &call.tool,
                "sender_thread_id": &call.sender_thread_id,
                "receiver_thread_id": &call.receiver_thread_id,
                "new_thread_id": &call.new_thread_id,
                "prompt": &call.prompt,
                "agent_status": &call.agent_status,
            }),
        }),
        _ => None,
    }
}

fn completed_thread_item_piece(item: &ThreadItem) -> Option<OutputPieceDraft> {
    match item {
        ThreadItem::AgentMessage(message) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ModelMessage,
            item_id: Some(message.id.clone()),
            title: if message.is_final_answer() {
                "final answer".to_owned()
            } else {
                "model output".to_owned()
            },
            body: message.text.clone(),
            metadata: serde_json::json!({
                "phase": message.phase.map(|phase| phase.as_str()),
                "final_answer": message.is_final_answer(),
            }),
        }),
        ThreadItem::Plan(plan) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ModelMessage,
            item_id: Some(plan.id.clone()),
            title: "plan".to_owned(),
            body: plan.text.clone(),
            metadata: serde_json::json!({ "item_type": "plan" }),
        }),
        ThreadItem::Reasoning(reasoning) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::Reasoning,
            item_id: Some(reasoning.id.clone()),
            title: "reasoning".to_owned(),
            body: reasoning.text.clone(),
            metadata: serde_json::json!({}),
        }),
        ThreadItem::CommandExecution(command) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(command.id.clone()),
            title: format!("command {:?}", command.status),
            body: command.command.clone(),
            metadata: serde_json::json!({
                "tool_type": "command",
                "status": format!("{:?}", command.status),
                "command": &command.command,
                "exit_code": command.exit_code,
                "output": &command.aggregated_output,
            }),
        }),
        ThreadItem::FileChange(change) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::FileChange,
            item_id: Some(change.id.clone()),
            title: format!("file change {:?}", change.status),
            body: format!("{} file(s)", change.changes.len()),
            metadata: serde_json::json!({
                "status": format!("{:?}", change.status),
                "changes": change.changes.iter().map(|change| {
                    serde_json::json!({
                        "path": &change.path,
                        "kind": format!("{:?}", change.kind),
                    })
                }).collect::<Vec<_>>(),
            }),
        }),
        ThreadItem::McpToolCall(call) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(call.id.clone()),
            title: format!("mcp tool {:?}: {}/{}", call.status, call.server, call.tool),
            body: format!("{}/{}", call.server, call.tool),
            metadata: serde_json::json!({
                "tool_type": "mcp",
                "status": format!("{:?}", call.status),
                "server": &call.server,
                "tool": &call.tool,
                "arguments": &call.arguments,
                "result": &call.result,
                "error": call.error.as_ref().map(|error| error.message.clone()),
            }),
        }),
        ThreadItem::DynamicToolCall(call) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(call.id.clone()),
            title: format!("dynamic tool {}: {}", call.status, call.tool),
            body: call.tool.clone(),
            metadata: serde_json::json!({
                "tool_type": "dynamic",
                "status": &call.status,
                "tool": &call.tool,
                "arguments": &call.arguments,
                "content_items": &call.content_items,
                "success": call.success,
                "duration_ms": call.duration_ms,
            }),
        }),
        ThreadItem::CollabToolCall(call) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(call.id.clone()),
            title: format!("collaboration tool {}: {}", call.status, call.tool),
            body: call.tool.clone(),
            metadata: serde_json::json!({
                "tool_type": "collaboration",
                "status": &call.status,
                "tool": &call.tool,
                "sender_thread_id": &call.sender_thread_id,
                "receiver_thread_id": &call.receiver_thread_id,
                "new_thread_id": &call.new_thread_id,
                "prompt": &call.prompt,
                "agent_status": &call.agent_status,
            }),
        }),
        ThreadItem::WebSearch(search) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(search.id.clone()),
            title: "web search".to_owned(),
            body: search.query.clone(),
            metadata: serde_json::json!({
                "tool_type": "web_search",
                "query": &search.query,
            }),
        }),
        ThreadItem::ImageView(image) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::ToolCall,
            item_id: Some(image.id.clone()),
            title: "image view".to_owned(),
            body: image.path.clone(),
            metadata: serde_json::json!({
                "tool_type": "image_view",
                "path": &image.path,
            }),
        }),
        ThreadItem::EnteredReviewMode(review) => {
            Some(review_mode_piece(review, "entered review mode"))
        }
        ThreadItem::ExitedReviewMode(review) => {
            Some(review_mode_piece(review, "exited review mode"))
        }
        ThreadItem::ContextCompaction(item) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: Some(item.id.clone()),
            title: "context compaction".to_owned(),
            body: String::new(),
            metadata: serde_json::json!({}),
        }),
        ThreadItem::TodoList(list) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: Some(list.id.clone()),
            title: "todo list".to_owned(),
            body: format!("{} item(s)", list.items.len()),
            metadata: serde_json::json!({
                "items": list.items.iter().map(|item| {
                    serde_json::json!({
                        "text": &item.text,
                        "completed": item.completed,
                    })
                }).collect::<Vec<_>>(),
            }),
        }),
        ThreadItem::Error(error) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::Error,
            item_id: Some(error.id.clone()),
            title: "error".to_owned(),
            body: error.message.clone(),
            metadata: serde_json::json!({ "message": &error.message }),
        }),
        ThreadItem::Unknown(unknown) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: unknown.id.clone(),
            title: format!(
                "unknown item {}",
                unknown.item_type.as_deref().unwrap_or("unknown")
            ),
            body: String::new(),
            metadata: serde_json::json!({
                "item_type": &unknown.item_type,
                "raw": &unknown.raw,
            }),
        }),
        ThreadItem::UserMessage(message) => Some(OutputPieceDraft {
            kind: AgentRunOutputKind::System,
            item_id: Some(message.id.clone()),
            title: "user message".to_owned(),
            body: String::new(),
            metadata: serde_json::json!({ "content_item_count": message.content.len() }),
        }),
    }
}

fn review_mode_piece(review: &ReviewModeItem, title: &'static str) -> OutputPieceDraft {
    OutputPieceDraft {
        kind: AgentRunOutputKind::System,
        item_id: Some(review.id.clone()),
        title: title.to_owned(),
        body: review.review.clone(),
        metadata: serde_json::json!({ "review": &review.review }),
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;

    #[test]
    fn token_usage_reads_latest_run_output_metadata() {
        let pieces = vec![
            new_output_piece(
                1,
                AgentRunOutputKind::System,
                None,
                "turn completed",
                "",
                serde_json::json!({
                    "usage": {
                        "input_tokens": 10,
                        "cached_input_tokens": 2,
                        "output_tokens": 4
                    }
                }),
            ),
            new_output_piece(
                2,
                AgentRunOutputKind::System,
                None,
                "turn completed",
                "",
                serde_json::json!({
                    "usage": {
                        "inputTokens": 20,
                        "cachedInputTokens": 5,
                        "outputTokens": 7
                    }
                }),
            ),
        ];

        let usage = token_usage_from_output_pieces(&pieces).unwrap();

        assert_eq!(
            usage,
            AgentRunTokenUsageView {
                input_tokens: 20,
                cached_input_tokens: 5,
                output_tokens: 7,
                total_tokens: 27,
            }
        );
    }

    #[tokio::test]
    async fn read_run_output_wraps_legacy_plain_text_logs() {
        let temp = TempDir::new().unwrap();
        let log_path = temp.path().join("legacy.log");
        fs::write(&log_path, "old text log").unwrap();

        let output = read_run_output(Some(log_path.to_str().unwrap()))
            .await
            .unwrap();

        assert_eq!(output.len(), 1);
        assert_eq!(output[0].kind, AgentRunOutputKind::Legacy);
        assert_eq!(output[0].title, "legacy log");
        assert_eq!(output[0].body, "old text log");
        assert_eq!(output[0].metadata["format"], "plain_text");
    }
}
