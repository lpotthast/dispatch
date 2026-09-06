//! Migration compatibility, interruption, and upgrade-boundary regression tests.

use assertr::prelude::*;
use sea_orm::{ConnectionTrait, Database, DatabaseConnection, DbBackend, QueryResult, Statement};
use sea_orm_migration::{MigrationTrait, MigratorTrait};
use tempfile::TempDir;
use uuid::Uuid;

use crate::backend::migrations::Migrator;

use super::sqlite_url;

async fn connect_sqlite_with_foreign_keys(path: &std::path::Path) -> DatabaseConnection {
    let db = Database::connect(sqlite_url(path)).await.unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "PRAGMA foreign_keys = ON".to_owned(),
    ))
    .await
    .unwrap();
    db
}

#[tokio::test]
async fn knowledge_foundation_upgrade_preserves_legacy_rows_and_creates_no_operational_rows() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let before_foundations = Migrator::migrations()
        .iter()
        .position(|migration| {
            migration.name() == "m20260904_000045_add_knowledge_operational_foundations"
        })
        .unwrap() as u32;
    Migrator::up(&db, Some(before_foundations)).await.unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO projects (name, display_name) VALUES ('legacy', 'Legacy')"#.to_owned(),
    ))
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO agent_runs (project_id, run_kind, tool_name, mutability, status, command, working_dir) VALUES (1, 'task', 'codex', 'mutating', 'completed', 'old command', '/old/workspace')"#.to_owned(),
    ))
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO knowledge_source_baselines (project_id, run_id, knowledge_revision, baseline_kind, baseline_json, baseline_hash) VALUES (1, 1, 'revision', 'git', '{"old":true}', 'baseline-hash')"#.to_owned(),
    ))
    .await
    .unwrap();

    Migrator::up(&db, None).await.unwrap();
    let legacy = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT b.id, b.project_id, b.run_id, b.knowledge_revision, b.baseline_kind, b.baseline_json, b.baseline_hash, r.source_baseline_id, r.source_snapshot_id, r.knowledge_view_sha256, p.knowledge_source_lineage_id FROM knowledge_source_baselines b JOIN agent_runs r ON r.id = b.run_id JOIN projects p ON p.id = b.project_id".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    let mut counts = Vec::new();
    for table in [
        "knowledge_external_artifacts",
        "knowledge_ignore_snapshots",
        "knowledge_source_snapshots",
        "knowledge_cycles",
        "knowledge_cycle_events",
        "knowledge_cycle_runs",
        "knowledge_cycle_artifacts",
        "knowledge_cycle_event_artifacts",
        "knowledge_source_snapshot_artifacts",
        "knowledge_ignore_snapshot_artifacts",
        "knowledge_execution_projects",
        "knowledge_cycle_execution_identities",
        "knowledge_active_bootstrap_cycles",
        "knowledge_cycle_shards",
        "knowledge_lease_slots",
        "knowledge_lease_acquisitions",
        "knowledge_computation_outputs",
        "agent_run_launch_contracts",
        "agent_run_skill_provenance",
        "knowledge_cycle_run_bindings",
        "knowledge_cycle_run_outcomes",
    ] {
        let row = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT COUNT(*) AS count FROM {table}"),
            ))
            .await
            .unwrap()
            .unwrap();
        counts.push(row.try_get::<i64>("", "count").unwrap());
    }

    assert_that!(&legacy.try_get::<i64>("", "id").unwrap()).is_equal_to(1);
    assert_that!(&legacy.try_get::<i64>("", "project_id").unwrap()).is_equal_to(1);
    assert_that!(&legacy.try_get::<i64>("", "run_id").unwrap()).is_equal_to(1);
    assert_that!(&legacy.try_get::<String>("", "knowledge_revision").unwrap())
        .is_equal_to("revision");
    assert_that!(&legacy.try_get::<String>("", "baseline_kind").unwrap()).is_equal_to("git");
    assert_that!(&legacy.try_get::<String>("", "baseline_json").unwrap())
        .is_equal_to("{\"old\":true}");
    assert_that!(&legacy.try_get::<String>("", "baseline_hash").unwrap())
        .is_equal_to("baseline-hash");
    assert_that!(
        &legacy
            .try_get::<Option<i64>>("", "source_baseline_id")
            .unwrap()
    )
    .is_none();
    assert_that!(
        &legacy
            .try_get::<Option<String>>("", "source_snapshot_id")
            .unwrap()
    )
    .is_none();
    assert_that!(
        &legacy
            .try_get::<Option<String>>("", "knowledge_view_sha256")
            .unwrap()
    )
    .is_none();
    assert_that!(
        &Uuid::parse_str(
            &legacy
                .try_get::<String>("", "knowledge_source_lineage_id")
                .unwrap()
        )
        .is_ok()
    )
    .is_true();
    assert_that!(&counts).is_equal_to(vec![0; 21]);
}

#[tokio::test]
async fn source_view_upgrade_backfills_lineage_without_synthesizing_bindings() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let before_source_views = Migrator::migrations()
        .iter()
        .position(|migration| {
            migration.name() == "m20260904_000046_generalize_knowledge_source_views"
        })
        .unwrap() as u32;
    Migrator::up(&db, Some(before_source_views)).await.unwrap();
    for statement in [
        r#"INSERT INTO projects (name, display_name) VALUES ('legacy', 'Legacy')"#,
        r#"INSERT INTO agent_runs (project_id, run_kind, tool_name, mutability, status, command, working_dir) VALUES (1, 'task', 'codex', 'mutating', 'completed', 'old command', '/old/workspace')"#,
        r#"INSERT INTO knowledge_ignore_snapshots (id, project_id, store_uuid, schema_version, matcher_schema_version, matcher_version, knowledge_ignore_sha256, state, control_count, parse_failure_count, created_at) VALUES ('018f0000-0000-7000-8000-000000000001', 1, '018f0000-0000-7000-8000-000000000002', 1, 1, 'v1', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'valid', 0, 0, '2026-09-04T20:00:00Z')"#,
        r#"INSERT INTO knowledge_source_snapshots (id, project_id, store_uuid, schema_version, source_kind, snapshot_sha256, source_scope_sha256, knowledge_ignore_sha256, manifest_sha256, ignore_snapshot_id, created_at) VALUES ('018f0000-0000-7000-8000-000000000003', 1, '018f0000-0000-7000-8000-000000000002', 1, 'git', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', '018f0000-0000-7000-8000-000000000001', '2026-09-04T20:00:00Z')"#,
    ] {
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            statement.to_owned(),
        ))
        .await
        .unwrap();
    }

    Migrator::up(&db, None).await.unwrap();
    let lineage = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT p.knowledge_source_lineage_id AS project_lineage, s.source_lineage_id AS snapshot_lineage FROM projects p JOIN knowledge_source_snapshots s ON s.project_id = p.id".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    let run = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT source_snapshot_id, knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head FROM agent_runs WHERE id = 1".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    let project_lineage = lineage.try_get::<String>("", "project_lineage").unwrap();

    assert_that!(&Uuid::parse_str(&project_lineage).is_ok()).is_true();
    assert_that!(&lineage.try_get::<String>("", "snapshot_lineage").unwrap())
        .is_equal_to(project_lineage.clone());
    for column in [
        "source_snapshot_id",
        "knowledge_view_sha256",
        "input_overlay_sha256",
        "source_authority_kind",
        "source_ref_name",
        "source_raw_head",
    ] {
        assert_that!(&run.try_get::<Option<String>>("", column).unwrap()).is_none();
    }

    // The replacement no longer requires lineage for newly created projects. Existing lineage
    // and historical source bindings remain intact above.
    let null_project_lineage = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"INSERT INTO projects (name, display_name, knowledge_source_lineage_id) VALUES ('invalid-lineage', 'Invalid', NULL)"#.to_owned(),
        ))
        .await;
    assert_that!(&null_project_lineage.is_ok()).is_true();

    let incomplete_run_binding = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            r#"UPDATE agent_runs SET source_snapshot_id = '018f0000-0000-7000-8000-000000000003' WHERE id = 1"#.to_owned(),
        ))
        .await;
    assert_that!(&incomplete_run_binding.is_err()).is_true();

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        format!(
            "INSERT INTO knowledge_source_snapshots (id, project_id, store_uuid, source_lineage_id, schema_version, source_kind, snapshot_sha256, source_scope_sha256, source_scope_artifact_sha256, knowledge_ignore_sha256, manifest_sha256, ignore_snapshot_id, created_at) VALUES ('018f0000-0000-7000-8000-000000000004', 1, '018f0000-0000-7000-8000-000000000005', '{project_lineage}', 1, 'git', '{}', '{}', '{}', '{}', '{}', '018f0000-0000-7000-8000-000000000001', '2026-09-04T20:00:00Z')",
            "b".repeat(64),
            "c".repeat(64),
            "e".repeat(64),
            "a".repeat(64),
            "d".repeat(64),
        ),
    ))
    .await
    .unwrap();
    let cross_store_count = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "SELECT COUNT(*) AS count FROM knowledge_source_snapshots WHERE project_id = 1 AND source_lineage_id = '{project_lineage}' AND snapshot_sha256 = '{}'",
                "b".repeat(64)
            ),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_that!(&cross_store_count).is_equal_to(2);

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO projects (name, display_name, knowledge_source_lineage_id) VALUES ('other', 'Other', '018f0000-0000-7000-8000-000000000006')".to_owned(),
    ))
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO agent_runs (project_id, run_kind, tool_name, mutability, status, command, working_dir) VALUES (2, 'task', 'codex', 'mutating', 'running', 'command', '/workspace')"#.to_owned(),
    ))
    .await
    .unwrap();
    let cross_project_binding = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "UPDATE agent_runs SET source_snapshot_id = '018f0000-0000-7000-8000-000000000003', knowledge_view_sha256 = '{}', source_authority_kind = 'canonical' WHERE id = 2",
                "f".repeat(64)
            ),
        ))
        .await;
    assert_that!(&cross_project_binding.is_err()).is_true();

    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        format!(
            "INSERT INTO knowledge_cycles (id, project_id, store_uuid, kind, status, version, knowledge_revision, source_snapshot_id, knowledge_view_sha256, source_authority_kind, ignore_snapshot_id, created_at, updated_at) VALUES ('018f0000-0000-7000-8000-000000000007', 1, '018f0000-0000-7000-8000-000000000002', 'incremental', 'queued', 1, '{}', '018f0000-0000-7000-8000-000000000003', '{}', 'canonical', '018f0000-0000-7000-8000-000000000001', '2026-09-04T20:00:00Z', '2026-09-04T20:00:00Z')",
            "e".repeat(64),
            "f".repeat(64),
        ),
    ))
    .await
    .unwrap();
    let mutate_cycle_binding = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "UPDATE knowledge_cycles SET knowledge_view_sha256 = '{}' WHERE id = '018f0000-0000-7000-8000-000000000007'",
                "e".repeat(64)
            ),
        ))
        .await;
    assert_that!(&mutate_cycle_binding.is_err()).is_true();

    let valid_bootstrap = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO knowledge_cycles (id, project_id, store_uuid, kind, status, version, uninitialized_knowledge_sha256, source_snapshot_id, source_authority_kind, ignore_snapshot_id, created_at, updated_at) VALUES ('018f0000-0000-7000-8000-000000000008', 1, '018f0000-0000-7000-8000-000000000002', 'bootstrap', 'queued', 1, '{}', '018f0000-0000-7000-8000-000000000003', 'canonical', '018f0000-0000-7000-8000-000000000001', '2026-09-04T20:00:00Z', '2026-09-04T20:00:00Z')",
                "a".repeat(64),
            ),
        ))
        .await;
    assert_that!(&valid_bootstrap.is_ok()).is_true();
    let bootstrap_with_view = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO knowledge_cycles (id, project_id, store_uuid, kind, status, version, uninitialized_knowledge_sha256, source_snapshot_id, knowledge_view_sha256, source_authority_kind, ignore_snapshot_id, created_at, updated_at) VALUES ('018f0000-0000-7000-8000-000000000009', 1, '018f0000-0000-7000-8000-000000000002', 'bootstrap', 'queued', 1, '{}', '018f0000-0000-7000-8000-000000000003', '{}', 'canonical', '018f0000-0000-7000-8000-000000000001', '2026-09-04T20:00:00Z', '2026-09-04T20:00:00Z')",
                "a".repeat(64),
                "b".repeat(64),
            ),
        ))
        .await;
    assert_that!(&bootstrap_with_view.is_err()).is_true();
    let broken_checkpoint = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "UPDATE knowledge_cycles SET application_receipt_sha256 = '{}' WHERE id = '018f0000-0000-7000-8000-000000000007'",
                "c".repeat(64),
            ),
        ))
        .await;
    assert_that!(&broken_checkpoint.is_err()).is_true();
}

async fn prepare_source_evidence_before_46(db: &DatabaseConnection) {
    let before_source_views = Migrator::migrations()
        .iter()
        .position(|migration| {
            migration.name() == "m20260904_000046_generalize_knowledge_source_views"
        })
        .unwrap() as u32;
    Migrator::up(db, Some(before_source_views)).await.unwrap();
    for statement in [
        r#"INSERT INTO projects (id, name, display_name) VALUES (1, 'interrupted', 'Interrupted')"#,
        r#"INSERT INTO knowledge_ignore_snapshots (id, project_id, store_uuid, schema_version, matcher_schema_version, matcher_version, knowledge_ignore_sha256, state, control_count, parse_failure_count, created_at) VALUES ('018f0000-0000-7000-8000-000000000001', 1, '018f0000-0000-7000-8000-000000000002', 1, 1, 'v1', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'valid', 0, 0, '2026-09-04T20:00:00Z')"#,
        r#"INSERT INTO knowledge_source_snapshots (id, project_id, store_uuid, schema_version, source_kind, snapshot_sha256, source_scope_sha256, knowledge_ignore_sha256, manifest_sha256, ignore_snapshot_id, created_at) VALUES ('018f0000-0000-7000-8000-000000000003', 1, '018f0000-0000-7000-8000-000000000002', 1, 'git', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', '018f0000-0000-7000-8000-000000000001', '2026-09-04T20:00:00Z')"#,
        r#"INSERT INTO knowledge_external_artifacts (id, project_id, storage_backend, storage_key, media_type, sha256, byte_length, created_at, verified_at) VALUES ('018f0000-0000-7000-8000-000000000004', 1, 'local_file_v1', 'projects/1/018f0000-0000-7000-8000-000000000004/dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 'application/json', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 2, '2026-09-04T20:00:00Z', '2026-09-04T20:00:00Z')"#,
        r#"INSERT INTO knowledge_source_snapshot_artifacts (project_id, source_snapshot_id, artifact_id, purpose, created_at) VALUES (1, '018f0000-0000-7000-8000-000000000003', '018f0000-0000-7000-8000-000000000004', 'manifest', '2026-09-04T20:00:00Z')"#,
    ] {
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            statement.to_owned(),
        ))
        .await
        .unwrap();
    }
}

async fn assert_source_evidence_rebuild_completed(db: &DatabaseConnection, expected_rows: i64) {
    let row = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM knowledge_source_snapshot_artifacts".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&row.try_get::<i64>("", "count").unwrap()).is_equal_to(expected_rows);
    assert_that!(
        &db.query_all(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_key_check".to_owned(),
        ))
        .await
        .unwrap()
    )
    .is_empty();
    let legacy_exists = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT EXISTS (SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = 'knowledge_source_snapshot_artifacts_v45') AS present".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&legacy_exists.try_get::<i64>("", "present").unwrap()).is_equal_to(0);
}

#[tokio::test]
async fn source_evidence_rebuild_resumes_from_v45_only_state() {
    let temp = TempDir::new().unwrap();
    let db = connect_sqlite_with_foreign_keys(&temp.path().join("v45-only.sqlite3")).await;
    prepare_source_evidence_before_46(&db).await;
    db.execute_unprepared(
        "ALTER TABLE knowledge_source_snapshot_artifacts RENAME TO knowledge_source_snapshot_artifacts_v45",
    )
    .await
    .unwrap();

    Migrator::up(&db, None).await.unwrap();

    assert_source_evidence_rebuild_completed(&db, 1).await;
}

#[tokio::test]
async fn source_evidence_rebuild_resumes_when_both_tables_exist() {
    let temp = TempDir::new().unwrap();
    let db = connect_sqlite_with_foreign_keys(&temp.path().join("both-tables.sqlite3")).await;
    prepare_source_evidence_before_46(&db).await;
    db.execute_unprepared(
        "ALTER TABLE knowledge_source_snapshot_artifacts RENAME TO knowledge_source_snapshot_artifacts_v45",
    )
    .await
    .unwrap();
    db.execute_unprepared(
        r#"
        CREATE TABLE knowledge_source_snapshot_artifacts (
            project_id BIGINT NOT NULL,
            source_snapshot_id VARCHAR(36) NOT NULL,
            artifact_id VARCHAR(36) NOT NULL,
            purpose TEXT NOT NULL CHECK (purpose IN ('manifest', 'source_scope')),
            created_at TEXT NOT NULL,
            PRIMARY KEY (project_id, source_snapshot_id, artifact_id, purpose),
            UNIQUE (project_id, source_snapshot_id, purpose),
            FOREIGN KEY (project_id, source_snapshot_id)
                REFERENCES knowledge_source_snapshots (project_id, id) ON DELETE CASCADE,
            FOREIGN KEY (project_id, artifact_id)
                REFERENCES knowledge_external_artifacts (project_id, id)
        )
        "#,
    )
    .await
    .unwrap();
    db.execute_unprepared(
        r#"INSERT INTO knowledge_source_snapshot_artifacts
           (project_id, source_snapshot_id, artifact_id, purpose, created_at)
           VALUES (1, '018f0000-0000-7000-8000-000000000003',
                   '018f0000-0000-7000-8000-000000000004', 'source_scope',
                   '2026-09-04T20:00:01Z')"#,
    )
    .await
    .unwrap();

    Migrator::up(&db, None).await.unwrap();

    assert_source_evidence_rebuild_completed(&db, 2).await;
}

#[tokio::test]
async fn cycle_lifecycle_upgrade_preserves_unprovable_legacy_events_byte_for_byte() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let before_lifecycle = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == "m20260905_000047_add_knowledge_cycle_lifecycle")
        .unwrap() as u32;
    Migrator::up(&db, Some(before_lifecycle)).await.unwrap();
    for statement in [
        r#"INSERT INTO projects (name, display_name, knowledge_source_lineage_id) VALUES ('legacy-cycle', 'Legacy cycle', '018f0000-0000-7000-8000-000000000001')"#,
        r#"INSERT INTO knowledge_ignore_snapshots (id, project_id, store_uuid, schema_version, matcher_schema_version, matcher_version, knowledge_ignore_sha256, state, control_count, parse_failure_count, created_at) VALUES ('018f0000-0000-7000-8000-000000000002', 1, '018f0000-0000-7000-8000-000000000003', 1, 1, 'v1', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'valid', 0, 0, '2026-09-05T10:00:00Z')"#,
        r#"INSERT INTO knowledge_source_snapshots (id, project_id, store_uuid, source_lineage_id, schema_version, source_kind, snapshot_sha256, source_scope_sha256, source_scope_artifact_sha256, knowledge_ignore_sha256, manifest_sha256, ignore_snapshot_id, created_at) VALUES ('018f0000-0000-7000-8000-000000000004', 1, '018f0000-0000-7000-8000-000000000003', '018f0000-0000-7000-8000-000000000001', 1, 'git', 'bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb', 'cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc', 'dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee', '018f0000-0000-7000-8000-000000000002', '2026-09-05T10:00:00Z')"#,
        r#"INSERT INTO knowledge_cycles (id, project_id, store_uuid, kind, status, version, knowledge_revision, source_snapshot_id, knowledge_view_sha256, source_authority_kind, ignore_snapshot_id, created_at, updated_at) VALUES ('018f0000-0000-7000-8000-000000000005', 1, '018f0000-0000-7000-8000-000000000003', 'incremental', 'queued', 1, 'ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff', '018f0000-0000-7000-8000-000000000004', '9999999999999999999999999999999999999999999999999999999999999999', 'canonical', '018f0000-0000-7000-8000-000000000002', '2026-09-05T10:00:00Z', '2026-09-05T10:00:00Z')"#,
        r#"INSERT INTO knowledge_cycle_events (id, project_id, cycle_id, sequence, kind, prior_version, resulting_version, actor_kind, created_at) VALUES ('018f0000-0000-7000-8000-000000000006', 1, '018f0000-0000-7000-8000-000000000005', 1, 'created', 0, 1, 'service', '2026-09-05T10:00:00Z')"#,
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
            "SELECT event_schema_version, event_json, event_sha256 FROM knowledge_cycle_events"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        &row.try_get::<Option<i64>>("", "event_schema_version")
            .unwrap()
    )
    .is_none();
    assert_that!(&row.try_get::<Option<String>>("", "event_json").unwrap()).is_none();
    assert_that!(&row.try_get::<Option<String>>("", "event_sha256").unwrap()).is_none();

    let append_only = db
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE knowledge_cycle_events SET actor_kind = 'human'".to_owned(),
        ))
        .await;
    assert_that!(&append_only.is_err()).is_true();

    Migrator::down(
        &db,
        Some(
            migration_count_to_roll_back_through(
                &db,
                "m20260905_000047_add_knowledge_cycle_lifecycle",
            )
            .await
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let preserved = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT event_json, event_sha256 FROM knowledge_cycle_events".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        &preserved
            .try_get::<Option<String>>("", "event_json")
            .unwrap()
    )
    .is_none();
    assert_that!(
        &preserved
            .try_get::<Option<String>>("", "event_sha256")
            .unwrap()
    )
    .is_none();
}

#[tokio::test]
async fn execution_fencing_upgrade_down_and_reapply_are_empty_and_non_destructive() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let before_execution_fencing = Migrator::migrations()
        .iter()
        .position(|migration| {
            migration.name() == "m20260905_000048_add_knowledge_execution_fencing"
        })
        .unwrap() as u32;
    Migrator::up(&db, Some(before_execution_fencing))
        .await
        .unwrap();
    let absent = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_lease_slots'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_that!(&absent).is_equal_to(0);

    Migrator::up(&db, None).await.unwrap();
    for table in [
        "knowledge_execution_projects",
        "knowledge_cycle_execution_identities",
        "knowledge_active_bootstrap_cycles",
        "knowledge_cycle_shards",
        "knowledge_lease_slots",
        "knowledge_lease_acquisitions",
        "knowledge_computation_outputs",
    ] {
        let count = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT COUNT(*) AS count FROM {table}"),
            ))
            .await
            .unwrap()
            .unwrap()
            .try_get::<i64>("", "count")
            .unwrap();
        assert_that!(&count).is_equal_to(0);
    }
    Migrator::down(
        &db,
        Some(
            migration_count_to_roll_back_through(
                &db,
                "m20260905_000048_add_knowledge_execution_fencing",
            )
            .await
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    let retained = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name = 'knowledge_lease_slots'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_that!(&retained).is_equal_to(1);
    Migrator::up(&db, None).await.unwrap();
}

#[tokio::test]
async fn knowledge_foundation_down_and_reapply_preserve_durable_rows() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO projects (name, display_name, knowledge_source_lineage_id) VALUES ('durable', 'Durable', '018f0000-0000-7000-8000-000000000000')"#.to_owned(),
    ))
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO knowledge_external_artifacts (id, project_id, storage_backend, storage_key, media_type, sha256, byte_length, created_at, verified_at) VALUES ('018f0000-0000-7000-8000-000000000001', 1, 'local_file_v1', 'projects/1/018f0000-0000-7000-8000-000000000001/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 'application/json', 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa', 2, '2026-09-04T20:00:00Z', '2026-09-04T20:00:00Z')".to_owned(),
    ))
    .await
    .unwrap();

    Migrator::down(
        &db,
        Some(
            migration_count_to_roll_back_through(
                &db,
                "m20260904_000045_add_knowledge_operational_foundations",
            )
            .await
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let row = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT id, sha256 FROM knowledge_external_artifacts".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();

    assert_that!(&row.try_get::<String>("", "id").unwrap())
        .is_equal_to("018f0000-0000-7000-8000-000000000001");
    assert_that!(&row.try_get::<String>("", "sha256").unwrap())
        .is_equal_to("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
}

pub(super) async fn migration_count_to_roll_back_through(
    db: &DatabaseConnection,
    name: &str,
) -> Result<u32, sea_orm::DbErr> {
    let migrations = Migrator::get_applied_migrations(db).await?;
    let position = migrations
        .iter()
        .position(|migration| migration.name() == name)
        .ok_or_else(|| sea_orm::DbErr::Migration(format!("migration '{name}' is not applied")))?;
    (migrations.len() - position)
        .try_into()
        .map_err(|_| sea_orm::DbErr::Migration("migration rollback count overflowed".to_owned()))
}

struct ThroughKnowledgeDirectoryMigrator;

#[async_trait::async_trait]
impl MigratorTrait for ThroughKnowledgeDirectoryMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Migrator::migrations()
            .into_iter()
            .take_while(|migration| {
                migration.name() != "m20260904_000044_migrate_projects_to_newest_gpt_model"
            })
            .collect()
    }
}

struct GapMigrator;

#[async_trait::async_trait]
impl MigratorTrait for GapMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        const RESTORED: [&str; 8] = [
            "m20260904_000044_migrate_projects_to_newest_gpt_model",
            "m20260904_000045_add_knowledge_operational_foundations",
            "m20260904_000046_generalize_knowledge_source_views",
            "m20260905_000047_add_knowledge_cycle_lifecycle",
            "m20260905_000048_add_knowledge_execution_fencing",
            "m20260905_000049_support_knowledge_agent_runs",
            "m20260905_000050_add_knowledge_inventory_impact",
            "m20260905_000051_add_knowledge_evidence_proposals",
        ];
        Migrator::migrations()
            .into_iter()
            .take_while(|migration| migration.name() != "m20260905_000055_retire_legacy_knowledge")
            .filter(|migration| !RESTORED.contains(&migration.name()))
            .collect()
    }
}

struct ThroughMutationOperationsMigrator;

#[async_trait::async_trait]
impl MigratorTrait for ThroughMutationOperationsMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        migrations_through("m20260905_000052_add_knowledge_mutation_operations")
    }
}

struct ThroughMutationBindingMigrator;

#[async_trait::async_trait]
impl MigratorTrait for ThroughMutationBindingMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        migrations_through("m20260905_000053_add_knowledge_mutation_binding")
    }
}

fn migrations_through(name: &str) -> Vec<Box<dyn MigrationTrait>> {
    let mut found = false;
    let migrations = Migrator::migrations()
        .into_iter()
        .take_while(|migration| {
            if found {
                return false;
            }
            if migration.name() == name {
                found = true;
            }
            true
        })
        .collect::<Vec<_>>();
    assert_that!(&found).is_true();
    migrations
}

async fn drop_mutation_hardening_triggers(db: &DatabaseConnection) {
    for name in [
        "trg_knowledge_mutation_operation_refs_insert",
        "trg_knowledge_mutation_operation_refs_update",
        "trg_knowledge_mutation_receipt_insert",
        "trg_knowledge_mutation_receipt_update",
        "trg_agent_runs_preserve_knowledge_mutation_project",
        "trg_knowledge_proposals_preserve_mutation_project",
    ] {
        db.execute_unprepared(&format!("DROP TRIGGER IF EXISTS {name}"))
            .await
            .unwrap();
    }
}

async fn normalized_schema(db: &DatabaseConnection) -> Vec<(String, String, String, String)> {
    db.query_all(Statement::from_string(
        DbBackend::Sqlite,
        r#"
        SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
          AND name != 'seaql_migrations'
        ORDER BY type, name;
        "#
        .to_owned(),
    ))
    .await
    .unwrap()
    .into_iter()
    .map(normalize_schema_row)
    .collect()
}

fn normalize_schema_row(row: QueryResult) -> (String, String, String, String) {
    let sql = row.try_get::<String>("", "sql").unwrap();
    (
        row.try_get("", "type").unwrap(),
        row.try_get("", "name").unwrap(),
        row.try_get("", "tbl_name").unwrap(),
        sql.split_whitespace().collect::<Vec<_>>().join(" "),
    )
}

#[tokio::test]
async fn fresh_and_upgrade_from_43_have_identical_schema() {
    let temp = TempDir::new().unwrap();
    let fresh = Database::connect(sqlite_url(&temp.path().join("fresh.sqlite3")))
        .await
        .unwrap();
    let upgraded = Database::connect(sqlite_url(&temp.path().join("upgraded.sqlite3")))
        .await
        .unwrap();

    Migrator::up(&fresh, None).await.unwrap();
    ThroughKnowledgeDirectoryMigrator::up(&upgraded, None)
        .await
        .unwrap();
    for statement in [
        r#"INSERT INTO projects (id, name, display_name, default_agent_model, default_agent_reasoning_effort) VALUES (1, 'legacy', 'Legacy', 'gpt-5.5', 'high')"#,
        r#"INSERT INTO agent_runs (id, project_id, run_kind, tool_name, mutability, status, command, working_dir) VALUES (1, 1, 'task', 'codex', 'mutating', 'completed', 'legacy', '/legacy')"#,
        r#"INSERT INTO knowledge_source_baselines (id, project_id, run_id, knowledge_revision, baseline_kind, baseline_json, baseline_hash) VALUES (1, 1, 1, 'legacy-revision', 'git', '{}', 'legacy-hash')"#,
    ] {
        upgraded
            .execute(Statement::from_string(
                DbBackend::Sqlite,
                statement.to_owned(),
            ))
            .await
            .unwrap();
    }
    Migrator::up(&upgraded, None).await.unwrap();

    assert_that!(&normalized_schema(&upgraded).await).is_equal_to(normalized_schema(&fresh).await);
    let legacy = upgraded
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            SELECT p.default_agent_model, p.default_agent_reasoning_effort,
                   COUNT(DISTINCT r.id) AS run_count,
                   COUNT(DISTINCT b.id) AS baseline_count
            FROM projects p
            LEFT JOIN agent_runs r ON r.project_id = p.id
            LEFT JOIN knowledge_source_baselines b ON b.project_id = p.id
            WHERE p.id = 1
            GROUP BY p.id;
            "#
            .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&legacy.try_get::<String>("", "default_agent_model").unwrap())
        .is_equal_to("gpt-5.6-sol");
    assert_that!(
        &legacy
            .try_get::<String>("", "default_agent_reasoning_effort")
            .unwrap()
    )
    .is_equal_to("xhigh");
    assert_that!(&legacy.try_get::<i64>("", "run_count").unwrap()).is_equal_to(1);
    assert_that!(&legacy.try_get::<i64>("", "baseline_count").unwrap()).is_equal_to(1);
    assert_that!(
        &upgraded
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_check".to_owned(),
            ))
            .await
            .unwrap()
    )
    .is_empty();
}

#[tokio::test]
async fn restored_migrations_repair_the_52_53_gap() {
    let temp = TempDir::new().unwrap();
    let gap = connect_sqlite_with_foreign_keys(&temp.path().join("gap.sqlite3")).await;
    let fresh = connect_sqlite_with_foreign_keys(&temp.path().join("fresh-gap.sqlite3")).await;
    GapMigrator::up(&gap, None).await.unwrap();
    gap.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO projects (id, name, display_name, default_agent_model, default_agent_reasoning_effort) VALUES (1, 'gap', 'Gap', 'gpt-5.4-mini', 'high')".to_owned(),
    ))
    .await
    .unwrap();
    let operation_id = "018f0000-0000-7000-8000-000000000001";
    gap.execute(Statement::from_string(
        DbBackend::Sqlite,
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, transaction_id, source_knowledge_directory, canonical_timestamp) VALUES ('{operation_id}', 1, 'change', '{}', '{operation_id}', 'knowledge', '2026-09-05T00:00:00Z')",
            "a".repeat(64),
        ),
    ))
    .await
    .unwrap();

    Migrator::up(&gap, None).await.unwrap();
    Migrator::up(&fresh, None).await.unwrap();

    let restored_count = gap
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            r#"
            SELECT COUNT(*) AS count
            FROM seaql_migrations
            WHERE version >= 'm20260904_000044'
              AND version <= 'm20260905_000051_zzzz';
            "#
            .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_that!(&restored_count).is_equal_to(8);
    let operation = gap
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "SELECT operation_id, expected_change_summary, expected_compression_note FROM knowledge_mutation_operations WHERE operation_id = '{operation_id}'"
            ),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&operation.try_get::<String>("", "operation_id").unwrap())
        .is_equal_to(operation_id);
    assert_that!(
        &operation
            .try_get::<String>("", "expected_change_summary")
            .unwrap()
    )
    .is_equal_to("");
    assert_that!(
        &operation
            .try_get::<String>("", "expected_compression_note")
            .unwrap()
    )
    .is_equal_to("");
    let project = gap
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT default_agent_model, default_agent_reasoning_effort FROM projects WHERE id = 1"
                .to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(
        &project
            .try_get::<String>("", "default_agent_model")
            .unwrap()
    )
    .is_equal_to("gpt-5.4-mini");
    assert_that!(
        &project
            .try_get::<String>("", "default_agent_reasoning_effort")
            .unwrap()
    )
    .is_equal_to("high");
    assert_that!(
        &gap.query_all(Statement::from_string(
            DbBackend::Sqlite,
            "PRAGMA foreign_key_check".to_owned(),
        ))
        .await
        .unwrap()
    )
    .is_empty();
    assert_that!(&normalized_schema(&gap).await).is_equal_to(normalized_schema(&fresh).await);
}

#[tokio::test]
async fn mutation_ledger_downgrade_refuses_rows_at_52_and_53() {
    for (name, through_53) in [
        ("m20260905_000052_add_knowledge_mutation_operations", false),
        ("m20260905_000053_add_knowledge_mutation_binding", true),
    ] {
        let temp = TempDir::new().unwrap();
        let db = connect_sqlite_with_foreign_keys(
            &temp
                .path()
                .join(format!("{}.sqlite3", name.replace(':', "_"))),
        )
        .await;
        if through_53 {
            ThroughMutationBindingMigrator::up(&db, None).await.unwrap();
        } else {
            ThroughMutationOperationsMigrator::up(&db, None)
                .await
                .unwrap();
        }
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            "INSERT INTO projects (id, name, display_name, knowledge_source_lineage_id) VALUES (1, 'ledger', 'Ledger', '018f0000-0000-7000-8000-000000000001')"
                .to_owned(),
        ))
        .await
        .unwrap();
        let operation_id = if through_53 {
            "018f0000-0000-7000-8000-000000000053"
        } else {
            "018f0000-0000-7000-8000-000000000052"
        };
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            format!(
                "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, transaction_id, source_knowledge_directory, canonical_timestamp) VALUES ('{operation_id}', 1, 'change', '{}', '{operation_id}', 'knowledge', '2026-09-05T00:00:00Z')",
                "a".repeat(64),
            ),
        ))
        .await
        .unwrap();

        let error = if through_53 {
            ThroughMutationBindingMigrator::down(&db, Some(1))
                .await
                .unwrap_err()
        } else {
            ThroughMutationOperationsMigrator::down(&db, Some(1))
                .await
                .unwrap_err()
        };
        assert_that!(&error.to_string()).contains("durable rows");
        let history = db
            .query_one(Statement::from_sql_and_values(
                DbBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM seaql_migrations WHERE version = ?",
                [name.into()],
            ))
            .await
            .unwrap()
            .unwrap();
        assert_that!(&history.try_get::<i64>("", "count").unwrap()).is_equal_to(1);

        Migrator::up(&db, None).await.unwrap();
        let operation = db
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!(
                    "SELECT operation_id, expected_change_summary, expected_compression_note FROM knowledge_mutation_operations WHERE operation_id = '{operation_id}'"
                ),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_that!(&operation.try_get::<String>("", "operation_id").unwrap())
            .is_equal_to(operation_id);
        assert_that!(
            &operation
                .try_get::<String>("", "expected_change_summary")
                .unwrap()
        )
        .is_equal_to("");
        assert_that!(
            &operation
                .try_get::<String>("", "expected_compression_note")
                .unwrap()
        )
        .is_equal_to("");
    }
}

#[tokio::test]
async fn migration_54_hardens_existing_52_53_schema_and_matches_fresh_schema() {
    let temp = TempDir::new().unwrap();
    let upgraded =
        connect_sqlite_with_foreign_keys(&temp.path().join("mutation-upgraded.sqlite3")).await;
    let fresh = connect_sqlite_with_foreign_keys(&temp.path().join("mutation-fresh.sqlite3")).await;
    ThroughMutationBindingMigrator::up(&upgraded, None)
        .await
        .unwrap();
    drop_mutation_hardening_triggers(&upgraded).await;
    for statement in [
        "INSERT INTO projects (id, name, display_name, knowledge_source_lineage_id) VALUES (1, 'one', 'One', '018f0000-0000-7000-8000-000000000001'), (2, 'two', 'Two', '018f0000-0000-7000-8000-000000000002')".to_owned(),
        "INSERT INTO agent_runs (id, project_id, run_kind, tool_name, mutability, status, command, working_dir) VALUES (1, 1, 'task', 'codex', 'mutating', 'completed', '', ''), (2, 2, 'task', 'codex', 'mutating', 'completed', '', '')".to_owned(),
        "INSERT INTO knowledge_proposals (id, project_id, state, base_revision, summary, change_set_json) VALUES (1, 1, 'pending', 'base', 'one', '{}'), (2, 2, 'pending', 'base', 'two', '{}')".to_owned(),
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, agent_run_id, transaction_id, source_knowledge_directory, canonical_timestamp, proposal_id) VALUES ('018f0000-0000-7000-8000-000000000061', 1, 'proposal', '{}', 1, '018f0000-0000-7000-8000-000000000061', 'knowledge', '2026-09-05T00:00:00Z', 1)",
            "a".repeat(64),
        ),
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, transaction_id, source_knowledge_directory, canonical_timestamp, commit_state, receipt_store_uuid, receipt_transaction_sha256, receipt_resulting_revision, receipt_file_changes_json) VALUES ('018f0000-0000-7000-8000-000000000062', 1, 'initialization', '{}', '018f0000-0000-7000-8000-000000000062', 'knowledge', '2026-09-05T00:00:00Z', 'committed', '018f0000-0000-7000-8000-000000000099', '{}', '{}', '[]')",
            "b".repeat(64),
            "c".repeat(64),
            "d".repeat(64),
        ),
    ] {
        upgraded
            .execute(Statement::from_string(DbBackend::Sqlite, statement))
            .await
            .unwrap();
    }

    Migrator::up(&upgraded, None).await.unwrap();
    Migrator::up(&fresh, None).await.unwrap();

    for sql in [
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, agent_run_id, transaction_id, source_knowledge_directory, canonical_timestamp) VALUES ('018f0000-0000-7000-8000-000000000071', 1, 'change', '{}', 2, '018f0000-0000-7000-8000-000000000071', 'knowledge', '2026-09-05T00:00:00Z')",
            "a".repeat(64),
        ),
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, transaction_id, source_knowledge_directory, canonical_timestamp, proposal_id) VALUES ('018f0000-0000-7000-8000-000000000072', 1, 'proposal', '{}', '018f0000-0000-7000-8000-000000000072', 'knowledge', '2026-09-05T00:00:00Z', 2)",
            "a".repeat(64),
        ),
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, transaction_id, source_knowledge_directory, canonical_timestamp, commit_state) VALUES ('018f0000-0000-7000-8000-000000000073', 1, 'initialization', '{}', '018f0000-0000-7000-8000-000000000073', 'knowledge', '2026-09-05T00:00:00Z', 'committed')",
            "a".repeat(64),
        ),
    ] {
        assert_that!(
            &upgraded
                .execute(Statement::from_string(DbBackend::Sqlite, sql))
                .await
                .is_err()
        )
        .is_true();
    }
    assert_that!(&normalized_schema(&upgraded).await).is_equal_to(normalized_schema(&fresh).await);
    assert_that!(
        &upgraded
            .query_all(Statement::from_string(
                DbBackend::Sqlite,
                "PRAGMA foreign_key_check".to_owned(),
            ))
            .await
            .unwrap()
    )
    .is_empty();
}

#[tokio::test]
async fn migration_54_rejects_invalid_existing_receipts_before_creating_triggers() {
    let temp = TempDir::new().unwrap();
    let db = connect_sqlite_with_foreign_keys(&temp.path().join("invalid-receipt.sqlite3")).await;
    ThroughMutationBindingMigrator::up(&db, None).await.unwrap();
    drop_mutation_hardening_triggers(&db).await;
    db.execute_unprepared(
        "INSERT INTO projects (id, name, display_name, knowledge_source_lineage_id) VALUES (1, 'invalid', 'Invalid', '018f0000-0000-7000-8000-000000000001')",
    )
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        format!(
            "INSERT INTO knowledge_mutation_operations (operation_id, project_id, kind, request_sha256, transaction_id, source_knowledge_directory, canonical_timestamp, commit_state) VALUES ('018f0000-0000-7000-8000-000000000081', 1, 'initialization', '{}', '018f0000-0000-7000-8000-000000000081', 'knowledge', '2026-09-05T00:00:00Z', 'committed')",
            "a".repeat(64),
        ),
    ))
    .await
    .unwrap();

    let error = Migrator::up(&db, None).await.unwrap_err();

    assert_that!(&error.to_string()).contains("committed receipt envelope is invalid");
    let trigger = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM sqlite_schema WHERE type = 'trigger' AND name = 'trg_knowledge_mutation_receipt_insert'".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&trigger.try_get::<i64>("", "count").unwrap()).is_equal_to(0);
}

#[tokio::test]
async fn rollback_count_uses_only_applied_history_and_requires_the_target() {
    let temp = TempDir::new().unwrap();
    let partial =
        connect_sqlite_with_foreign_keys(&temp.path().join("partial-history.sqlite3")).await;
    ThroughMutationOperationsMigrator::up(&partial, None)
        .await
        .unwrap();
    let partial_count = migration_count_to_roll_back_through(
        &partial,
        "m20260905_000051_add_knowledge_evidence_proposals",
    )
    .await
    .unwrap();
    assert_that!(&partial_count).is_equal_to(2);

    let db = connect_sqlite_with_foreign_keys(&temp.path().join("gapped-history.sqlite3")).await;
    Migrator::up(&db, None).await.unwrap();
    db.execute_unprepared(
        "DELETE FROM seaql_migrations WHERE version = 'm20260905_000053_add_knowledge_mutation_binding'",
    )
    .await
    .unwrap();

    let count = migration_count_to_roll_back_through(
        &db,
        "m20260905_000052_add_knowledge_mutation_operations",
    )
    .await
    .unwrap();
    assert_that!(&count).is_equal_to(3);
    let absent = migration_count_to_roll_back_through(
        &db,
        "m20260905_000053_add_knowledge_mutation_binding",
    )
    .await;
    assert_that!(&absent.is_err()).is_true();
}

#[tokio::test]
async fn knowledge_directory_migration_preserves_existing_projects_and_defaults_new_ones() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let migration_count_before_knowledge_directory = Migrator::migrations()
        .iter()
        .position(|migration| {
            migration.name() == "m20260904_000043_add_project_knowledge_directory"
        })
        .unwrap() as u32;
    Migrator::up(&db, Some(migration_count_before_knowledge_directory))
        .await
        .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO "projects" ("name", "display_name") VALUES ('existing', 'Existing');"#
            .to_owned(),
    ))
    .await
    .unwrap();

    Migrator::up(&db, None).await.unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        r#"INSERT INTO "projects" ("name", "display_name") VALUES ('new', 'New');"#.to_owned(),
    ))
    .await
    .unwrap();
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            r#"SELECT "name", "knowledge_directory" FROM "projects" ORDER BY "id";"#.to_owned(),
        ))
        .await
        .unwrap();

    assert_that!(&(rows[0].try_get::<String>("", "name").unwrap())).is_equal_to("existing");
    assert_that!(
        &(rows[0]
            .try_get::<String>("", "knowledge_directory")
            .unwrap())
    )
    .is_equal_to("design");
    assert_that!(&(rows[1].try_get::<String>("", "name").unwrap())).is_equal_to("new");
    assert_that!(
        &(rows[1]
            .try_get::<String>("", "knowledge_directory")
            .unwrap())
    )
    .is_equal_to("knowledge");
}

#[tokio::test]
async fn newest_gpt_model_migration_updates_existing_projects_and_is_irreversible() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let migration_count_before_newest_gpt_model = Migrator::migrations()
        .iter()
        .position(|migration| {
            migration.name() == "m20260904_000044_migrate_projects_to_newest_gpt_model"
        })
        .unwrap() as u32;
    Migrator::up(&db, Some(migration_count_before_newest_gpt_model))
        .await
        .unwrap();
    for statement in [
        r#"INSERT INTO "projects" ("name", "display_name", "default_agent_model", "default_agent_reasoning_effort") VALUES ('old-default', 'Old default', 'gpt-5.5', 'xhigh');"#,
        r#"INSERT INTO "projects" ("name", "display_name", "default_agent_model", "default_agent_reasoning_effort") VALUES ('customized', 'Customized', 'gpt-5.4-mini', 'high');"#,
    ] {
        db.execute(Statement::from_string(
            DbBackend::Sqlite,
            statement.to_owned(),
        ))
        .await
        .unwrap();
    }

    Migrator::up(&db, None).await.unwrap();
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            r#"SELECT "default_agent_model", "default_agent_reasoning_effort" FROM "projects" ORDER BY "id";"#.to_owned(),
        ))
        .await
        .unwrap();

    assert_that!(&(rows.len())).is_equal_to(2);
    for row in rows {
        assert_that!(&(row.try_get::<String>("", "default_agent_model").unwrap()))
            .is_equal_to("gpt-5.6-sol");
        assert_that!(
            &(row
                .try_get::<String>("", "default_agent_reasoning_effort")
                .unwrap())
        )
        .is_equal_to("xhigh");
    }

    Migrator::down(
        &db,
        Some(
            migration_count_to_roll_back_through(
                &db,
                "m20260904_000044_migrate_projects_to_newest_gpt_model",
            )
            .await
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    let rows_after_down = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            r#"SELECT "default_agent_model", "default_agent_reasoning_effort" FROM "projects" ORDER BY "id";"#.to_owned(),
        ))
        .await
        .unwrap();

    assert_that!(&(rows_after_down.len())).is_equal_to(2);
    for row in rows_after_down {
        assert_that!(&(row.try_get::<String>("", "default_agent_model").unwrap()))
            .is_equal_to("gpt-5.6-sol");
        assert_that!(
            &(row
                .try_get::<String>("", "default_agent_reasoning_effort")
                .unwrap())
        )
        .is_equal_to("xhigh");
    }
}

#[tokio::test]
async fn knowledge_agent_run_migration_down_up_is_non_synthetic() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    let before_agent_runs = Migrator::migrations()
        .iter()
        .position(|migration| migration.name() == "m20260905_000049_support_knowledge_agent_runs")
        .unwrap() as u32;
    Migrator::up(&db, Some(before_agent_runs)).await.unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO projects (id, name, display_name, knowledge_source_lineage_id) VALUES (1, 'legacy', 'Legacy', '018f0000-0000-7000-8000-000000000001')".to_owned(),
    ))
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO agent_runs (id, project_id, run_kind, tool_name, mutability, status, command, working_dir) VALUES (1, 1, 'task', 'codex', 'mutating', 'completed', '', '')".to_owned(),
    )).await.unwrap();

    Migrator::up(&db, None).await.unwrap();
    let legacy = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT purpose FROM agent_runs WHERE id = 1".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    let contracts = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM agent_run_launch_contracts".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&legacy.try_get::<Option<String>>("", "purpose").unwrap()).is_none();
    assert_that!(&contracts.try_get::<i64>("", "count").unwrap()).is_equal_to(0);

    Migrator::down(
        &db,
        Some(
            migration_count_to_roll_back_through(
                &db,
                "m20260905_000049_support_knowledge_agent_runs",
            )
            .await
            .unwrap(),
        ),
    )
    .await
    .unwrap();
    Migrator::up(&db, None).await.unwrap();
    let after = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM agent_run_launch_contracts".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap();
    assert_that!(&after.try_get::<i64>("", "count").unwrap()).is_equal_to(0);
}

#[tokio::test]
async fn knowledge_agent_run_migration_refuses_to_drop_any_launch_history() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("dispatch.sqlite3");
    let db = Database::connect(sqlite_url(&path)).await.unwrap();
    Migrator::up(&db, None).await.unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO projects (id, name, display_name) VALUES (1, 'history', 'History')".to_owned(),
    ))
    .await
    .unwrap();
    db.execute(Statement::from_string(
        DbBackend::Sqlite,
        "INSERT INTO agent_runs (id, project_id, run_kind, purpose, tool_name, mutability, status, command, working_dir) VALUES (1, 1, 'task', 'ordinary', 'codex', 'mutating', 'running', '', '')".to_owned(),
    )).await.unwrap();
    db.execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "INSERT INTO agent_run_launch_contracts (run_id, project_id, purpose, state, target_schema_version, target_json, target_sha256, resolution_schema_version, resolution_json, resolution_sha256, capability_schema_version, capability_json, capability_sha256, created_at, updated_at) VALUES (1, 1, 'ordinary', 'target_resolved', 1, ?, ?, 1, ?, ?, 1, ?, ?, '2026-09-05T00:00:00Z', '2026-09-05T00:00:00Z')",
        vec![
            r#"{"kind":"none","schema_version":1}"#.into(),
            "a".repeat(64).into(),
            r#"{"kind":"none","schema_version":1}"#.into(),
            "b".repeat(64).into(),
            r#"{"schema_version":1,"capabilities":[]}"#.into(),
            "c".repeat(64).into(),
        ],
    )).await.unwrap();

    let error = Migrator::down(
        &db,
        Some(
            migration_count_to_roll_back_through(
                &db,
                "m20260905_000049_support_knowledge_agent_runs",
            )
            .await
            .unwrap(),
        ),
    )
    .await
    .unwrap_err();
    assert_that!(&error.to_string()).contains("authoritative rows remain");
    let remaining = db
        .query_one(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT COUNT(*) AS count FROM agent_run_launch_contracts".to_owned(),
        ))
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "count")
        .unwrap();
    assert_that!(&remaining).is_equal_to(1);
}

#[tokio::test]
async fn retirement_stops_reserved_work_and_preserves_ordinary_rules_and_history() {
    let temp = TempDir::new().unwrap();
    let db = connect_sqlite_with_foreign_keys(&temp.path().join("retirement.sqlite3")).await;
    let before = Migrator::migrations()
        .iter()
        .position(|m| m.name() == "m20260905_000055_retire_legacy_knowledge")
        .unwrap() as u32;
    Migrator::up(&db, Some(before)).await.unwrap();
    for sql in [
        "INSERT INTO projects (id,name,display_name,knowledge_source_lineage_id) VALUES (1,'demo','Demo','11111111-1111-4111-8111-111111111111')",
        "INSERT INTO label_keys (project_id,label_key,persistent,built_in,accent_color) VALUES (1,'dispatch:knowledge-drift',TRUE,TRUE,'#123456')",
        "INSERT INTO automation_triggers (id,project_id,name,enabled,activation,effect,schedule,tool_name,prompt,work_item_selector,pending_evaluation_count) VALUES (1,1,'Ordinary',TRUE,'work_item','consume_work','* * * * *','codex','Code work','{\"All\":[{\"column_name\":\"dispatch:knowledge-drift\",\"operator\":\"=\",\"value\":{\"Bool\":false}}]}',2)",
        "INSERT INTO automation_triggers (id,project_id,name,enabled,activation,effect,schedule,tool_name,prompt,work_item_selector,pending_evaluation_count) VALUES (2,1,'Custom drift worker',TRUE,'work_item','consume_work','* * * * *','codex','Original prompt','{\"All\":[{\"column_name\":\"dispatch:knowledge-drift\",\"operator\":\"=\",\"value\":{\"Bool\":true}}]}',3)",
        "INSERT INTO automation_triggers (id,project_id,name,enabled,activation,effect,schedule,tool_name,prompt,work_item_selector) VALUES (3,1,'No selector',TRUE,'cron','consume_work','* * * * *','codex','Ordinary scheduled work','   ')",
        "INSERT INTO work_items (id,project_id,title,description,version) VALUES (1,1,'Old knowledge work','Keep this text',1)",
        "INSERT INTO work_item_labels (project_id,work_item_id,label_key) VALUES (1,1,'dispatch:knowledge-drift')",
        "INSERT INTO agent_runs (id,project_id,run_kind,tool_name,mutability,status,command,working_dir,result_summary) VALUES (1,1,'knowledge_answer','codex','read_only','completed','original command','/old','Historical answer')",
        "INSERT INTO agent_runs (id,project_id,run_kind,tool_name,mutability,status,command,working_dir) VALUES (2,1,'knowledge_answer','codex','read_only','running','interrupted command','/old')",
    ] {
        db.execute(Statement::from_string(DbBackend::Sqlite, sql.to_owned()))
            .await
            .unwrap();
    }
    Migrator::up(&db, None).await.unwrap();
    let rows=db.query_all(Statement::from_string(DbBackend::Sqlite,"SELECT id, enabled, pending_evaluation_count, prompt FROM automation_triggers ORDER BY id".to_owned())).await.unwrap();
    let label = db.query_one(Statement::from_string(
        DbBackend::Sqlite,
        "SELECT built_in, accent_color FROM label_keys WHERE label_key = 'dispatch:knowledge-drift'".to_owned(),
    )).await.unwrap().unwrap();
    assert_that!(&label.try_get::<bool>("", "built_in").unwrap()).is_false();
    assert_that!(&label.try_get::<String>("", "accent_color").unwrap()).is_equal_to("#123456");
    assert_that!(&rows[0].try_get::<bool>("", "enabled").unwrap()).is_true();
    assert_that!(
        &rows[0]
            .try_get::<i64>("", "pending_evaluation_count")
            .unwrap()
    )
    .is_equal_to(2);
    assert_that!(&rows[1].try_get::<bool>("", "enabled").unwrap()).is_false();
    assert_that!(
        &rows[1]
            .try_get::<i64>("", "pending_evaluation_count")
            .unwrap()
    )
    .is_equal_to(0);
    assert_that!(&rows[2].try_get::<bool>("", "enabled").unwrap()).is_true();
    assert_that!(&rows[1].try_get::<String>("", "prompt").unwrap()).is_equal_to("Original prompt");
    let row=db.query_one(Statement::from_string(DbBackend::Sqlite,"SELECT COUNT(*) AS count FROM work_item_labels WHERE work_item_id=1 AND label_key='dispatch:automation-blocked'".to_owned())).await.unwrap().unwrap();
    assert_that!(&row.try_get::<i64>("", "count").unwrap()).is_equal_to(1);
    let rows = db
        .query_all(Statement::from_string(
            DbBackend::Sqlite,
            "SELECT status,result_summary FROM agent_runs ORDER BY id".to_owned(),
        ))
        .await
        .unwrap();
    assert_that!(&rows[0].try_get::<String>("", "result_summary").unwrap())
        .is_equal_to("Historical answer");
    assert_that!(&rows[1].try_get::<String>("", "status").unwrap()).is_equal_to("failed");
}
