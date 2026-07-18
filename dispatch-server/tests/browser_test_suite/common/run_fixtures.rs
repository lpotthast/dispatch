use std::fs;

use assertr::prelude::*;
use leptos_browser_test::{Report, ResultExt};
use rootcause::option_ext::OptionExt;
use sea_orm::{ConnectionTrait, Database, DbBackend, Statement};

use super::DispatchTestApp;

pub(crate) async fn seed_run_commit_outcome_fixtures(app: &DispatchTestApp) -> Result<(), Report> {
    let db = Database::connect(format!("sqlite://{}?mode=rwc", app.database.display()))
        .await
        .context("failed to connect to Dispatch browser-test database")?;
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA foreign_keys = ON".to_owned(),
    ))
    .await
    .context("failed to enable browser-test SQLite foreign keys")?;

    // These run fields are server-owned and intentionally excluded from public CrudKit create and
    // update models, so the browser test seeds deterministic rows directly in its private database.
    let committed = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            INSERT INTO "agent_runs" (
                "id",
                "project_id",
                "work_item_id",
                "memory_event_id",
                "trigger_id",
                "trigger_name",
                "tool_name",
                "mutability",
                "status",
                "command",
                "working_dir",
                "worktree_path",
                "branch_name",
                "process_id",
                "exit_code",
                "log_path",
                "developer_instructions_path",
                "user_prompt_path",
                "agent_model",
                "agent_reasoning_effort",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "commit_required",
                "commit_outcome",
                "commit_shas",
                "pr_requested",
                "pr_url",
                "cleanup_status",
                "worktree_cleaned_at",
                "result_summary",
                "started_at",
                "finished_at",
                "created_at",
                "updated_at"
            )
            SELECT
                501,
                "id",
                NULL,
                NULL,
                NULL,
                NULL,
                'codex',
                'mutating',
                'completed',
                'codex --browser-test',
                '/tmp/dispatch-browser-test',
                NULL,
                NULL,
                NULL,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                1,
                'committed',
                '["0123456789abcdef0123456789abcdef01234567"]',
                0,
                NULL,
                'not_applicable',
                NULL,
                'Done. Created browser-test commit fixture.',
                '2026-06-19T10:00:00Z',
                '2026-06-19T10:01:00Z',
                '2026-06-19T10:00:00Z',
                '2026-06-19T10:01:00Z'
            FROM "projects"
            WHERE "name" = 'demo';
            "#
            .to_owned(),
        ))
        .await
        .context("failed to seed committed automation run fixture")?;
    assert_that!(committed.rows_affected()).is_equal_to(1);

    let missing_required = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            INSERT INTO "agent_runs" (
                "id",
                "project_id",
                "work_item_id",
                "memory_event_id",
                "trigger_id",
                "trigger_name",
                "tool_name",
                "mutability",
                "status",
                "command",
                "working_dir",
                "worktree_path",
                "branch_name",
                "process_id",
                "exit_code",
                "log_path",
                "developer_instructions_path",
                "user_prompt_path",
                "agent_model",
                "agent_reasoning_effort",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "commit_required",
                "commit_outcome",
                "commit_shas",
                "pr_requested",
                "pr_url",
                "cleanup_status",
                "worktree_cleaned_at",
                "result_summary",
                "started_at",
                "finished_at",
                "created_at",
                "updated_at"
            )
            SELECT
                502,
                "id",
                NULL,
                NULL,
                NULL,
                NULL,
                'codex',
                'mutating',
                'failed',
                'codex --browser-test',
                '/tmp/dispatch-browser-test',
                NULL,
                NULL,
                NULL,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                1,
                'missing_required',
                '[]',
                0,
                NULL,
                'not_applicable',
                NULL,
                'Missing required commit: completed run left uncommitted changes.',
                '2026-06-19T10:02:00Z',
                '2026-06-19T10:03:00Z',
                '2026-06-19T10:02:00Z',
                '2026-06-19T10:03:00Z'
            FROM "projects"
            WHERE "name" = 'demo';
            "#
            .to_owned(),
        ))
        .await
        .context("failed to seed missing-required automation run fixture")?;
    assert_that!(missing_required.rows_affected()).is_equal_to(1);

    let output_path = app.temp_dir().join("run-503.output.json");
    let command = r#"/bin/zsh -lc "sed -n '1,4p' design/ui.md""#;
    let generic_command = "/bin/zsh -lc 'just check'";
    let diff_command = "/bin/zsh -lc 'git diff -- design/ui.md'";
    let diff_output = "diff --git a/design/ui.md b/design/ui.md\nindex 1111111..2222222 100644\n--- a/design/ui.md\n+++ b/design/ui.md\n@@ -1 +1 @@\n-old copy\n+new copy";
    let output = serde_json::json!({
        "schema_version": 1,
        "pieces": [
            {
                "sequence": 1,
                "timestamp": "2026-06-19T10:04:00Z",
                "kind": "reasoning",
                "source": "codex",
                "item_id": "reasoning_1",
                "title": "thinking",
                "body": "",
                "metadata": { "status": "started" }
            },
            {
                "sequence": 2,
                "timestamp": "2026-06-19T10:04:08Z",
                "kind": "reasoning",
                "source": "codex",
                "item_id": "reasoning_1",
                "title": "thinking",
                "body": "",
                "metadata": { "status": "completed" }
            },
            {
                "sequence": 3,
                "timestamp": "2026-06-19T10:04:09Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_1",
                "title": "command started",
                "body": command,
                "metadata": {
                    "tool_type": "command",
                    "status": "started",
                    "command": command
                }
            },
            {
                "sequence": 4,
                "timestamp": "2026-06-19T10:04:10Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_1",
                "title": "command completed",
                "body": command,
                "metadata": {
                    "tool_type": "command",
                    "status": "completed",
                    "command": command,
                    "exit_code": 0,
                    "output": "line one\nline two\nline three\nline four"
                }
            },
            {
                "sequence": 5,
                "timestamp": "2026-06-19T10:04:11Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_2",
                "title": "command started",
                "body": generic_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "started",
                    "command": generic_command
                }
            },
            {
                "sequence": 6,
                "timestamp": "2026-06-19T10:04:12Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_2",
                "title": "command completed",
                "body": generic_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "completed",
                    "command": generic_command,
                    "exit_code": 0,
                    "output": "checked"
                }
            },
            {
                "sequence": 7,
                "timestamp": "2026-06-19T10:04:13Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_3",
                "title": "command started",
                "body": diff_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "started",
                    "command": diff_command
                }
            },
            {
                "sequence": 8,
                "timestamp": "2026-06-19T10:04:14Z",
                "kind": "tool_call",
                "source": "codex",
                "item_id": "command_3",
                "title": "command completed",
                "body": diff_command,
                "metadata": {
                    "tool_type": "command",
                    "status": "completed",
                    "command": diff_command,
                    "exit_code": 0,
                    "output": diff_output
                }
            },
            {
                "sequence": 9,
                "timestamp": "2026-06-19T10:04:15Z",
                "kind": "model_message",
                "source": "codex",
                "item_id": "message_1",
                "title": "final answer",
                "body": "Readable model output.",
                "metadata": { "final_answer": true }
            }
        ]
    });
    fs::write(
        &output_path,
        serde_json::to_vec_pretty(&output).context("failed to encode run-output fixture")?,
    )
    .context("failed to write run-output fixture")?;
    let compact_output = db
        .execute(Statement::from_sql_and_values(
            DbBackend::Sqlite,
            r#"
            INSERT INTO "agent_runs" (
                "id",
                "project_id",
                "work_item_id",
                "memory_event_id",
                "trigger_id",
                "trigger_name",
                "tool_name",
                "mutability",
                "status",
                "command",
                "working_dir",
                "worktree_path",
                "branch_name",
                "process_id",
                "exit_code",
                "log_path",
                "developer_instructions_path",
                "user_prompt_path",
                "agent_model",
                "agent_reasoning_effort",
                "input_tokens",
                "cached_input_tokens",
                "output_tokens",
                "commit_required",
                "commit_outcome",
                "commit_shas",
                "pr_requested",
                "pr_url",
                "cleanup_status",
                "worktree_cleaned_at",
                "result_summary",
                "started_at",
                "finished_at",
                "created_at",
                "updated_at"
            )
            SELECT
                503,
                "id",
                NULL,
                NULL,
                NULL,
                NULL,
                'codex',
                'mutating',
                'completed',
                'codex --browser-test',
                '/tmp/dispatch-browser-test',
                NULL,
                NULL,
                NULL,
                0,
                ?1,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                NULL,
                0,
                'not_required',
                '[]',
                0,
                NULL,
                'not_applicable',
                NULL,
                'Rendered compact browser-test output.',
                '2026-06-19T10:04:00Z',
                '2026-06-19T10:04:13Z',
                '2026-06-19T10:04:00Z',
                '2026-06-19T10:04:13Z'
            FROM "projects"
            WHERE "name" = 'demo';
            "#,
            vec![output_path.to_string_lossy().into_owned().into()],
        ))
        .await
        .context("failed to seed compact run-output fixture")?;
    assert_that!(compact_output.rows_affected()).is_equal_to(1);

    Ok(())
}

pub(crate) async fn link_run_fixtures_to_browser_item(app: &DispatchTestApp) -> Result<(), Report> {
    let db = Database::connect(format!("sqlite://{}?mode=rwc", app.database.display()))
        .await
        .context("failed to connect to Dispatch browser-test database")?;
    let result = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            UPDATE "agent_runs"
            SET "work_item_id" = (
                SELECT "work_items"."id"
                FROM "work_items"
                INNER JOIN "projects" ON "projects"."id" = "work_items"."project_id"
                WHERE "projects"."name" = 'demo'
                  AND "work_items"."title" = 'Browser item'
            )
            WHERE "id" IN (501, 502, 503);
            "#
            .to_owned(),
        ))
        .await
        .context("failed to link browser-test run fixtures to Browser item")?;
    assert_that!(result.rows_affected()).is_equal_to(3);
    let linked = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            SELECT COUNT(*) AS "linked"
            FROM "agent_runs"
            INNER JOIN "work_items" ON "work_items"."id" = "agent_runs"."work_item_id"
            INNER JOIN "projects" ON "projects"."id" = "work_items"."project_id"
            WHERE "agent_runs"."id" IN (501, 502, 503)
              AND "projects"."name" = 'demo'
              AND "work_items"."title" = 'Browser item';
            "#
            .to_owned(),
        ))
        .await
        .context("failed to verify browser-test run fixture links")?
        .context("browser-test run fixture link query returned no row")?
        .try_get::<i64>("", "linked")
        .context("failed to read browser-test run fixture link count")?;
    assert_that!(linked).is_equal_to(3);
    Ok(())
}

pub(crate) async fn activate_browser_item_run(app: &DispatchTestApp) -> Result<(), Report> {
    let db = Database::connect(format!("sqlite://{}?mode=rwc", app.database.display()))
        .await
        .context("failed to connect to Dispatch browser-test database")?;
    let result = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            UPDATE "agent_runs"
            SET "status" = 'running',
                "trigger_name" = 'Claim open work',
                "finished_at" = NULL,
                "updated_at" = CURRENT_TIMESTAMP
            WHERE "id" = 503;
            "#
            .to_owned(),
        ))
        .await
        .context("failed to activate browser-test item run fixture")?;
    assert_that!(result.rows_affected()).is_equal_to(1);
    Ok(())
}
