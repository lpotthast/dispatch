use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use rootcause::{Result, prelude::*};
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, Statement};
use sea_orm_migration::MigratorTrait;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::backend::migrations::Migrator;

#[derive(Clone)]
pub struct Store {
    db: Arc<DatabaseConnection>,
    path: Arc<PathBuf>,
    automation_production_lock: Arc<tokio::sync::Mutex<()>>,
}

impl Store {
    pub async fn open(path: PathBuf) -> Result<Self> {
        let path = absolute_path(path)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).context_with(|| {
                format!("failed to create database directory {}", parent.display())
            })?;
        }

        let url = sqlite_url(&path);
        let db = Database::connect(&url)
            .await
            .context_with(|| format!("failed to open database {}", path.display()))?;
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .context("failed to enable SQLite foreign keys")?;

        Migrator::up(&db, None)
            .await
            .context("failed to apply database migrations")?;

        Ok(Self {
            db: Arc::new(db),
            path: Arc::new(path),
            automation_production_lock: Arc::new(tokio::sync::Mutex::new(())),
        })
    }

    pub fn db(&self) -> Arc<DatabaseConnection> {
        self.db.clone()
    }

    pub fn path(&self) -> &Path {
        self.path.as_ref().as_path()
    }

    /// Serializes the producer deduplication read and item creation transaction.
    ///
    /// Dispatch has one owning server process for a database. SQLite deferred transactions do not
    /// acquire a write lock until the first write, so concurrent producer evaluations otherwise
    /// could both observe no unfinished item before either inserts its origin row.
    pub(crate) async fn lock_automation_production(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.automation_production_lock.lock().await
    }
}

pub fn default_database_path() -> PathBuf {
    dispatch_home_dir().join("dispatch.sqlite3")
}

pub fn dispatch_home_dir() -> PathBuf {
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".dispatch");
    }

    PathBuf::from(".dispatch")
}

pub fn utc_now() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn sqlite_url(path: &Path) -> String {
    format!("sqlite://{}?mode=rwc", path.display())
}

fn absolute_path(path: PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path);
    }
    Ok(env::current_dir()
        .context("failed to read current directory for database path")?
        .join(path))
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use sea_orm::{ConnectionTrait, Statement};
    use tempfile::TempDir;

    use crate::backend::migrations::REMOVED_REFINEMENT_CONCURRENCY_COLUMN;

    use super::*;

    #[tokio::test]
    async fn migrations_apply_cleanly() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dispatch.sqlite3");

        Store::open(path.clone()).await.unwrap();

        let second = Store::open(path).await.unwrap();
        let row = second
            .db()
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM seaql_migrations".to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        let count: i64 = row.try_get("", "count").unwrap();

        assert_that!(&(count as usize)).is_equal_to(Migrator::migrations().len());
    }

    #[tokio::test]
    async fn automation_run_input_migration_replaces_the_combined_path() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dispatch.sqlite3");
        let db = Database::connect(sqlite_url(&path)).await.unwrap();
        let migration_count_before_input_split = Migrator::migrations()
            .iter()
            .position(|migration| {
                migration.name() == "m20260710_000037_separate_automation_run_inputs"
            })
            .unwrap() as u32;
        Migrator::up(&db, Some(migration_count_before_input_split))
            .await
            .unwrap();
        for statement in [
            r#"DROP VIEW "agent_runs_read_view";"#,
            r#"ALTER TABLE "agent_runs" DROP COLUMN "developer_instructions_path";"#,
            r#"ALTER TABLE "agent_runs" DROP COLUMN "user_prompt_path";"#,
            r#"ALTER TABLE "agent_runs" ADD COLUMN "prompt_path" TEXT;"#,
            r#"CREATE VIEW "agent_runs_read_view" AS SELECT "agent_runs".*, 0 AS has_validation_errors FROM "agent_runs";"#,
            r#"INSERT INTO "projects" ("id", "name", "display_name") VALUES (1, 'demo', 'Demo');"#,
            r#"INSERT INTO "agent_runs" ("project_id", "tool_name", "status", "command", "working_dir", "prompt_path") VALUES (1, 'codex', 'completed', 'codex app-server', '/tmp/project', '/tmp/combined-prompt.md');"#,
        ] {
            db.execute(Statement::from_string(
                DbBackend::Sqlite,
                statement.to_owned(),
            ))
            .await
            .unwrap();
        }

        Migrator::up(&db, None).await.unwrap();
        let row = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"
                SELECT
                    SUM(name = 'developer_instructions_path') AS developer_count,
                    SUM(name = 'user_prompt_path') AS user_count,
                    SUM(name = 'prompt_path') AS combined_count
                FROM pragma_table_info('agent_runs');
                "#
                .to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();

        assert_that!(&(row.try_get::<i64>("", "developer_count").unwrap())).is_equal_to(1);
        assert_that!(&(row.try_get::<i64>("", "user_count").unwrap())).is_equal_to(1);
        assert_that!(&(row.try_get::<i64>("", "combined_count").unwrap())).is_equal_to(0);

        let run = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"
                SELECT "developer_instructions_path", "user_prompt_path"
                FROM "agent_runs"
                LIMIT 1;
                "#
                .to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_that!(
            &(run
                .try_get::<String>("", "developer_instructions_path")
                .unwrap())
        )
        .is_equal_to("/tmp/combined-prompt.md");
        assert_that!(&(run.try_get::<String>("", "user_prompt_path").unwrap()))
            .is_equal_to("/tmp/combined-prompt.md");
    }

    #[tokio::test]
    async fn automation_run_input_down_migration_restores_legacy_prompt_artifacts() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dispatch.sqlite3");
        let db = Database::connect(sqlite_url(&path)).await.unwrap();
        Migrator::up(&db, None).await.unwrap();

        let shared_prompt_path = temp.path().join("shared-prompt.md");
        let developer_instructions_path = temp.path().join("run-42.developer-instructions.md");
        let user_prompt_path = temp.path().join("run-42.user-prompt.md");
        fs::write(&shared_prompt_path, "Legacy combined prompt.").unwrap();
        fs::write(&developer_instructions_path, "Follow Dispatch policy.\n").unwrap();
        fs::write(&user_prompt_path, "Implement the requested change.\n").unwrap();

        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"INSERT INTO "projects" ("id", "name", "display_name") VALUES (1, 'demo', 'Demo');"#
                .to_owned(),
        ))
        .await
        .unwrap();
        for (run_id, developer_path, user_path) in [
            (41, &shared_prompt_path, &shared_prompt_path),
            (42, &developer_instructions_path, &user_prompt_path),
        ] {
            db.execute(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                r#"
                INSERT INTO "agent_runs" (
                    "id",
                    "project_id",
                    "tool_name",
                    "status",
                    "command",
                    "working_dir",
                    "developer_instructions_path",
                    "user_prompt_path"
                )
                VALUES (?1, 1, 'codex', 'completed', 'codex app-server', '/tmp/project', ?2, ?3);
                "#,
                vec![
                    run_id.into(),
                    developer_path.to_string_lossy().into_owned().into(),
                    user_path.to_string_lossy().into_owned().into(),
                ],
            ))
            .await
            .unwrap();
        }

        // Step through the workflow-support and work-group migrations before rolling back the
        // role-separated prompt migration itself.
        Migrator::down(&db, Some(3)).await.unwrap();

        let rows = db
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                r#"
                SELECT "id", "prompt_path"
                FROM "agent_runs"
                ORDER BY "id";
                "#
                .to_owned(),
            ))
            .await
            .unwrap();
        assert_that!(&(rows.len())).is_equal_to(2);
        assert_that!(&(rows[0].try_get::<i64>("", "id").unwrap())).is_equal_to(41);
        assert_that!(&(rows[0].try_get::<String>("", "prompt_path").unwrap()))
            .is_equal_to(shared_prompt_path.to_string_lossy());

        let combined_prompt_path = temp.path().join("run-42.prompt.md");
        assert_that!(&(rows[1].try_get::<i64>("", "id").unwrap())).is_equal_to(42);
        assert_that!(&(rows[1].try_get::<String>("", "prompt_path").unwrap()))
            .is_equal_to(combined_prompt_path.to_string_lossy());
        assert_that!(&(fs::read_to_string(combined_prompt_path).unwrap())).is_equal_to(concat!(
            "# Dispatch Automation Prompt\n\n",
            "## Developer Instructions\n\n",
            "Follow Dispatch policy.\n\n",
            "## User Prompt\n\n",
            "Implement the requested change.\n",
        ));

        let columns = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"
                SELECT
                    SUM(name = 'developer_instructions_path') AS developer_count,
                    SUM(name = 'user_prompt_path') AS user_count,
                    SUM(name = 'prompt_path') AS combined_count
                FROM pragma_table_info('agent_runs');
                "#
                .to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_that!(&(columns.try_get::<i64>("", "developer_count").unwrap())).is_equal_to(0);
        assert_that!(&(columns.try_get::<i64>("", "user_count").unwrap())).is_equal_to(0);
        assert_that!(&(columns.try_get::<i64>("", "combined_count").unwrap())).is_equal_to(1);
    }

    #[tokio::test]
    async fn refinement_concurrency_column_is_removed_from_projects() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dispatch.sqlite3");
        let url = sqlite_url(&path);
        let db = Database::connect(&url).await.unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();

        let migration_count_before_cleanup = Migrator::migrations()
            .iter()
            .position(|migration| {
                migration.name() == "m20260618_000032_remove_refinement_concurrency_setting"
            })
            .unwrap() as u32;
        Migrator::up(&db, Some(migration_count_before_cleanup))
            .await
            .unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                r#"ALTER TABLE "projects" ADD COLUMN "{REMOVED_REFINEMENT_CONCURRENCY_COLUMN}" BOOLEAN NOT NULL DEFAULT 0;"#
            ),
        ))
        .await
        .unwrap();

        Migrator::up(&db, None).await.unwrap();
        let row = db
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                r#"
                SELECT COUNT(*) AS count
                FROM pragma_table_info('projects')
                WHERE name = ?1;
                "#
                .to_owned(),
                vec![REMOVED_REFINEMENT_CONCURRENCY_COLUMN.into()],
            ))
            .await
            .unwrap()
            .unwrap();
        let count: i64 = row.try_get("", "count").unwrap();

        assert_that!(&(count)).is_equal_to(0);
    }

    #[tokio::test]
    async fn run_mutability_migration_defaults_existing_rows() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dispatch.sqlite3");
        let url = sqlite_url(&path);
        let db = Database::connect(&url).await.unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();

        let migration_count_before_mutability = Migrator::migrations()
            .iter()
            .position(|migration| {
                migration.name() == "m20260618_000034_add_automation_run_mutability"
            })
            .unwrap() as u32;
        Migrator::up(&db, Some(migration_count_before_mutability))
            .await
            .unwrap();
        for statement in [
            r#"DROP VIEW IF EXISTS "projects_read_view";"#,
            r#"DROP VIEW IF EXISTS "agent_runs_read_view";"#,
            r#"DROP VIEW IF EXISTS "automation_triggers_read_view";"#,
            r#"ALTER TABLE "projects" DROP COLUMN "max_read_only_agents";"#,
            r#"ALTER TABLE "agent_runs" DROP COLUMN "mutability";"#,
            r#"ALTER TABLE "automation_triggers" DROP COLUMN "mutability";"#,
            r#"INSERT INTO "projects" ("name", "display_name") VALUES ('demo', 'Demo');"#,
            r#"INSERT INTO "automation_triggers" ("project_id", "name", "enabled", "activation", "tool_name") VALUES (1, 'Legacy trigger', 1, 'work_item', 'codex');"#,
            r#"INSERT INTO "agent_runs" ("project_id", "tool_name", "status", "command", "working_dir") VALUES (1, 'codex', 'running', '', '/tmp/demo');"#,
        ] {
            db.execute(Statement::from_string(
                DbBackend::Sqlite,
                statement.to_owned(),
            ))
            .await
            .unwrap();
        }

        Migrator::up(&db, None).await.unwrap();
        let project = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"SELECT "max_read_only_agents" FROM "projects" WHERE "id" = 1;"#.to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        let trigger = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"SELECT "mutability" FROM "automation_triggers" WHERE "id" = 1;"#.to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        let run = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"SELECT "mutability" FROM "agent_runs" WHERE "id" = 1;"#.to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();

        assert_that!(&(project.try_get::<i64>("", "max_read_only_agents").unwrap())).is_equal_to(2);
        assert_that!(&(trigger.try_get::<String>("", "mutability").unwrap()))
            .is_equal_to("mutating");
        assert_that!(&(run.try_get::<String>("", "mutability").unwrap())).is_equal_to("mutating");
    }

    #[tokio::test]
    async fn personality_migration_seeds_defaults_and_backfills_triggers() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dispatch.sqlite3");
        let url = sqlite_url(&path);
        let db = Database::connect(&url).await.unwrap();
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_keys = ON".to_owned(),
        ))
        .await
        .unwrap();

        let migration_count_before_personalities = Migrator::migrations()
            .iter()
            .position(|migration| {
                migration.name() == "m20260619_000036_add_automation_personalities"
            })
            .unwrap() as u32;
        Migrator::up(&db, Some(migration_count_before_personalities))
            .await
            .unwrap();
        for statement in [
            r#"INSERT INTO "projects" ("name", "display_name") VALUES ('demo', 'Demo');"#,
            r#"INSERT INTO "automation_triggers" ("project_id", "name", "enabled", "activation", "effect", "schedule", "tool_name", "mutability") VALUES (1, 'Legacy trigger', 1, 'work_item', 'consume_work', '@every 15s', 'codex', 'mutating');"#,
        ] {
            db.execute(Statement::from_string(
                DbBackend::Sqlite,
                statement.to_owned(),
            ))
            .await
            .unwrap();
        }

        Migrator::up(&db, None).await.unwrap();
        let personality = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"
                SELECT "id", "personality_description"
                FROM "personalities"
                WHERE "project_id" = 1
                  AND "name" = 'Default';
                "#
                .to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        let default_id: i64 = personality.try_get("", "id").unwrap();
        assert_that!(
            &(personality
                .try_get::<String>("", "personality_description")
                .unwrap())
        )
        .is_equal_to("");

        let trigger = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                r#"
                SELECT "personality_id", "personality_name"
                FROM "automation_triggers_read_view"
                WHERE "id" = 1;
                "#
                .to_owned(),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_that!(&(trigger.try_get::<i64>("", "personality_id").unwrap()))
            .is_equal_to(default_id);
        assert_that!(&(trigger.try_get::<String>("", "personality_name").unwrap()))
            .is_equal_to("Default");
    }

    #[test]
    fn migration_history_names_all_current_migrations() {
        let migrations = Migrator::migrations();
        let names = migrations
            .iter()
            .map(|migration| migration.name())
            .collect::<Vec<_>>();
        let expected = [
            "migrations",
            "m20260612_000002_add_phase_two_coordination",
            "m20260612_000003_add_project_context",
            "m20260612_000004_add_phase_three_automation",
            "m20260612_000005_add_phase_three_workspace_policy",
            "m20260612_000006_add_phase_four_hardening",
            "m20260612_000007_add_project_default_agent_tool",
            "m20260613_000008_move_run_settings_into_projects",
            "m20260613_000009_drop_claude_code_support",
            "m20260613_000010_rename_project_repo_path",
            "m20260613_000011_add_project_path_status",
            "m20260613_000012_add_automation_run_configuration",
            "m20260614_000013_remove_automation_trigger_dry_run",
            "m20260614_000014_add_automation_run_trigger_origin",
            "m20260614_000015_add_project_memory_events",
            "m20260615_000016_remove_work_item_automation_claimable",
            "m20260615_000017_add_labels_and_swim_lanes",
            "m20260615_000018_add_automation_work_item_selectors",
            "m20260615_000018_rename_automation_activation_require_schedule",
            "m20260615_000019_add_automation_work_item_selectors",
            "m20260615_000020_rename_automation_activation_require_schedule",
            "m20260615_000021_add_work_item_state_label_read_view",
            "m20260616_000022_add_swim_lane_create_item_flag",
            "m20260616_000023_add_project_agent_extra_writable_roots",
            "m20260616_000024_add_project_agent_sandbox_mode",
            "m20260617_000025_decouple_states_and_swim_lanes",
            "m20260617_000026_add_project_commit_policy",
            "m20260617_000027_add_project_agent_git_command_policy",
            "m20260617_000028_add_automation_run_commit_outcomes",
            "m20260617_000029_add_automation_run_token_usage",
            "m20260618_000030_add_refiner_verifier_automations",
            "m20260618_000031_remove_automation_modes",
            "m20260618_000032_remove_refinement_concurrency_setting",
            "m20260618_000033_add_feedback_request_workflow",
            "m20260618_000034_add_automation_run_mutability",
            "m20260618_000035_add_work_item_relationships",
            "m20260619_000036_add_automation_personalities",
            "m20260710_000037_separate_automation_run_inputs",
            "m20260713_000038_add_automation_workflow_support",
            "m20260714_000039_add_work_item_groups",
        ];

        assert_that!(&(names.as_slice())).is_equal_to(expected.as_slice());
    }
}
