use std::io::{self, Write};

use patchbay_types::{
    AgentCommitOutcome, AgentRunOutputPiece, AgentRunTokenUsageView, AgentRunView, CommentView,
    ProjectLabelView, ProjectMemoryEventView, RunLogView, WorkItemLabelView,
    WorkItemRelationshipDirection, WorkItemRelationshipItemSummary, WorkItemRelationshipListEntry,
    WorkItemRelationshipView, WorkItemView,
};

pub(crate) fn write_item_rows(output: &mut dyn Write, items: &[WorkItemView]) -> io::Result<()> {
    for item in items {
        write_item_row(output, item)?;
    }
    Ok(())
}

pub(crate) fn write_item_row(output: &mut dyn Write, item: &WorkItemView) -> io::Result<()> {
    writeln!(
        output,
        "#{}\t{}\tv{}\t{}",
        item.id,
        item_state_label(item),
        item.version,
        item.title
    )
}

pub(crate) fn write_item_detail(output: &mut dyn Write, item: &WorkItemView) -> io::Result<()> {
    writeln!(
        output,
        "#{} [{}] v{}",
        item.id,
        item_state_label(item),
        item.version
    )?;
    writeln!(output, "{}", item.title)?;
    if let Some(agent) = &item.claimed_by {
        writeln!(output, "claimed by: {agent}")?;
    }
    if !item.labels.is_empty() {
        writeln!(
            output,
            "labels: {}",
            item.labels
                .iter()
                .map(|label| format_label(&label.key, label.value.as_deref()))
                .collect::<Vec<_>>()
                .join(", ")
        )?;
    }
    writeln!(output)?;
    writeln!(output, "{}", item.description)
}

pub(crate) fn item_state_label(item: &WorkItemView) -> &str {
    item.state.as_deref().unwrap_or("(no state)")
}

pub(crate) fn write_item_labels(
    output: &mut dyn Write,
    labels: &[WorkItemLabelView],
) -> io::Result<()> {
    for label in labels {
        writeln!(
            output,
            "#{}\t{}",
            label.id,
            format_label(&label.key, label.value.as_deref())
        )?;
    }
    Ok(())
}

pub(crate) fn write_project_label_suggestions(
    output: &mut dyn Write,
    labels: &[ProjectLabelView],
) -> io::Result<()> {
    for label in labels {
        writeln!(
            output,
            "{}\t{}",
            format_label(&label.key, label.value.as_deref()),
            label.usage_count
        )?;
    }
    Ok(())
}

pub(crate) fn write_relationship_rows(
    output: &mut dyn Write,
    relationships: &[WorkItemRelationshipListEntry],
) -> io::Result<()> {
    for entry in relationships {
        let relationship = &entry.relationship;
        let related = match entry.direction {
            WorkItemRelationshipDirection::Outgoing => &relationship.target,
            WorkItemRelationshipDirection::Incoming => &relationship.source,
        };
        writeln!(
            output,
            "#{}\t{}\t#{} [{}] -- {} --> #{} [{}]\trelated: #{} {}",
            relationship.id,
            entry.direction,
            relationship.source.id,
            relationship_state_label(&relationship.source),
            relationship.kind,
            relationship.target.id,
            relationship_state_label(&relationship.target),
            related.id,
            related.title
        )?;
    }
    Ok(())
}

pub(crate) fn write_relationship_view(
    output: &mut dyn Write,
    relationship: &WorkItemRelationshipView,
    verb: &str,
) -> io::Result<()> {
    writeln!(
        output,
        "{verb} relationship #{}: #{} {} #{}",
        relationship.id,
        relationship.source_work_item_id,
        relationship.kind,
        relationship.target_work_item_id
    )
}

pub(crate) fn write_comments(output: &mut dyn Write, comments: &[CommentView]) -> io::Result<()> {
    for comment in comments {
        writeln!(
            output,
            "#{}\t{}\t{}\t{}",
            comment.id,
            comment.author_type,
            comment.author_name.as_deref().unwrap_or(""),
            comment.body
        )?;
    }
    Ok(())
}

pub(crate) fn write_memory_events(
    output: &mut dyn Write,
    events: &[ProjectMemoryEventView],
) -> io::Result<()> {
    for event in events {
        writeln!(
            output,
            "#{}\t{}\t{}\t{}",
            event.id,
            event.operation,
            event.created_at,
            event
                .actor_id
                .as_deref()
                .or(event.actor_type.as_deref())
                .unwrap_or("")
        )?;
    }
    Ok(())
}

pub(crate) fn write_automation_runs(
    output: &mut dyn Write,
    runs: &[AgentRunView],
) -> io::Result<()> {
    for run in runs {
        writeln!(
            output,
            "#{}\t{}\t{}\t{}\t{}\t{}",
            run.id,
            run.status,
            run.tool_name,
            run.mutability,
            run_token_usage_text(run),
            run.result_summary
        )?;
    }
    Ok(())
}

pub(crate) fn write_run_log(output: &mut dyn Write, log: &RunLogView) -> io::Result<()> {
    writeln!(output, "run #{} {}", log.run.id, log.run.status)?;
    writeln!(output, "mutability: {}", log.run.mutability)?;
    writeln!(output, "summary: {}", log.run.result_summary)?;
    writeln!(output, "tokens: {}", run_token_usage_text(&log.run))?;
    writeln!(output, "commit: {}", run_commit_outcome_text(&log.run))?;
    writeln!(output)?;
    writeln!(output, "output:")?;
    write_output_pieces(output, &log.output)?;
    if let Some(prompt) = &log.prompt {
        writeln!(output)?;
        writeln!(output, "prompt:")?;
        writeln!(output, "{prompt}")?;
    }
    Ok(())
}

fn format_label(key: &str, value: Option<&str>) -> String {
    match value {
        Some(value) => format!("{key}={value}"),
        None => key.to_owned(),
    }
}

fn relationship_state_label(item: &WorkItemRelationshipItemSummary) -> &str {
    item.state.as_deref().unwrap_or("(no state)")
}

fn run_commit_outcome_text(run: &AgentRunView) -> String {
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
                format!("committed {}", run.commit_shas.join(", "))
            }
        }
        AgentCommitOutcome::SkippedNoChanges => "skipped: no changes".to_owned(),
        AgentCommitOutcome::SkippedNoGitRepo => "skipped: no git repository".to_owned(),
        AgentCommitOutcome::MissingRequired => "missing required commit".to_owned(),
        AgentCommitOutcome::Unknown => "unknown".to_owned(),
    };
    format!("{base} ({requirement})")
}

fn run_token_usage_text(run: &AgentRunView) -> String {
    run.token_usage
        .map(run_token_usage_label)
        .unwrap_or_else(|| "not reported".to_owned())
}

fn run_token_usage_label(usage: AgentRunTokenUsageView) -> String {
    format!(
        "{} total ({} input, {} cached input, {} output)",
        format_number(usage.total_tokens),
        format_number(usage.input_tokens),
        format_number(usage.cached_input_tokens),
        format_number(usage.output_tokens)
    )
}

fn format_number(value: i64) -> String {
    let absolute = if value < 0 {
        -(value as i128)
    } else {
        value as i128
    };
    let mut chars = absolute.to_string().chars().rev().collect::<Vec<_>>();
    let mut formatted = String::new();
    for (index, ch) in chars.drain(..).enumerate() {
        if index > 0 && index % 3 == 0 {
            formatted.push(',');
        }
        formatted.push(ch);
    }
    let mut formatted = formatted.chars().rev().collect::<String>();
    if value < 0 {
        formatted.insert(0, '-');
    }
    formatted
}

fn write_output_pieces(output: &mut dyn Write, pieces: &[AgentRunOutputPiece]) -> io::Result<()> {
    if pieces.is_empty() {
        writeln!(output, "(empty)")?;
        return Ok(());
    }
    for piece in pieces {
        writeln!(
            output,
            "[#{} {}] {}",
            piece.sequence, piece.kind, piece.title
        )?;
        if !piece.body.trim().is_empty() {
            writeln!(output, "{}", piece.body)?;
        }
        if let Some(tool_output) = output_metadata_text(piece) {
            writeln!(output, "output:")?;
            writeln!(output, "{tool_output}")?;
        }
    }
    Ok(())
}

fn output_metadata_text(piece: &AgentRunOutputPiece) -> Option<String> {
    ["output", "result", "content_items", "error"]
        .into_iter()
        .find_map(|key| metadata_value_text(&piece.metadata, key))
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

#[cfg(test)]
mod tests {
    use patchbay_types::{
        AgentRunOutputKind, AgentRunStatus, AgentToolName, AuthorType, AutomationRunMutability,
        WorkItemRelationshipDirection, WorkItemRelationshipItemSummary,
        WorkItemRelationshipListEntry, WorkItemRelationshipView,
    };
    use serde_json::json;

    use super::*;

    fn text_output(write: impl FnOnce(&mut Vec<u8>) -> io::Result<()>) -> String {
        let mut output = Vec::new();
        write(&mut output).unwrap();
        String::from_utf8(output).unwrap()
    }

    fn label(key: &str, value: Option<&str>) -> WorkItemLabelView {
        WorkItemLabelView {
            id: 7,
            project_id: 1,
            work_item_id: 42,
            key: key.to_owned(),
            value: value.map(str::to_owned),
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
        }
    }

    fn work_item() -> WorkItemView {
        WorkItemView {
            id: 42,
            project_id: 1,
            title: "Review renderer".to_owned(),
            description: "Keep text output stable.".to_owned(),
            state: Some("open".to_owned()),
            labels: vec![label("priority", Some("high")), label("source", None)],
            version: 3,
            claimed_by: Some("patchbay-run-1".to_owned()),
            claimed_at: None,
            claim_expires_at: None,
            claim_source: None,
            finished_at: None,
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
            comment_count: 0,
        }
    }

    fn relationship_item(id: i64, title: &str, state: &str) -> WorkItemRelationshipItemSummary {
        WorkItemRelationshipItemSummary {
            id,
            title: title.to_owned(),
            state: Some(state.to_owned()),
            version: 1,
        }
    }

    fn relationship() -> WorkItemRelationshipView {
        WorkItemRelationshipView {
            id: 9,
            project_id: 1,
            kind: "is follow-up of".to_owned(),
            source_work_item_id: 42,
            target_work_item_id: 18,
            source: relationship_item(42, "Follow-up", "open"),
            target: relationship_item(18, "Original", "in_progress"),
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
        }
    }

    fn agent_run() -> AgentRunView {
        AgentRunView {
            id: 12,
            project_id: 1,
            work_item_id: Some(42),
            memory_event_id: None,
            trigger_id: None,
            trigger_name: None,
            tool_name: AgentToolName::Codex,
            mutability: AutomationRunMutability::Mutating,
            status: AgentRunStatus::Completed,
            command: "codex".to_owned(),
            working_dir: "/tmp/project".to_owned(),
            worktree_path: None,
            branch_name: None,
            process_id: None,
            exit_code: Some(0),
            log_path: None,
            prompt_path: None,
            agent_model: None,
            agent_reasoning_effort: None,
            token_usage: Some(AgentRunTokenUsageView {
                input_tokens: 1234,
                cached_input_tokens: 1000,
                output_tokens: 2500,
                total_tokens: 3734,
            }),
            commit_required: true,
            commit_outcome: AgentCommitOutcome::Committed,
            commit_shas: vec!["abc123".to_owned()],
            pr_requested: false,
            pr_url: None,
            cleanup_status: "not_needed".to_owned(),
            worktree_cleaned_at: None,
            result_summary: "Done".to_owned(),
            started_at: None,
            finished_at: None,
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
        }
    }

    #[test]
    fn item_detail_renders_state_claim_and_labels() {
        let item = work_item();
        let output = text_output(|output| write_item_detail(output, &item));

        assert_eq!(
            output,
            "#42 [open] v3\nReview renderer\nclaimed by: patchbay-run-1\nlabels: priority=high, source\n\nKeep text output stable.\n"
        );
    }

    #[test]
    fn relationship_rows_render_direction_kind_and_related_item() {
        let output = text_output(|output| {
            write_relationship_rows(
                output,
                &[
                    WorkItemRelationshipListEntry {
                        relationship: relationship(),
                        direction: WorkItemRelationshipDirection::Outgoing,
                    },
                    WorkItemRelationshipListEntry {
                        relationship: relationship(),
                        direction: WorkItemRelationshipDirection::Incoming,
                    },
                ],
            )
        });

        assert!(output.contains(
            "#9\toutgoing\t#42 [open] -- is follow-up of --> #18 [in_progress]\trelated: #18 Original\n"
        ));
        assert!(output.contains(
            "#9\tincoming\t#42 [open] -- is follow-up of --> #18 [in_progress]\trelated: #42 Follow-up\n"
        ));
    }

    #[test]
    fn relationship_mutation_summary_renders_source_kind_and_target() {
        let output =
            text_output(|output| write_relationship_view(output, &relationship(), "Updated"));

        assert_eq!(output, "Updated relationship #9: #42 is follow-up of #18\n");
    }

    #[test]
    fn run_log_renders_token_commit_and_output_metadata() {
        let log = RunLogView {
            run: agent_run(),
            active: false,
            memory_event: None,
            prompt: Some("Run this task".to_owned()),
            output: vec![AgentRunOutputPiece {
                sequence: 2,
                timestamp: "2026-06-18T00:00:00Z".to_owned(),
                kind: AgentRunOutputKind::ToolCall,
                source: "codex".to_owned(),
                item_id: Some("42".to_owned()),
                title: "Command".to_owned(),
                body: "cargo test".to_owned(),
                metadata: json!({ "output": { "status": "ok" } }),
            }],
        };

        let output = text_output(|output| write_run_log(output, &log));

        assert!(
            output.contains("tokens: 3,734 total (1,234 input, 1,000 cached input, 2,500 output)")
        );
        assert!(output.contains("commit: committed abc123 (required)"));
        assert!(output.contains("[#2 tool_call] Command\ncargo test\noutput:\n"));
        assert!(output.contains("\"status\": \"ok\""));
        assert!(output.contains("\nprompt:\nRun this task\n"));
    }

    #[test]
    fn comment_rows_keep_author_columns_even_when_name_is_missing() {
        let comments = vec![CommentView {
            id: 5,
            work_item_id: 42,
            author_type: AuthorType::Agent,
            author_name: None,
            body: "Progress".to_owned(),
            created_at: "2026-06-18T00:00:00Z".to_owned(),
        }];

        let output = text_output(|output| write_comments(output, &comments));

        assert_eq!(output, "#5\tagent\t\tProgress\n");
    }
}
