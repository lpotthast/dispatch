// SQLite cannot add a table-level foreign key with ALTER TABLE. SeaQuery 0.30
// has no inline REFERENCES builder, so these additive columns retain that clause.
use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DatabaseBackend, Statement, TransactionTrait};
use sea_orm_migration::sea_query::{Func, SimpleExpr};
use uuid::Uuid;

use super::super::create_read_view;
use super::super::drop_read_view;
use super::super::execute_migration_sql;
use super::add_column_if_missing;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260904_000046_generalize_knowledge_source_views"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        generalize_knowledge_source_views(manager).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Source views are durable audit inputs. Older binaries ignore these additive columns;
        // rollback deliberately keeps their schema and values for a later binary to resume.
        Ok(())
    }
}

async fn generalize_knowledge_source_views(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    drop_read_view(manager, "projects_read_view").await?;
    drop_read_view(manager, "agent_runs_read_view").await?;

    add_column_if_missing(
        manager,
        "projects",
        ColumnDef::new(Columns::KnowledgeSourceLineageId)
            .text()
            .check(
                Expr::col(Columns::KnowledgeSourceLineageId)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::KnowledgeSourceLineageId),
                        )]))
                        .eq(36)
                        .and(
                            Expr::col(Columns::KnowledgeSourceLineageId).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::KnowledgeSourceLineageId)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
    )
    .await?;
    let project_query = Query::select()
        .column(Columns::Id)
        .from(Tables::Projects)
        .and_where(Expr::col(Columns::KnowledgeSourceLineageId).is_null())
        .to_owned();
    let projects = manager
        .get_connection()
        .query_all(manager.get_database_backend().build(&project_query))
        .await?;
    for project in projects {
        let project_id = project.try_get::<i64>("", "id")?;
        let update = Query::update()
            .table(Tables::Projects)
            .value(
                Columns::KnowledgeSourceLineageId,
                Uuid::new_v4().to_string(),
            )
            .and_where(Expr::col(Columns::Id).eq(project_id))
            .to_owned();
        manager
            .get_connection()
            .execute(manager.get_database_backend().build(&update))
            .await?;
    }
    manager
        .create_index(
            Index::create()
                .name("uq_projects_knowledge_source_lineage")
                .table(Tables::Projects)
                .col(Columns::KnowledgeSourceLineageId)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await?;

    add_column_if_missing(
        manager,
        "knowledge_source_snapshots",
        ColumnDef::new(Columns::SourceLineageId)
            .text()
            .check(
                Expr::col(Columns::SourceLineageId).is_null().or(Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::SourceLineageId))]),
                )
                .eq(36)
                .and(
                    Expr::col(Columns::SourceLineageId)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::SourceLineageId)))),
                )),
            )
            .to_owned(),
    )
    .await?;
    add_column_if_missing(
        manager,
        "knowledge_source_snapshots",
        ColumnDef::new(Columns::SourceScopeArtifactSha256)
            .text()
            .check(
                Expr::col(Columns::SourceScopeArtifactSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::SourceScopeArtifactSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::SourceScopeArtifactSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::SourceScopeArtifactSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
    )
    .await?;
    manager
        .exec_stmt(
            Query::update()
                .table(Tables::KnowledgeSourceSnapshots)
                .value(
                    Columns::SourceLineageId,
                    Expr::expr(SimpleExpr::SubQuery(
                        None,
                        Box::new(
                            Query::select()
                                .expr(Expr::col((
                                    Tables::Projects,
                                    Columns::KnowledgeSourceLineageId,
                                )))
                                .from(Tables::Projects)
                                .and_where(Expr::col((Tables::Projects, Columns::Id)).eq(
                                    Expr::col((
                                        Tables::KnowledgeSourceSnapshots,
                                        Columns::ProjectId,
                                    )),
                                ))
                                .to_owned()
                                .into_sub_query_statement(),
                        ),
                    )),
                )
                .and_where(Expr::col(Columns::SourceLineageId).is_null())
                .to_owned(),
        )
        .await?;
    manager
        .get_connection()
        .execute(
            manager.get_database_backend().build(
                &Index::drop()
                    .name("uq_knowledge_source_snapshots_project_sha")
                    .if_exists()
                    .to_owned(),
            ),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("uq_knowledge_source_snapshots_lineage_sha")
                .table(Tables::KnowledgeSourceSnapshots)
                .col(Columns::ProjectId)
                .col(Columns::StoreUuid)
                .col(Columns::SourceLineageId)
                .col(Columns::SnapshotSha256)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await?;

    expand_source_snapshot_evidence_table(manager).await?;

    for column in source_view_audit_columns() {
        add_column_if_missing(manager, "agent_runs", column).await?;
    }
    for column in source_view_audit_columns() {
        add_column_if_missing(manager, "knowledge_cycles", column).await?;
    }
    for column in source_view_audit_columns() {
        add_column_if_missing(manager, "knowledge_proposals", column).await?;
    }

    for column in [
        ColumnDef::new(Columns::BaselineSourceSnapshotId)
            .text()
            .extra("REFERENCES knowledge_source_snapshots (id)")
            .to_owned(),
        ColumnDef::new(Columns::BaselineKnowledgeViewSha256)
            .text()
            .check(
                Expr::col(Columns::BaselineKnowledgeViewSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::BaselineKnowledgeViewSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::BaselineKnowledgeViewSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::BaselineKnowledgeViewSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
        ColumnDef::new(Columns::CurrentSourceSnapshotId)
            .text()
            .extra("REFERENCES knowledge_source_snapshots (id)")
            .to_owned(),
        ColumnDef::new(Columns::CurrentKnowledgeViewSha256)
            .text()
            .check(
                Expr::col(Columns::CurrentKnowledgeViewSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::CurrentKnowledgeViewSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::CurrentKnowledgeViewSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::CurrentKnowledgeViewSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
        ColumnDef::new(Columns::InputOverlaySha256)
            .text()
            .check(
                Expr::col(Columns::InputOverlaySha256)
                    .is_null()
                    .or(Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::InputOverlaySha256))]),
                    )
                    .eq(64)
                    .and(
                        Expr::col(Columns::InputOverlaySha256).eq(Expr::expr(Func::lower(
                            Expr::col(Columns::InputOverlaySha256),
                        ))),
                    )),
            )
            .to_owned(),
        source_authority_column(),
        ColumnDef::new(Columns::SourceRefName).text().to_owned(),
        ColumnDef::new(Columns::SourceRawHead).text().to_owned(),
    ] {
        add_column_if_missing(manager, "knowledge_impact_receipts", column).await?;
    }
    for column in [
        ColumnDef::new(Columns::ReviewSourceSnapshotId)
            .text()
            .extra("REFERENCES knowledge_source_snapshots (id)")
            .to_owned(),
        ColumnDef::new(Columns::ReviewKnowledgeViewSha256)
            .text()
            .check(
                Expr::col(Columns::ReviewKnowledgeViewSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::ReviewKnowledgeViewSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::ReviewKnowledgeViewSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::ReviewKnowledgeViewSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
        ColumnDef::new(Columns::InputOverlaySha256)
            .text()
            .check(
                Expr::col(Columns::InputOverlaySha256)
                    .is_null()
                    .or(Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::InputOverlaySha256))]),
                    )
                    .eq(64)
                    .and(
                        Expr::col(Columns::InputOverlaySha256).eq(Expr::expr(Func::lower(
                            Expr::col(Columns::InputOverlaySha256),
                        ))),
                    )),
            )
            .to_owned(),
        source_authority_column(),
        ColumnDef::new(Columns::SourceRefName).text().to_owned(),
        ColumnDef::new(Columns::SourceRawHead).text().to_owned(),
    ] {
        add_column_if_missing(manager, "knowledge_review_receipts", column).await?;
    }

    for (name, table, columns) in [
        (
            "idx_agent_runs_source_snapshot",
            "agent_runs",
            &["project_id", "source_snapshot_id"][..],
        ),
        (
            "idx_knowledge_cycles_view",
            "knowledge_cycles",
            &["project_id", "knowledge_view_sha256"][..],
        ),
        (
            "idx_knowledge_proposals_view",
            "knowledge_proposals",
            &["project_id", "knowledge_view_sha256"][..],
        ),
        (
            "idx_knowledge_impact_current_view",
            "knowledge_impact_receipts",
            &["project_id", "run_id", "current_knowledge_view_sha256"][..],
        ),
        (
            "idx_knowledge_review_view",
            "knowledge_review_receipts",
            &["project_id", "run_id", "review_knowledge_view_sha256"][..],
        ),
    ] {
        let mut index = Index::create();
        index.name(name).table(Alias::new(table)).if_not_exists();
        for column in columns {
            index.col(Alias::new(*column));
        }
        manager.create_index(index.to_owned()).await?;
    }

    create_source_view_integrity_triggers(manager).await?;

    create_read_view(manager, "projects", "projects_read_view").await?;
    create_read_view(manager, "agent_runs", "agent_runs_read_view").await
}

async fn expand_source_snapshot_evidence_table(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DatabaseBackend::Sqlite {
        return Ok(());
    }

    let transaction = manager.get_connection().begin().await?;
    let transaction_manager = SchemaManager::new(&transaction);
    let result = expand_source_snapshot_evidence_table_sqlite(&transaction_manager).await;
    match result {
        Ok(()) => transaction.commit().await,
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

async fn expand_source_snapshot_evidence_table_sqlite(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let mut current_exists = manager
        .has_table("knowledge_source_snapshot_artifacts")
        .await?;
    let legacy_exists = manager
        .has_table("knowledge_source_snapshot_artifacts_v45")
        .await?;

    if !current_exists && !legacy_exists {
        return Err(DbErr::Migration(
            "knowledge source snapshot artifact table is missing".to_owned(),
        ));
    }

    if current_exists && !source_snapshot_evidence_table_is_expanded(manager).await? {
        if legacy_exists {
            return Err(DbErr::Migration(
                "both knowledge source snapshot artifact tables exist, but the current table has the v45 schema"
                    .to_owned(),
            ));
        }
        manager
            .exec_stmt(
                Table::rename()
                    .table(
                        Tables::KnowledgeSourceSnapshotArtifacts,
                        Tables::KnowledgeSourceSnapshotArtifactsV45,
                    )
                    .to_owned(),
            )
            .await?;
        current_exists = false;
    }

    if !current_exists {
        create_expanded_source_snapshot_evidence_table(manager).await?;
    }

    if manager
        .has_table("knowledge_source_snapshot_artifacts_v45")
        .await?
    {
        manager
            .get_connection()
            .execute_unprepared(
                r#"
                INSERT OR IGNORE INTO knowledge_source_snapshot_artifacts (
                    project_id, source_snapshot_id, artifact_id, purpose, created_at
                )
                SELECT project_id, source_snapshot_id, artifact_id, purpose, created_at
                FROM knowledge_source_snapshot_artifacts_v45
                "#,
            )
            .await?;
        let missing = manager
            .get_connection()
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                r#"
                SELECT COUNT(*) AS count
                FROM knowledge_source_snapshot_artifacts_v45 legacy
                WHERE NOT EXISTS (
                    SELECT 1
                    FROM knowledge_source_snapshot_artifacts current
                    WHERE current.project_id = legacy.project_id
                      AND current.source_snapshot_id = legacy.source_snapshot_id
                      AND current.artifact_id = legacy.artifact_id
                      AND current.purpose = legacy.purpose
                      AND current.created_at = legacy.created_at
                )
                "#
                .to_owned(),
            ))
            .await?
            .ok_or_else(|| {
                DbErr::Migration("failed to verify migrated source evidence".to_owned())
            })?
            .try_get::<i64>("", "count")?;
        if missing != 0 {
            return Err(DbErr::Migration(format!(
                "{missing} source evidence rows were not preserved"
            )));
        }
        manager
            .exec_stmt(
                Table::drop()
                    .table(Tables::KnowledgeSourceSnapshotArtifactsV45)
                    .to_owned(),
            )
            .await?;
    }

    manager
        .exec_stmt(
            Index::create()
                .name("idx_knowledge_source_snapshot_artifacts_artifact")
                .table(Tables::KnowledgeSourceSnapshotArtifacts)
                .col(Columns::ArtifactId)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn source_snapshot_evidence_table_is_expanded(
    manager: &SchemaManager<'_>,
) -> Result<bool, DbErr> {
    let row = manager
        .get_connection()
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'knowledge_source_snapshot_artifacts'"
                .to_owned(),
        ))
        .await?
        .ok_or_else(|| DbErr::Migration("knowledge source evidence schema is missing".to_owned()))?;
    Ok(row.try_get::<String>("", "sql")?.contains("source_scope"))
}

async fn create_expanded_source_snapshot_evidence_table(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    manager
        .exec_stmt(
            Table::create()
                .table(Tables::KnowledgeSourceSnapshotArtifacts)
                .col(ColumnDef::new(Columns::ProjectId).big_integer().not_null())
                .col(
                    ColumnDef::new(Columns::SourceSnapshotId)
                        .string_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Columns::ArtifactId)
                        .string_len(36)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(Columns::Purpose).text().not_null().check(
                        Expr::col(Columns::Purpose)
                            .is_in([Expr::val("manifest"), Expr::val("source_scope")]),
                    ),
                )
                .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
                .primary_key(
                    Index::create()
                        .col(Columns::ProjectId)
                        .col(Columns::SourceSnapshotId)
                        .col(Columns::ArtifactId)
                        .col(Columns::Purpose),
                )
                .index(
                    Index::create()
                        .unique()
                        .col(Columns::ProjectId)
                        .col(Columns::SourceSnapshotId)
                        .col(Columns::Purpose),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from_tbl(Tables::KnowledgeSourceSnapshotArtifacts)
                        .from_col(Columns::ProjectId)
                        .from_col(Columns::SourceSnapshotId)
                        .to_tbl(Tables::KnowledgeSourceSnapshots)
                        .to_col(Columns::ProjectId)
                        .to_col(Columns::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from_tbl(Tables::KnowledgeSourceSnapshotArtifacts)
                        .from_col(Columns::ProjectId)
                        .from_col(Columns::ArtifactId)
                        .to_tbl(Tables::KnowledgeExternalArtifacts)
                        .to_col(Columns::ProjectId)
                        .to_col(Columns::Id),
                )
                .to_owned(),
        )
        .await
}

async fn create_source_view_integrity_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    if manager.get_database_backend() != DatabaseBackend::Sqlite {
        return Ok(());
    }
    for sql in [
        r#"CREATE TRIGGER IF NOT EXISTS trg_projects_source_lineage_insert
           BEFORE INSERT ON projects WHEN NEW.knowledge_source_lineage_id IS NULL
           BEGIN SELECT RAISE(ABORT, 'project source lineage is required'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_projects_source_lineage_update
           BEFORE UPDATE OF knowledge_source_lineage_id ON projects
           WHEN NEW.knowledge_source_lineage_id IS NULL
           BEGIN SELECT RAISE(ABORT, 'project source lineage is required'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_source_snapshots_lineage_insert
           BEFORE INSERT ON knowledge_source_snapshots
           WHEN NEW.source_lineage_id IS NULL OR NEW.source_scope_artifact_sha256 IS NULL
             OR NOT EXISTS (SELECT 1 FROM projects p WHERE p.id = NEW.project_id AND p.knowledge_source_lineage_id = NEW.source_lineage_id)
           BEGIN SELECT RAISE(ABORT, 'source snapshot lineage and exact source scope are required'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_source_snapshots_lineage_update
           BEFORE UPDATE OF source_lineage_id, source_scope_artifact_sha256 ON knowledge_source_snapshots
           WHEN NEW.source_lineage_id IS NULL OR NEW.source_scope_artifact_sha256 IS NULL
             OR NEW.source_lineage_id IS NOT OLD.source_lineage_id
             OR NEW.source_scope_artifact_sha256 IS NOT OLD.source_scope_artifact_sha256
             OR NOT EXISTS (SELECT 1 FROM projects p WHERE p.id = NEW.project_id AND p.knowledge_source_lineage_id = NEW.source_lineage_id)
           BEGIN SELECT RAISE(ABORT, 'source snapshot lineage and exact source scope are immutable'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_cycles_source_view_immutable
           BEFORE UPDATE OF source_snapshot_id, knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head ON knowledge_cycles
           WHEN OLD.knowledge_view_sha256 IS NOT NULL AND (
             NEW.source_snapshot_id IS NOT OLD.source_snapshot_id
             OR NEW.knowledge_view_sha256 IS NOT OLD.knowledge_view_sha256
             OR NEW.input_overlay_sha256 IS NOT OLD.input_overlay_sha256
             OR NEW.source_authority_kind IS NOT OLD.source_authority_kind
             OR NEW.source_ref_name IS NOT OLD.source_ref_name
             OR NEW.source_raw_head IS NOT OLD.source_raw_head)
           BEGIN SELECT RAISE(ABORT, 'cycle source-view binding is immutable'); END"#,
    ] {
        execute_migration_sql(manager, sql).await?;
    }
    for (table, operation, timing, required) in [
        ("agent_runs", "insert", "BEFORE INSERT", false),
        (
            "agent_runs",
            "update",
            "BEFORE UPDATE OF source_snapshot_id, knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head",
            false,
        ),
        ("knowledge_proposals", "insert", "BEFORE INSERT", false),
        (
            "knowledge_proposals",
            "update",
            "BEFORE UPDATE OF source_snapshot_id, knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head",
            false,
        ),
        ("knowledge_cycles", "insert", "BEFORE INSERT", true),
        (
            "knowledge_cycles",
            "update",
            "BEFORE UPDATE OF source_snapshot_id, knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head",
            true,
        ),
    ] {
        let sql = source_view_envelope_trigger(table, operation, timing, required);
        execute_migration_sql(manager, &sql).await?;
    }
    for sql in [
        r#"CREATE TRIGGER IF NOT EXISTS trg_impact_view_insert BEFORE INSERT ON knowledge_impact_receipts
           WHEN ((NEW.baseline_source_snapshot_id IS NULL) != (NEW.baseline_knowledge_view_sha256 IS NULL))
             OR ((NEW.current_source_snapshot_id IS NULL) != (NEW.current_knowledge_view_sha256 IS NULL))
             OR ((NEW.current_source_snapshot_id IS NULL) != (NEW.source_authority_kind IS NULL))
             OR (NEW.current_source_snapshot_id IS NULL AND (NEW.input_overlay_sha256 IS NOT NULL OR NEW.source_ref_name IS NOT NULL OR NEW.source_raw_head IS NOT NULL))
             OR (NEW.baseline_source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.baseline_source_snapshot_id AND s.project_id = NEW.project_id))
             OR (NEW.current_source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.current_source_snapshot_id AND s.project_id = NEW.project_id))
           BEGIN SELECT RAISE(ABORT, 'invalid impact source-view binding'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_impact_view_update
           BEFORE UPDATE OF baseline_source_snapshot_id, baseline_knowledge_view_sha256, current_source_snapshot_id, current_knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head ON knowledge_impact_receipts
           WHEN ((NEW.baseline_source_snapshot_id IS NULL) != (NEW.baseline_knowledge_view_sha256 IS NULL))
             OR ((NEW.current_source_snapshot_id IS NULL) != (NEW.current_knowledge_view_sha256 IS NULL))
             OR ((NEW.current_source_snapshot_id IS NULL) != (NEW.source_authority_kind IS NULL))
             OR (NEW.current_source_snapshot_id IS NULL AND (NEW.input_overlay_sha256 IS NOT NULL OR NEW.source_ref_name IS NOT NULL OR NEW.source_raw_head IS NOT NULL))
             OR (NEW.baseline_source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.baseline_source_snapshot_id AND s.project_id = NEW.project_id))
             OR (NEW.current_source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.current_source_snapshot_id AND s.project_id = NEW.project_id))
           BEGIN SELECT RAISE(ABORT, 'invalid impact source-view binding'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_review_view_insert BEFORE INSERT ON knowledge_review_receipts
           WHEN ((NEW.review_source_snapshot_id IS NULL) != (NEW.review_knowledge_view_sha256 IS NULL))
             OR ((NEW.review_source_snapshot_id IS NULL) != (NEW.source_authority_kind IS NULL))
             OR (NEW.review_source_snapshot_id IS NULL AND (NEW.input_overlay_sha256 IS NOT NULL OR NEW.source_ref_name IS NOT NULL OR NEW.source_raw_head IS NOT NULL))
             OR (NEW.review_source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.review_source_snapshot_id AND s.project_id = NEW.project_id))
           BEGIN SELECT RAISE(ABORT, 'invalid review source-view binding'); END"#,
        r#"CREATE TRIGGER IF NOT EXISTS trg_review_view_update
           BEFORE UPDATE OF review_source_snapshot_id, review_knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head ON knowledge_review_receipts
           WHEN ((NEW.review_source_snapshot_id IS NULL) != (NEW.review_knowledge_view_sha256 IS NULL))
             OR ((NEW.review_source_snapshot_id IS NULL) != (NEW.source_authority_kind IS NULL))
             OR (NEW.review_source_snapshot_id IS NULL AND (NEW.input_overlay_sha256 IS NOT NULL OR NEW.source_ref_name IS NOT NULL OR NEW.source_raw_head IS NOT NULL))
             OR (NEW.review_source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.review_source_snapshot_id AND s.project_id = NEW.project_id))
           BEGIN SELECT RAISE(ABORT, 'invalid review source-view binding'); END"#,
    ] {
        execute_migration_sql(manager, sql).await?;
    }
    Ok(())
}

fn source_view_envelope_trigger(
    table: &str,
    operation: &str,
    timing: &str,
    require_binding: bool,
) -> String {
    let required = if require_binding {
        "NEW.source_snapshot_id IS NULL OR NEW.knowledge_view_sha256 IS NULL OR NEW.source_authority_kind IS NULL OR"
    } else {
        ""
    };
    format!(
        "CREATE TRIGGER IF NOT EXISTS trg_{table}_source_view_{operation} {timing} ON {table} \
         WHEN {required} ((NEW.source_snapshot_id IS NULL) != (NEW.knowledge_view_sha256 IS NULL)) \
           OR ((NEW.source_snapshot_id IS NULL) != (NEW.source_authority_kind IS NULL)) \
           OR (NEW.source_snapshot_id IS NULL AND (NEW.input_overlay_sha256 IS NOT NULL OR NEW.source_ref_name IS NOT NULL OR NEW.source_raw_head IS NOT NULL)) \
           OR (NEW.source_snapshot_id IS NOT NULL AND NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.source_snapshot_id AND s.project_id = NEW.project_id)) \
         BEGIN SELECT RAISE(ABORT, 'invalid source-view binding'); END"
    )
}

fn source_view_audit_columns() -> [ColumnDef; 6] {
    [
        ColumnDef::new(Columns::SourceSnapshotId)
            .text()
            .extra("REFERENCES knowledge_source_snapshots (id)")
            .to_owned(),
        ColumnDef::new(Columns::KnowledgeViewSha256)
            .text()
            .check(
                Expr::col(Columns::KnowledgeViewSha256)
                    .is_null()
                    .or(Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::KnowledgeViewSha256))]),
                    )
                    .eq(64)
                    .and(
                        Expr::col(Columns::KnowledgeViewSha256).eq(Expr::expr(Func::lower(
                            Expr::col(Columns::KnowledgeViewSha256),
                        ))),
                    )),
            )
            .to_owned(),
        ColumnDef::new(Columns::InputOverlaySha256)
            .text()
            .check(
                Expr::col(Columns::InputOverlaySha256)
                    .is_null()
                    .or(Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::InputOverlaySha256))]),
                    )
                    .eq(64)
                    .and(
                        Expr::col(Columns::InputOverlaySha256).eq(Expr::expr(Func::lower(
                            Expr::col(Columns::InputOverlaySha256),
                        ))),
                    )),
            )
            .to_owned(),
        source_authority_column(),
        ColumnDef::new(Columns::SourceRefName).text().to_owned(),
        ColumnDef::new(Columns::SourceRawHead).text().to_owned(),
    ]
}

fn source_authority_column() -> ColumnDef {
    ColumnDef::new(Columns::SourceAuthorityKind)
        .text()
        .check(
            Expr::col(Columns::SourceAuthorityKind)
                .is_null()
                .or(Expr::col(Columns::SourceAuthorityKind)
                    .is_in([Expr::val("canonical"), Expr::val("proposal_only")])),
        )
        .to_owned()
}

#[derive(DeriveIden)]
enum Tables {
    KnowledgeExternalArtifacts,
    KnowledgeSourceSnapshotArtifacts,
    KnowledgeSourceSnapshotArtifactsV45,
    KnowledgeSourceSnapshots,
    Projects,
}

#[derive(DeriveIden)]
enum Columns {
    ArtifactId,
    BaselineKnowledgeViewSha256,
    BaselineSourceSnapshotId,
    CreatedAt,
    CurrentKnowledgeViewSha256,
    CurrentSourceSnapshotId,
    Id,
    InputOverlaySha256,
    KnowledgeSourceLineageId,
    KnowledgeViewSha256,
    ProjectId,
    Purpose,
    ReviewKnowledgeViewSha256,
    ReviewSourceSnapshotId,
    SnapshotSha256,
    SourceAuthorityKind,
    SourceLineageId,
    SourceRawHead,
    SourceRefName,
    SourceScopeArtifactSha256,
    SourceSnapshotId,
    StoreUuid,
}
