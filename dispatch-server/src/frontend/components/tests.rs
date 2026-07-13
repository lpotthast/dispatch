use crate::frontend::rich_text::rich_text_editor_html;
use crate::shared::view_models::{AgentRunOutputKind, AgentRunOutputPiece};

use super::{
    abbreviate_lines, claim_elapsed_seconds_at, command_presentation, compact_run_output,
    current_reasoning_sequence, diff_line_class, format_claim_elapsed_seconds,
    format_output_duration, looks_like_diff, output_entry_duration_seconds, output_entry_title,
    preview,
};
use serde_json::json;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

#[test]
fn formats_claim_elapsed_time() {
    assert_eq!(format_claim_elapsed_seconds(70), "1:10");
    assert_eq!(format_claim_elapsed_seconds(3670), "1:01:10");
    assert_eq!(format_claim_elapsed_seconds(-5), "0:00");
}

#[test]
fn derives_claim_elapsed_time_from_claim_timestamp() {
    let now = OffsetDateTime::parse("2026-06-17T18:01:10Z", &Rfc3339).unwrap();
    assert_eq!(
        claim_elapsed_seconds_at("2026-06-17T18:00:00Z", now),
        Some(70)
    );
    assert_eq!(
        claim_elapsed_seconds_at("2026-06-17T18:02:00Z", now),
        Some(0)
    );
    assert_eq!(claim_elapsed_seconds_at("not a timestamp", now), None);
}

#[test]
fn rich_text_editor_html_preserves_plain_text_line_breaks() {
    assert_eq!(
        rich_text_editor_html("First line\nSecond line\n\nThird"),
        "<p>First line<br>Second line</p><p>Third</p>"
    );
}

#[test]
fn preview_omits_rich_text_markup() {
    assert_eq!(
        preview("<p>First <strong>item</strong></p><p>Second</p>"),
        "First item\nSecond"
    );
}

#[test]
fn compact_run_output_replaces_started_tool_entry() {
    let output = vec![
        output_piece(
            1,
            "2026-06-19T10:00:00Z",
            AgentRunOutputKind::ToolCall,
            Some("call_1"),
            "command started",
            "just fmt",
            json!({
                "tool_type": "command",
                "status": "started",
                "command": "just fmt"
            }),
        ),
        output_piece(
            2,
            "2026-06-19T10:01:23Z",
            AgentRunOutputKind::ToolCall,
            Some("call_1"),
            "command Completed",
            "just fmt",
            json!({
                "tool_type": "command",
                "status": "Completed",
                "command": "just fmt",
                "output": "done"
            }),
        ),
    ];

    let entries = compact_run_output(output);

    assert_eq!(entries.len(), 1);
    assert_eq!(
        entries[0].start.as_ref().map(|piece| piece.title.as_str()),
        Some("command started")
    );
    assert_eq!(entries[0].piece.title, "command Completed");
    assert_eq!(output_entry_duration_seconds(&entries[0]), Some(83));
    assert_eq!(output_entry_title(&entries[0]), "Ran command in 1m 23s");
}

#[test]
fn compact_run_output_turns_completed_reasoning_into_a_timing_entry() {
    let output = vec![
        output_piece(
            1,
            "2026-06-19T10:00:00Z",
            AgentRunOutputKind::Reasoning,
            Some("reasoning_1"),
            "thinking",
            "",
            json!({ "status": "started" }),
        ),
        output_piece(
            2,
            "2026-06-19T10:00:08Z",
            AgentRunOutputKind::Reasoning,
            Some("reasoning_1"),
            "thinking",
            "",
            json!({ "status": "completed" }),
        ),
    ];

    let entries = compact_run_output(output);

    assert_eq!(entries.len(), 1);
    assert_eq!(output_entry_title(&entries[0]), "Thought for 8s");
    assert!(entries[0].piece.body.is_empty());
}

#[test]
fn current_reasoning_must_be_active_and_the_latest_event() {
    let mut output = vec![output_piece(
        1,
        "2026-06-19T10:00:00Z",
        AgentRunOutputKind::Reasoning,
        Some("reasoning_1"),
        "thinking",
        "",
        json!({ "status": "started" }),
    )];

    assert_eq!(current_reasoning_sequence(&output, true), Some(1));
    assert_eq!(current_reasoning_sequence(&output, false), None);

    output.push(output_piece(
        2,
        "2026-06-19T10:00:01Z",
        AgentRunOutputKind::ToolCall,
        Some("call_1"),
        "command started",
        "just check",
        json!({ "tool_type": "command", "status": "started" }),
    ));
    assert_eq!(current_reasoning_sequence(&output, true), None);
}

#[test]
fn compact_run_output_hides_low_value_system_noise() {
    let output = vec![
        output_piece(
            1,
            "2026-06-19T10:00:00Z",
            AgentRunOutputKind::System,
            None,
            "thread started",
            "thread_hash",
            json!({ "thread_id": "thread_hash" }),
        ),
        output_piece(
            2,
            "2026-06-19T10:00:01Z",
            AgentRunOutputKind::ModelMessage,
            Some("msg_1"),
            "model output",
            "Useful output",
            json!({}),
        ),
    ];

    let entries = compact_run_output(output);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].piece.body, "Useful output");
}

#[test]
fn output_duration_uses_readable_units() {
    assert_eq!(format_output_duration(0), "<1s");
    assert_eq!(format_output_duration(12), "12s");
    assert_eq!(format_output_duration(83), "1m 23s");
    assert_eq!(format_output_duration(3723), "1h 2m 3s");
}

#[test]
fn abbreviate_lines_keeps_short_output_and_marks_truncation() {
    assert_eq!(
        abbreviate_lines("one\ntwo", 5),
        ("one\ntwo".to_owned(), false)
    );
    assert_eq!(abbreviate_lines("1\n2\n3", 2), ("1\n2".to_owned(), true));
}

#[test]
fn command_presentation_unwraps_shell_and_recognizes_conservative_file_reads() {
    let sed = command_presentation(
        r#"/bin/zsh -lc "sed -n '10,30p' 'dispatch-server/src/file with spaces.rs'""#,
    );
    assert_eq!(
        sed.display,
        "sed -n '10,30p' 'dispatch-server/src/file with spaces.rs'"
    );
    assert_eq!(
        sed.exploring_file.as_deref(),
        Some("dispatch-server/src/file with spaces.rs")
    );

    assert_eq!(
        command_presentation("cat -- design/ui.md")
            .exploring_file
            .as_deref(),
        Some("design/ui.md")
    );
    assert_eq!(
        command_presentation("head -n 20 dispatch-server/src/lib.rs")
            .exploring_file
            .as_deref(),
        Some("dispatch-server/src/lib.rs")
    );
    assert_eq!(
        command_presentation("tail --lines=20 dispatch-server/src/lib.rs")
            .exploring_file
            .as_deref(),
        Some("dispatch-server/src/lib.rs")
    );
}

#[test]
fn command_presentation_leaves_ambiguous_inspection_as_commands() {
    for command in [
        "cat first.rs second.rs",
        "cat $FILE",
        "cat file.rs | head",
        "cat file.rs|head",
        "cat --help",
        "rg pattern dispatch-server/src/lib.rs",
        "git diff -- dispatch-server/src/lib.rs",
    ] {
        assert_eq!(
            command_presentation(command).exploring_file,
            None,
            "unexpected exploration label for {command}"
        );
    }
}

#[test]
fn diff_helpers_detect_and_classify_changed_lines() {
    let diff = "diff --git a/file b/file\n@@ -1 +1 @@\n-old\n+new\n context";

    assert!(looks_like_diff(diff));
    assert_eq!(diff_line_class("+new"), "diff-line diff-added");
    assert_eq!(diff_line_class("-old"), "diff-line diff-removed");
    assert_eq!(diff_line_class("@@ -1 +1 @@"), "diff-line diff-hunk");
    assert_eq!(diff_line_class(" context"), "diff-line diff-context");
    assert_eq!(diff_line_class("+++ b/file"), "diff-line diff-context");
}

fn output_piece(
    sequence: u64,
    timestamp: &str,
    kind: AgentRunOutputKind,
    item_id: Option<&str>,
    title: &str,
    body: &str,
    metadata: serde_json::Value,
) -> AgentRunOutputPiece {
    AgentRunOutputPiece {
        sequence,
        timestamp: timestamp.to_owned(),
        kind,
        source: "codex".to_owned(),
        item_id: item_id.map(str::to_owned),
        title: title.to_owned(),
        body: body.to_owned(),
        metadata,
    }
}
