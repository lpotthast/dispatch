use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseBackend;
use sea_orm_migration::sea_query::{Func, SimpleExpr};

use super::super::execute_migration_sql;
use super::add_column_if_missing;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260905_000047_add_knowledge_cycle_lifecycle"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_knowledge_cycle_lifecycle(manager).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Cycle events are durable audit history. Older binaries ignore the additive canonical
        // envelope; rollback deliberately retains both its bytes and integrity triggers.
        Ok(())
    }
}

async fn add_knowledge_cycle_lifecycle(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for column in [
        ColumnDef::new(Columns::EventSchemaVersion)
            .integer()
            .check(
                Expr::col(Columns::EventSchemaVersion)
                    .is_null()
                    .or(Expr::col(Columns::EventSchemaVersion).gt(0)),
            )
            .to_owned(),
        ColumnDef::new(Columns::EventJson).text().to_owned(),
        ColumnDef::new(Columns::EventSha256)
            .text()
            .check(
                Expr::col(Columns::EventSha256).is_null().or(Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::EventSha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::EventSha256)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::EventSha256)))),
                )),
            )
            .to_owned(),
    ] {
        add_column_if_missing(manager, "knowledge_cycle_events", column).await?;
    }
    for column in [
        ColumnDef::new(Columns::UninitializedKnowledgeSha256)
            .text()
            .check(
                Expr::col(Columns::UninitializedKnowledgeSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::UninitializedKnowledgeSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::UninitializedKnowledgeSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::UninitializedKnowledgeSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
        ColumnDef::new(Columns::AcceptedDecisionSha256)
            .text()
            .check(
                Expr::col(Columns::AcceptedDecisionSha256)
                    .is_null()
                    .or(Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::AcceptedDecisionSha256))]),
                    )
                    .eq(64)
                    .and(
                        Expr::col(Columns::AcceptedDecisionSha256).eq(Expr::expr(Func::lower(
                            Expr::col(Columns::AcceptedDecisionSha256),
                        ))),
                    )),
            )
            .to_owned(),
        ColumnDef::new(Columns::ApplicationReceiptSha256)
            .text()
            .check(
                Expr::col(Columns::ApplicationReceiptSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::ApplicationReceiptSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::ApplicationReceiptSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::ApplicationReceiptSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
        ColumnDef::new(Columns::FinalizationReceiptSha256)
            .text()
            .check(
                Expr::col(Columns::FinalizationReceiptSha256)
                    .is_null()
                    .or(
                        Expr::expr(Func::cust(Alias::new("length")).args([SimpleExpr::from(
                            Expr::col(Columns::FinalizationReceiptSha256),
                        )]))
                        .eq(64)
                        .and(
                            Expr::col(Columns::FinalizationReceiptSha256).eq(Expr::expr(
                                Func::lower(Expr::col(Columns::FinalizationReceiptSha256)),
                            )),
                        ),
                    ),
            )
            .to_owned(),
        ColumnDef::new(Columns::VerificationSha256)
            .text()
            .check(
                Expr::col(Columns::VerificationSha256)
                    .is_null()
                    .or(Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::VerificationSha256))]),
                    )
                    .eq(64)
                    .and(
                        Expr::col(Columns::VerificationSha256).eq(Expr::expr(Func::lower(
                            Expr::col(Columns::VerificationSha256),
                        ))),
                    )),
            )
            .to_owned(),
    ] {
        add_column_if_missing(manager, "knowledge_cycles", column).await?;
    }

    if manager.get_database_backend() == DatabaseBackend::Sqlite {
        for name in [
            "trg_knowledge_cycles_source_view_insert",
            "trg_knowledge_cycles_source_view_update",
            "trg_cycles_source_view_immutable",
            "trg_cycle_state_insert",
            "trg_cycle_state_update",
        ] {
            execute_migration_sql(manager, &format!("DROP TRIGGER IF EXISTS {name}")).await?;
        }
        for sql in [
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_event_canonical_insert
               BEFORE INSERT ON knowledge_cycle_events
               WHEN NEW.event_schema_version IS NOT 1
                 OR NEW.event_json IS NULL OR length(NEW.event_json) = 0
                 OR NEW.event_sha256 IS NULL OR length(NEW.event_sha256) != 64 OR NEW.event_sha256 != lower(NEW.event_sha256)
                 OR NEW.input_sha256 IS NULL OR length(NEW.input_sha256) != 64 OR NEW.input_sha256 != lower(NEW.input_sha256)
                 OR NEW.output_sha256 IS NULL OR length(NEW.output_sha256) != 64 OR NEW.output_sha256 != lower(NEW.output_sha256)
                 OR NEW.sequence != NEW.resulting_version
                 OR NEW.sequence != COALESCE((SELECT MAX(e.sequence) + 1 FROM knowledge_cycle_events e WHERE e.cycle_id = NEW.cycle_id), 1)
                 OR NOT EXISTS (SELECT 1 FROM knowledge_cycles c WHERE c.project_id = NEW.project_id AND c.id = NEW.cycle_id AND c.version = NEW.resulting_version)
               BEGIN SELECT RAISE(ABORT, 'invalid canonical knowledge cycle event'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_event_append_only_update
               BEFORE UPDATE ON knowledge_cycle_events
               BEGIN SELECT RAISE(ABORT, 'knowledge cycle events are append-only'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_event_append_only_delete
               BEFORE DELETE ON knowledge_cycle_events
               WHEN EXISTS (SELECT 1 FROM knowledge_cycles c WHERE c.project_id = OLD.project_id AND c.id = OLD.cycle_id)
               BEGIN SELECT RAISE(ABORT, 'knowledge cycle events are append-only'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_identity_insert
               BEFORE INSERT ON knowledge_cycles
               WHEN NEW.source_authority_kind IS NULL OR NOT (
                 (NEW.kind = 'bootstrap' AND NEW.knowledge_revision IS NULL AND NEW.uninitialized_knowledge_sha256 IS NOT NULL AND NEW.knowledge_view_sha256 IS NULL)
                 OR (NEW.kind != 'bootstrap' AND NEW.knowledge_revision IS NOT NULL AND NEW.uninitialized_knowledge_sha256 IS NULL AND NEW.knowledge_view_sha256 IS NOT NULL)
               )
               BEGIN SELECT RAISE(ABORT, 'invalid cycle knowledge identity envelope'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_identity_update
               BEFORE UPDATE OF kind, knowledge_revision, uninitialized_knowledge_sha256, source_snapshot_id, knowledge_view_sha256, input_overlay_sha256, source_authority_kind, source_ref_name, source_raw_head, ignore_snapshot_id ON knowledge_cycles
               WHEN NEW.kind IS NOT OLD.kind
                 OR NEW.knowledge_revision IS NOT OLD.knowledge_revision
                 OR NEW.uninitialized_knowledge_sha256 IS NOT OLD.uninitialized_knowledge_sha256
                 OR NEW.source_snapshot_id IS NOT OLD.source_snapshot_id
                 OR NEW.knowledge_view_sha256 IS NOT OLD.knowledge_view_sha256
                 OR NEW.input_overlay_sha256 IS NOT OLD.input_overlay_sha256
                 OR NEW.source_authority_kind IS NOT OLD.source_authority_kind
                 OR NEW.source_ref_name IS NOT OLD.source_ref_name
                 OR NEW.source_raw_head IS NOT OLD.source_raw_head
                 OR NEW.ignore_snapshot_id IS NOT OLD.ignore_snapshot_id
               BEGIN SELECT RAISE(ABORT, 'cycle knowledge/source identity is immutable'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_source_binding_insert
               BEFORE INSERT ON knowledge_cycles
               WHEN NEW.source_snapshot_id IS NULL OR NEW.source_authority_kind IS NULL
                 OR NOT EXISTS (SELECT 1 FROM knowledge_source_snapshots s WHERE s.id = NEW.source_snapshot_id AND s.project_id = NEW.project_id AND s.store_uuid = NEW.store_uuid AND s.ignore_snapshot_id = NEW.ignore_snapshot_id)
               BEGIN SELECT RAISE(ABORT, 'invalid cycle source binding'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_checkpoint_insert
               BEFORE INSERT ON knowledge_cycles
               WHEN (NEW.application_receipt_sha256 IS NOT NULL AND NEW.accepted_decision_sha256 IS NULL)
                 OR (NEW.finalization_receipt_sha256 IS NOT NULL AND NEW.application_receipt_sha256 IS NULL)
                 OR (NEW.verification_sha256 IS NOT NULL AND NEW.finalization_receipt_sha256 IS NULL)
                 OR ((NEW.verification_sha256 IS NOT NULL) != (NEW.status = 'completed' AND NEW.terminal_outcome = 'applied'))
                 OR (NEW.application_receipt_sha256 IS NOT NULL AND NEW.terminal_outcome IN ('no_change','rejected','blocked','superseded','failed','cancelled'))
               BEGIN SELECT RAISE(ABORT, 'invalid cycle application checkpoint continuity'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_checkpoint_update
               BEFORE UPDATE OF accepted_decision_sha256, application_receipt_sha256, finalization_receipt_sha256, verification_sha256, status, terminal_outcome ON knowledge_cycles
               WHEN (NEW.application_receipt_sha256 IS NOT NULL AND NEW.accepted_decision_sha256 IS NULL)
                 OR (NEW.finalization_receipt_sha256 IS NOT NULL AND NEW.application_receipt_sha256 IS NULL)
                 OR (NEW.verification_sha256 IS NOT NULL AND NEW.finalization_receipt_sha256 IS NULL)
                 OR (OLD.accepted_decision_sha256 IS NOT NULL AND NEW.accepted_decision_sha256 IS NOT OLD.accepted_decision_sha256)
                 OR (OLD.application_receipt_sha256 IS NOT NULL AND NEW.application_receipt_sha256 IS NOT OLD.application_receipt_sha256)
                 OR (OLD.finalization_receipt_sha256 IS NOT NULL AND NEW.finalization_receipt_sha256 IS NOT OLD.finalization_receipt_sha256)
                 OR (OLD.verification_sha256 IS NOT NULL AND NEW.verification_sha256 IS NOT OLD.verification_sha256)
                 OR ((NEW.verification_sha256 IS NOT NULL) != (NEW.status = 'completed' AND NEW.terminal_outcome = 'applied'))
                 OR (NEW.application_receipt_sha256 IS NOT NULL AND NEW.terminal_outcome IN ('no_change','rejected','blocked','superseded','failed','cancelled'))
               BEGIN SELECT RAISE(ABORT, 'invalid cycle application checkpoint continuity'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_state_insert
               BEFORE INSERT ON knowledge_cycles
               WHEN NOT (
                 (NEW.status = 'queued' AND NEW.phase IS NULL AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome IS NULL AND NEW.started_at IS NULL AND NEW.finished_at IS NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'running' AND NEW.phase IS NOT NULL AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome IS NULL AND NEW.started_at IS NOT NULL AND NEW.finished_at IS NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'waiting' AND NEW.phase IS NOT NULL AND NEW.waiting_reason IS NOT NULL AND NEW.terminal_outcome IS NULL AND NEW.started_at IS NOT NULL AND NEW.finished_at IS NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'completed' AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome IN ('no_change', 'applied', 'rejected', 'blocked', 'superseded') AND NEW.finished_at IS NOT NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'failed' AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome = 'failed' AND NEW.finished_at IS NOT NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NOT NULL AND NEW.error_message IS NOT NULL)
                 OR (NEW.status = 'cancelled' AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome = 'cancelled' AND NEW.finished_at IS NOT NULL AND NEW.cancellation_actor_kind IS NOT NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
               )
               BEGIN SELECT RAISE(ABORT, 'invalid knowledge cycle lifecycle state'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_state_update
               BEFORE UPDATE OF status, phase, waiting_reason, terminal_outcome, cancellation_actor_kind, cancellation_actor_id, cancellation_reason, cancelled_at, error_code, error_message, started_at, finished_at ON knowledge_cycles
               WHEN NOT (
                 (NEW.status = 'queued' AND NEW.phase IS NULL AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome IS NULL AND NEW.started_at IS NULL AND NEW.finished_at IS NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'running' AND NEW.phase IS NOT NULL AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome IS NULL AND NEW.started_at IS NOT NULL AND NEW.finished_at IS NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'waiting' AND NEW.phase IS NOT NULL AND NEW.waiting_reason IS NOT NULL AND NEW.terminal_outcome IS NULL AND NEW.started_at IS NOT NULL AND NEW.finished_at IS NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'completed' AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome IN ('no_change', 'applied', 'rejected', 'blocked', 'superseded') AND NEW.finished_at IS NOT NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
                 OR (NEW.status = 'failed' AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome = 'failed' AND NEW.finished_at IS NOT NULL AND NEW.cancellation_actor_kind IS NULL AND NEW.error_code IS NOT NULL AND NEW.error_message IS NOT NULL)
                 OR (NEW.status = 'cancelled' AND NEW.waiting_reason IS NULL AND NEW.terminal_outcome = 'cancelled' AND NEW.finished_at IS NOT NULL AND NEW.cancellation_actor_kind IS NOT NULL AND NEW.error_code IS NULL AND NEW.error_message IS NULL)
               )
               BEGIN SELECT RAISE(ABORT, 'invalid knowledge cycle lifecycle state'); END"#,
        ] {
            execute_migration_sql(manager, sql).await?;
        }
    }
    Ok(())
}

#[derive(DeriveIden)]
enum Columns {
    AcceptedDecisionSha256,
    ApplicationReceiptSha256,
    EventJson,
    EventSchemaVersion,
    EventSha256,
    FinalizationReceiptSha256,
    UninitializedKnowledgeSha256,
    VerificationSha256,
}
