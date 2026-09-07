use super::*;
use assertr::prelude::*;
use serde_json::json;

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

    assert_that!(&(entries.len())).is_equal_to(1);
    assert_that!(&(entries[0].start.as_ref().map(|piece| piece.title.as_str())))
        .is_equal_to(Some("command started"));
    assert_that!(&(entries[0].piece.title)).is_equal_to("command Completed");
    assert_that!(&(output_entry_duration_seconds(&entries[0]))).is_equal_to(Some(83));
    assert_that!(&(output_entry_title(&entries[0]))).is_equal_to("Ran command in 1m 23s");
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

    assert_that!(&(entries.len())).is_equal_to(1);
    assert_that!(&(output_entry_title(&entries[0]))).is_equal_to("Thought for 8s");
    assert_that!(&(entries[0].piece.body.is_empty())).is_true();
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

    assert_that!(&(current_reasoning_sequence(&output, true))).is_equal_to(Some(1));
    assert_that!(&(current_reasoning_sequence(&output, false))).is_equal_to(None);

    output.push(output_piece(
        2,
        "2026-06-19T10:00:01Z",
        AgentRunOutputKind::ToolCall,
        Some("call_1"),
        "command started",
        "just check",
        json!({ "tool_type": "command", "status": "started" }),
    ));
    assert_that!(&(current_reasoning_sequence(&output, true))).is_equal_to(None);
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

    assert_that!(&(entries.len())).is_equal_to(1);
    assert_that!(&(entries[0].piece.body)).is_equal_to("Useful output");
}

#[test]
fn output_duration_uses_readable_units() {
    assert_that!(&(format_output_duration(0))).is_equal_to("<1s");
    assert_that!(&(format_output_duration(12))).is_equal_to("12s");
    assert_that!(&(format_output_duration(83))).is_equal_to("1m 23s");
    assert_that!(&(format_output_duration(3723))).is_equal_to("1h 2m 3s");
}

#[test]
fn abbreviate_lines_keeps_short_output_and_marks_truncation() {
    assert_that!(&(abbreviate_lines("one\ntwo", 5))).is_equal_to(("one\ntwo".to_owned(), false));
    assert_that!(&(abbreviate_lines("1\n2\n3", 2))).is_equal_to(("1\n2".to_owned(), true));
}

#[test]
fn command_presentation_unwraps_shell_and_recognizes_conservative_file_reads() {
    let sed = command_presentation(
        r#"/bin/zsh -lc "sed -n '10,30p' 'dispatch-server/src/file with spaces.rs'""#,
    );
    assert_that!(&(sed.display))
        .is_equal_to("sed -n '10,30p' 'dispatch-server/src/file with spaces.rs'");
    assert_that!(&(sed.exploring_file.as_deref()))
        .is_equal_to(Some("dispatch-server/src/file with spaces.rs"));

    assert_that!(
        &(command_presentation("cat -- knowledge/ui.md")
            .exploring_file
            .as_deref())
    )
    .is_equal_to(Some("knowledge/ui.md"));
    assert_that!(
        &(command_presentation("head -n 20 dispatch-server/src/lib.rs")
            .exploring_file
            .as_deref())
    )
    .is_equal_to(Some("dispatch-server/src/lib.rs"));
    assert_that!(
        &(command_presentation("tail --lines=20 dispatch-server/src/lib.rs")
            .exploring_file
            .as_deref())
    )
    .is_equal_to(Some("dispatch-server/src/lib.rs"));
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
        assert_that!(&(command_presentation(command).exploring_file))
            .with_detail_message(format!("unexpected exploration label for {command}"))
            .is_equal_to(None);
    }
}

#[test]
fn diff_helpers_detect_and_classify_changed_lines() {
    let diff = "diff --git a/file b/file\n@@ -1 +1 @@\n-old\n+new\n context";

    assert_that!(&(looks_like_diff(diff))).is_true();
    assert_that!(&(diff_line_class("+new"))).is_equal_to("diff-line diff-added");
    assert_that!(&(diff_line_class("-old"))).is_equal_to("diff-line diff-removed");
    assert_that!(&(diff_line_class("@@ -1 +1 @@"))).is_equal_to("diff-line diff-hunk");
    assert_that!(&(diff_line_class(" context"))).is_equal_to("diff-line diff-context");
    assert_that!(&(diff_line_class("+++ b/file"))).is_equal_to("diff-line diff-context");
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
