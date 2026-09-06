use std::io::{self, Write};

use dispatch_types::{
    AgentCommitOutcome, AgentRunOutputPiece, AgentRunTokenUsageView, AgentRunView, CommentView,
    ProjectLabelView, ProjectView, RunLogView, WorkItemLabelView, WorkItemRelationshipDirection,
    WorkItemRelationshipItemSummary, WorkItemRelationshipListEntry, WorkItemRelationshipView,
    WorkItemView,
};
use tabled::settings::Style;
use tabled::{Table, Tabled};

pub(crate) fn write_project_rows(
    output: &mut dyn Write,
    projects: &[ProjectView],
) -> io::Result<()> {
    let rows = projects.iter().map(PrintableProject::from);
    let mut table = Table::new(rows);
    table.with(Style::modern_rounded());
    writeln!(output, "{table}")
}

#[derive(Tabled)]
struct PrintableProject<'a> {
    #[tabled(rename = "Name")]
    name: &'a str,
    #[tabled(rename = "Display Name")]
    display_name: &'a str,
    #[tabled(rename = "Workspace Path")]
    workspace_path: &'a str,
}

impl<'a> From<&'a ProjectView> for PrintableProject<'a> {
    fn from(project: &'a ProjectView) -> Self {
        Self {
            name: &project.name,
            display_name: &project.display_name,
            workspace_path: project.path.as_deref().unwrap_or(""),
        }
    }
}

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
    if let Some(group) = &item.work_group {
        writeln!(output, "group: {} ({})", group.name, group.key)?;
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

pub(crate) fn write_automation_runs(
    output: &mut dyn Write,
    runs: &[AgentRunView],
) -> io::Result<()> {
    for run in runs {
        writeln!(
            output,
            "#{}\t{}\t{}\t{}\t{}\t{}\t{}",
            run.id,
            run.status,
            run.tool_name,
            run.mutability,
            run_launch_authority_text(run),
            run_token_usage_text(run),
            run.result_summary
        )?;
    }
    Ok(())
}

pub(crate) fn write_run_log(output: &mut dyn Write, log: &RunLogView) -> io::Result<()> {
    writeln!(output, "run #{} {}", log.run.id, log.run.status)?;
    writeln!(output, "mutability: {}", log.run.mutability)?;
    writeln!(output, "launch: {}", run_launch_authority_text(&log.run))?;
    writeln!(output, "summary: {}", log.run.result_summary)?;
    writeln!(output, "tokens: {}", run_token_usage_text(&log.run))?;
    writeln!(output, "commit: {}", run_commit_outcome_text(&log.run))?;
    if let Some(developer_instructions) = &log.developer_instructions {
        writeln!(output)?;
        writeln!(output, "developer instructions:")?;
        writeln!(output, "{developer_instructions}")?;
    }
    if let Some(user_prompt) = &log.user_prompt {
        writeln!(output)?;
        writeln!(output, "user prompt:")?;
        writeln!(output, "{user_prompt}")?;
    }
    writeln!(output)?;
    writeln!(output, "output:")?;
    write_output_pieces(output, &log.output)?;
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

fn run_launch_authority_text(run: &AgentRunView) -> String {
    use dispatch_types::{
        AgentRunLaunchResolutionView as Resolution, AgentRunLaunchTargetView as Target,
    };

    let target = match &run.launch_target {
        None => return "legacy".to_owned(),
        Some(Target::None { .. }) => "none".to_owned(),
        Some(Target::NextOpen { state, .. }) => format!("next_open:{state}"),
        Some(Target::Selector {
            selector_sha256, ..
        }) => {
            format!("selector:{}", &selector_sha256[..8])
        }
        Some(Target::Specific {
            work_item_id,
            expected_version,
            ..
        }) => {
            format!("item:{work_item_id}@v{expected_version}")
        }
    };
    let resolution = match &run.launch_resolution {
        None => "missing".to_owned(),
        Some(Resolution::Pending { .. }) => "pending".to_owned(),
        Some(Resolution::None { .. }) => "none".to_owned(),
        Some(Resolution::Claimed {
            work_item_id,
            claimed_version,
            ..
        }) => {
            format!("claimed:{work_item_id}@v{claimed_version}")
        }
        Some(Resolution::Unavailable { reason, .. }) => format!("unavailable:{reason}"),
    };
    format!("{target}/{resolution}")
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
    use assertr::prelude::*;
    use dispatch_types::{
        AgentRunCleanupStatus, AgentRunKind, AgentRunOutputKind, AgentRunStatus, AgentToolName,
        AuthorType, AutomationRunMutability, WorkItemRelationshipDirection,
        WorkItemRelationshipItemSummary, WorkItemRelationshipListEntry, WorkItemRelationshipView,
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
            claimed_by: Some("dispatch-run-1".to_owned()),
            claimed_at: None,
            claim_expires_at: None,
            claim_source: None,
            finished_at: None,
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            created_at: "2026-06-18T00:00:00Z".to_owned(),
            updated_at: "2026-06-18T00:00:00Z".to_owned(),
            comment_count: 0,
            work_group: None,
            origin: None,
        }
    }

    fn project() -> ProjectView {
        serde_json::from_value(json!({
            "id": 4,
            "name": "patchbay",
            "display_name": "Dispatch",
            "path": "/tmp/dispatch",
            "knowledge_directory": "knowledge",
            "path_exists": true,
            "path_checked_at": null,
            "git_status": null,
            "system_prompt": "",
            "workspace_mode": "current_branch",
            "max_code_edit_agents": 1,
            "max_read_only_agents": 2,
            "create_pr": false,
            "auto_commit": true,
            "commit_standard": "",
            "revert_strategy": "manual",
            "stale_claim_minutes": 0,
            "worktree_cleanup_policy": "manual",
            "default_agent_tool": "codex",
            "default_agent_model": null,
            "default_agent_reasoning_effort": null,
            "agent_sandbox_mode": "workspace_write",
            "agent_extra_writable_roots": [],
            "agent_git_command_policy": {
                "add": true,
                "commit": true,
                "push": true,
                "reset": true,
                "hard_reset": "isolated_workspaces"
            },
            "created_at": "2026-06-18T00:00:00Z",
            "updated_at": "2026-06-18T00:00:00Z"
        }))
        .unwrap()
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
            run_kind: AgentRunKind::Task,
            purpose: None,
            launch_target: None,
            launch_resolution: None,
            knowledge_revision: None,
            source_baseline_id: None,
            source_snapshot_id: None,
            knowledge_view_sha256: None,
            input_overlay_sha256: None,
            source_authority_kind: None,
            source_ref_name: None,
            source_raw_head: None,
            trigger_id: None,
            trigger_name: None,
            trigger_revision_id: None,
            personality_revision_id: None,
            system_prompt_event_id: None,
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
            developer_instructions_path: None,
            user_prompt_path: None,
            agent_model: None,
            agent_reasoning_effort: None,
            effective_input_sha256: None,
            effective_timeout_seconds: None,
            effective_concurrency_group: None,
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
            cleanup_status: AgentRunCleanupStatus::NotApplicable,
            worktree_cleaned_at: None,
            result_summary: "Done".to_owned(),
            semantic_postcondition_status: Default::default(),
            semantic_postcondition_failures: Vec::new(),
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

        assert_that!(&(output)).is_equal_to("#42 [open] v3\nReview renderer\nclaimed by: dispatch-run-1\nlabels: priority=high, source\n\nKeep text output stable.\n");
    }

    #[test]
    fn project_rows_render_a_bordered_table_with_headers() {
        let output = text_output(|output| write_project_rows(output, &[project()]));

        assert_that!(&(output)).is_equal_to(concat!(
            "╭──────────┬──────────────┬────────────────╮\n",
            "│ Name     │ Display Name │ Workspace Path │\n",
            "├──────────┼──────────────┼────────────────┤\n",
            "│ patchbay │ Dispatch     │ /tmp/dispatch  │\n",
            "╰──────────┴──────────────┴────────────────╯\n",
        ));
    }

    #[test]
    fn empty_project_list_still_renders_the_table_headers() {
        let output = text_output(|output| write_project_rows(output, &[]));

        assert_that!(&(output)).is_equal_to(concat!(
            "╭──────┬──────────────┬────────────────╮\n",
            "│ Name │ Display Name │ Workspace Path │\n",
            "╰──────┴──────────────┴────────────────╯\n",
        ));
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

        assert_that!(&(output.contains(
            "#9\toutgoing\t#42 [open] -- is follow-up of --> #18 [in_progress]\trelated: #18 Original\n"
        ))).is_true();
        assert_that!(&(output.contains(
            "#9\tincoming\t#42 [open] -- is follow-up of --> #18 [in_progress]\trelated: #42 Follow-up\n"
        ))).is_true();
    }

    #[test]
    fn relationship_mutation_summary_renders_source_kind_and_target() {
        let output =
            text_output(|output| write_relationship_view(output, &relationship(), "Updated"));

        assert_that!(&(output)).is_equal_to("Updated relationship #9: #42 is follow-up of #18\n");
    }

    #[test]
    fn run_log_renders_token_commit_and_output_metadata() {
        let log = RunLogView {
            run: agent_run(),
            active: false,
            developer_instructions: Some("Follow Dispatch policy".to_owned()),
            user_prompt: Some("Run this task".to_owned()),
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
            created_items: Vec::new(),
            modified_items: Vec::new(),
        };

        let output = text_output(|output| write_run_log(output, &log));

        assert_that!(
            &(output
                .contains("tokens: 3,734 total (1,234 input, 1,000 cached input, 2,500 output)"))
        )
        .is_true();
        assert_that!(&(output.contains("commit: committed abc123 (required)"))).is_true();
        assert_that!(&(output.contains("[#2 tool_call] Command\ncargo test\noutput:\n"))).is_true();
        assert_that!(&(output.contains("\"status\": \"ok\""))).is_true();
        assert_that!(&(output.contains("\ndeveloper instructions:\nFollow Dispatch policy\n")))
            .is_true();
        assert_that!(&(output.contains("\nuser prompt:\nRun this task\n"))).is_true();
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

        assert_that!(&(output)).is_equal_to("#5\tagent\t\tProgress\n");
    }
}
