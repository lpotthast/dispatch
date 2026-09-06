use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::DatabaseBackend;
use sea_orm_migration::sea_query::{Func, SimpleExpr, TableCreateStatement};

use super::super::execute_migration_sql;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260905_000048_add_knowledge_execution_fencing"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_knowledge_execution_fencing(manager).await
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // Execution identities, acquisitions, and outputs are durable audit evidence. Older
        // binaries ignore these additive tables; rollback deliberately retains every row.
        Ok(())
    }
}

fn knowledge_execution_fencing_tables() -> Vec<TableCreateStatement> {
    vec![
        Table::create()
            .table(Tables::KnowledgeExecutionProjects)
            .if_not_exists()
            .col(
                ColumnDef::new(Columns::ProjectId)
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(ColumnDef::new(Columns::RetiredAt).text().null())
            .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
            .col(ColumnDef::new(Columns::UpdatedAt).text().not_null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeExecutionProjects)
                    .from_col(Columns::ProjectId)
                    .to_tbl(Tables::Projects)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .to_owned(),
        Table::create()
            .table(Tables::KnowledgeCycleExecutionIdentities)
            .if_not_exists()
            .col(
                ColumnDef::new(Columns::CycleId)
                    .text()
                    .not_null()
                    .primary_key(),
            )
            .col(ColumnDef::new(Columns::ProjectId).big_integer().not_null())
            .col(ColumnDef::new(Columns::CreatedEventId).text().not_null())
            .col(
                ColumnDef::new(Columns::DeliverySchemaVersion)
                    .integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Columns::DeliveryJson).text().not_null())
            .col(ColumnDef::new(Columns::DeliverySha256).text().not_null())
            .col(
                ColumnDef::new(Columns::ComputationSchemaVersion)
                    .integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Columns::ComputationJson).text().not_null())
            .col(ColumnDef::new(Columns::ComputationSha256).text().not_null())
            .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeCycleExecutionIdentities)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::CycleId)
                    .to_tbl(Tables::KnowledgeCycles)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeCycleExecutionIdentities)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::CreatedEventId)
                    .to_tbl(Tables::KnowledgeCycleEvents)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(
                Expr::col(Columns::DeliverySchemaVersion)
                    .eq(1)
                    .and(Expr::col(Columns::ComputationSchemaVersion).eq(1)),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::CycleId))]),
                )
                .eq(36)
                .and(
                    Expr::col(Columns::CycleId)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::CycleId)))),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::CreatedEventId))]),
                )
                .eq(36)
                .and(
                    Expr::col(Columns::CreatedEventId)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::CreatedEventId)))),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::DeliveryJson))]),
                )
                .between(2, 16384)
                .and(
                    Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::ComputationJson))]),
                    )
                    .between(2, 16384),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::DeliverySha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::DeliverySha256)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::DeliverySha256)))),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::ComputationSha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::ComputationSha256).eq(Expr::expr(Func::lower(Expr::col(
                        Columns::ComputationSha256,
                    )))),
                ),
            )
            .index(
                Index::create()
                    .unique()
                    .col(Columns::ProjectId)
                    .col(Columns::CycleId),
            )
            .to_owned(),
        Table::create()
            .table(Tables::KnowledgeActiveBootstrapCycles)
            .if_not_exists()
            .col(
                ColumnDef::new(Columns::ProjectId)
                    .big_integer()
                    .not_null()
                    .primary_key(),
            )
            .col(
                ColumnDef::new(Columns::CycleId)
                    .text()
                    .not_null()
                    .unique_key(),
            )
            .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeActiveBootstrapCycles)
                    .from_col(Columns::ProjectId)
                    .to_tbl(Tables::Projects)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeActiveBootstrapCycles)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::CycleId)
                    .to_tbl(Tables::KnowledgeCycles)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::CycleId))]),
                )
                .eq(36)
                .and(
                    Expr::col(Columns::CycleId)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::CycleId)))),
                ),
            )
            .to_owned(),
        Table::create()
            .table(Tables::KnowledgeCycleShards)
            .if_not_exists()
            .col(ColumnDef::new(Columns::Id).text().not_null().primary_key())
            .col(ColumnDef::new(Columns::ProjectId).big_integer().not_null())
            .col(ColumnDef::new(Columns::CycleId).text().not_null())
            .col(
                ColumnDef::new(Columns::IdentitySchemaVersion)
                    .integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Columns::IdentityJson).text().not_null())
            .col(ColumnDef::new(Columns::IdentitySha256).text().not_null())
            .col(ColumnDef::new(Columns::ComputationSha256).text().not_null())
            .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeCycleShards)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::CycleId)
                    .to_tbl(Tables::KnowledgeCycles)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::Id))]),
                )
                .eq(36)
                .and(Expr::col(Columns::Id).eq(Expr::expr(Func::lower(Expr::col(Columns::Id))))),
            )
            .check(Expr::col(Columns::IdentitySchemaVersion).eq(1))
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::IdentityJson))]),
                )
                .between(2, 16384),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::IdentitySha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::IdentitySha256)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::IdentitySha256)))),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::ComputationSha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::ComputationSha256).eq(Expr::expr(Func::lower(Expr::col(
                        Columns::ComputationSha256,
                    )))),
                ),
            )
            .index(
                Index::create()
                    .unique()
                    .col(Columns::ProjectId)
                    .col(Columns::Id),
            )
            .to_owned(),
        Table::create()
            .table(Tables::KnowledgeLeaseSlots)
            .if_not_exists()
            .col(ColumnDef::new(Columns::Id).text().not_null().primary_key())
            .col(ColumnDef::new(Columns::ProjectId).big_integer().not_null())
            .col(ColumnDef::new(Columns::StoreUuid).text().not_null())
            .col(ColumnDef::new(Columns::SubjectKind).text().not_null())
            .col(ColumnDef::new(Columns::LogicalKey).text().not_null())
            .col(ColumnDef::new(Columns::CycleId).text().null())
            .col(ColumnDef::new(Columns::ShardId).text().null())
            .col(ColumnDef::new(Columns::CurrentAcquisitionId).text().null())
            .col(
                ColumnDef::new(Columns::CurrentFence)
                    .big_integer()
                    .not_null()
                    .default(0),
            )
            .col(
                ColumnDef::new(Columns::CurrentAttempt)
                    .big_integer()
                    .not_null()
                    .default(0),
            )
            .col(
                ColumnDef::new(Columns::Version)
                    .big_integer()
                    .not_null()
                    .default(1),
            )
            .col(ColumnDef::new(Columns::RetiredAt).text().null())
            .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
            .col(ColumnDef::new(Columns::UpdatedAt).text().not_null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeLeaseSlots)
                    .from_col(Columns::ProjectId)
                    .to_tbl(Tables::Projects)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeLeaseSlots)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::CycleId)
                    .to_tbl(Tables::KnowledgeCycles)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeLeaseSlots)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::ShardId)
                    .to_tbl(Tables::KnowledgeCycleShards)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::Id))]),
                )
                .eq(36)
                .and(Expr::col(Columns::Id).eq(Expr::expr(Func::lower(Expr::col(Columns::Id))))),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::StoreUuid))]),
                )
                .eq(36)
                .and(
                    Expr::col(Columns::StoreUuid)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::StoreUuid)))),
                ),
            )
            .check(Expr::col(Columns::SubjectKind).is_in([
                Expr::val("planner"),
                Expr::val("coordinator"),
                Expr::val("reducer"),
                Expr::val("shard"),
                Expr::val("store_writer"),
            ]))
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::LogicalKey))]),
                )
                .between(1, 512),
            )
            .check(
                Expr::col(Columns::CurrentFence)
                    .gte(0)
                    .and(Expr::col(Columns::CurrentAttempt).gte(0))
                    .and(Expr::col(Columns::Version).gt(0)),
            )
            .check(
                Expr::col(Columns::SubjectKind)
                    .is_in([Expr::val("planner"), Expr::val("store_writer")])
                    .and(Expr::col(Columns::CycleId).is_null())
                    .and(Expr::col(Columns::ShardId).is_null())
                    .or(Expr::col(Columns::SubjectKind)
                        .is_in([Expr::val("coordinator"), Expr::val("reducer")])
                        .and(Expr::expr(Expr::col(Columns::CycleId).is_null()).not())
                        .and(Expr::col(Columns::ShardId).is_null()))
                    .or(Expr::col(Columns::SubjectKind)
                        .eq("shard")
                        .and(Expr::expr(Expr::col(Columns::CycleId).is_null()).not())
                        .and(Expr::expr(Expr::col(Columns::ShardId).is_null()).not())),
            )
            .index(
                Index::create()
                    .unique()
                    .col(Columns::ProjectId)
                    .col(Columns::Id),
            )
            .to_owned(),
        Table::create()
            .table(Tables::KnowledgeLeaseAcquisitions)
            .if_not_exists()
            .col(ColumnDef::new(Columns::Id).text().not_null().primary_key())
            .col(ColumnDef::new(Columns::ProjectId).big_integer().not_null())
            .col(ColumnDef::new(Columns::SlotId).text().not_null())
            .col(ColumnDef::new(Columns::OwnerId).text().not_null())
            .col(ColumnDef::new(Columns::LeaseToken).text().not_null())
            .col(ColumnDef::new(Columns::Fence).big_integer().not_null())
            .col(ColumnDef::new(Columns::Attempt).big_integer().not_null())
            .col(ColumnDef::new(Columns::State).text().not_null())
            .col(ColumnDef::new(Columns::AcquiredAt).text().not_null())
            .col(ColumnDef::new(Columns::HeartbeatAt).text().not_null())
            .col(ColumnDef::new(Columns::ExpiresAt).text().not_null())
            .col(ColumnDef::new(Columns::ReleasedAt).text().null())
            .col(
                ColumnDef::new(Columns::PreparedSchemaVersion)
                    .integer()
                    .null(),
            )
            .col(ColumnDef::new(Columns::PreparedJson).text().null())
            .col(ColumnDef::new(Columns::PreparedSha256).text().null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeLeaseAcquisitions)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::SlotId)
                    .to_tbl(Tables::KnowledgeLeaseSlots)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::Id))]),
                )
                .eq(36)
                .and(Expr::col(Columns::Id).eq(Expr::expr(Func::lower(Expr::col(Columns::Id))))),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::OwnerId))]),
                )
                .between(1, 512),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::LeaseToken))]),
                )
                .eq(36)
                .and(
                    Expr::col(Columns::LeaseToken)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::LeaseToken)))),
                ),
            )
            .check(
                Expr::col(Columns::Fence)
                    .gt(0)
                    .and(Expr::col(Columns::Attempt).gt(0)),
            )
            .check(Expr::col(Columns::State).is_in([
                Expr::val("active"),
                Expr::val("released"),
                Expr::val("expired"),
                Expr::val("retired"),
            ]))
            .check(
                Expr::col(Columns::State)
                    .eq("active")
                    .and(Expr::col(Columns::ReleasedAt).is_null())
                    .or(Expr::col(Columns::State)
                        .ne("active")
                        .and(Expr::expr(Expr::col(Columns::ReleasedAt).is_null()).not())),
            )
            .check(
                Expr::col(Columns::PreparedSchemaVersion)
                    .is_null()
                    .and(Expr::col(Columns::PreparedJson).is_null())
                    .and(Expr::col(Columns::PreparedSha256).is_null())
                    .or(Expr::col(Columns::PreparedSchemaVersion)
                        .eq(1)
                        .and(
                            Expr::expr(
                                Func::cust(Alias::new("length"))
                                    .args([SimpleExpr::from(Expr::col(Columns::PreparedJson))]),
                            )
                            .between(2, 16384),
                        )
                        .and(
                            Expr::expr(
                                Func::cust(Alias::new("length"))
                                    .args([SimpleExpr::from(Expr::col(Columns::PreparedSha256))]),
                            )
                            .eq(64),
                        )
                        .and(
                            Expr::col(Columns::PreparedSha256)
                                .eq(Expr::expr(Func::lower(Expr::col(Columns::PreparedSha256)))),
                        )),
            )
            .index(
                Index::create()
                    .unique()
                    .col(Columns::ProjectId)
                    .col(Columns::Id),
            )
            .to_owned(),
        Table::create()
            .table(Tables::KnowledgeComputationOutputs)
            .if_not_exists()
            .col(ColumnDef::new(Columns::Id).text().not_null().primary_key())
            .col(ColumnDef::new(Columns::ProjectId).big_integer().not_null())
            .col(ColumnDef::new(Columns::CycleId).text().not_null())
            .col(ColumnDef::new(Columns::ShardId).text().not_null())
            .col(
                ColumnDef::new(Columns::ReusableSchemaVersion)
                    .integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Columns::ReusableJson).text().not_null())
            .col(ColumnDef::new(Columns::ReusableSha256).text().not_null())
            .col(
                ColumnDef::new(Columns::AttemptSchemaVersion)
                    .integer()
                    .not_null(),
            )
            .col(ColumnDef::new(Columns::AttemptJson).text().not_null())
            .col(ColumnDef::new(Columns::AttemptSha256).text().not_null())
            .col(ColumnDef::new(Columns::OutputSha256).text().not_null())
            .col(ColumnDef::new(Columns::AcquisitionId).text().not_null())
            .col(ColumnDef::new(Columns::Fence).big_integer().not_null())
            .col(ColumnDef::new(Columns::CreatedAt).text().not_null())
            .col(ColumnDef::new(Columns::VerifiedAt).text().not_null())
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeComputationOutputs)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::CycleId)
                    .to_tbl(Tables::KnowledgeCycles)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeComputationOutputs)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::ShardId)
                    .to_tbl(Tables::KnowledgeCycleShards)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .foreign_key(
                ForeignKey::create()
                    .from_tbl(Tables::KnowledgeComputationOutputs)
                    .from_col(Columns::ProjectId)
                    .from_col(Columns::AcquisitionId)
                    .to_tbl(Tables::KnowledgeLeaseAcquisitions)
                    .to_col(Columns::ProjectId)
                    .to_col(Columns::Id)
                    .on_delete(ForeignKeyAction::Cascade),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::Id))]),
                )
                .eq(36)
                .and(Expr::col(Columns::Id).eq(Expr::expr(Func::lower(Expr::col(Columns::Id))))),
            )
            .check(
                Expr::col(Columns::ReusableSchemaVersion).eq(1).and(
                    Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::ReusableJson))]),
                    )
                    .between(2, 16384),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::ReusableSha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::ReusableSha256)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::ReusableSha256)))),
                ),
            )
            .check(
                Expr::col(Columns::AttemptSchemaVersion).eq(1).and(
                    Expr::expr(
                        Func::cust(Alias::new("length"))
                            .args([SimpleExpr::from(Expr::col(Columns::AttemptJson))]),
                    )
                    .between(2, 16384),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::AttemptSha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::AttemptSha256)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::AttemptSha256)))),
                ),
            )
            .check(
                Expr::expr(
                    Func::cust(Alias::new("length"))
                        .args([SimpleExpr::from(Expr::col(Columns::OutputSha256))]),
                )
                .eq(64)
                .and(
                    Expr::col(Columns::OutputSha256)
                        .eq(Expr::expr(Func::lower(Expr::col(Columns::OutputSha256)))),
                ),
            )
            .check(Expr::col(Columns::Fence).gt(0))
            .to_owned(),
    ]
}

async fn add_knowledge_execution_fencing(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for table in knowledge_execution_fencing_tables() {
        manager.create_table(table).await?;
    }

    for index in [
        Index::create()
            .name("uq_cycle_execution_project_cycle")
            .table(Tables::KnowledgeCycleExecutionIdentities)
            .col(Columns::ProjectId)
            .col(Columns::CycleId)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_cycle_execution_delivery")
            .table(Tables::KnowledgeCycleExecutionIdentities)
            .col(Columns::ProjectId)
            .col(Columns::DeliverySha256)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("idx_cycle_execution_computation")
            .table(Tables::KnowledgeCycleExecutionIdentities)
            .col(Columns::ProjectId)
            .col(Columns::ComputationSha256)
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_cycle_shards_project_id")
            .table(Tables::KnowledgeCycleShards)
            .col(Columns::ProjectId)
            .col(Columns::Id)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_cycle_shards_stable_identity")
            .table(Tables::KnowledgeCycleShards)
            .col(Columns::ProjectId)
            .col(Columns::CycleId)
            .col(Columns::IdentitySha256)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_lease_slots_project_id")
            .table(Tables::KnowledgeLeaseSlots)
            .col(Columns::ProjectId)
            .col(Columns::Id)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_lease_slots_logical_key")
            .table(Tables::KnowledgeLeaseSlots)
            .col(Columns::ProjectId)
            .col(Columns::LogicalKey)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_lease_acquisitions_project_id")
            .table(Tables::KnowledgeLeaseAcquisitions)
            .col(Columns::ProjectId)
            .col(Columns::Id)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_lease_acquisitions_token")
            .table(Tables::KnowledgeLeaseAcquisitions)
            .col(Columns::ProjectId)
            .col(Columns::LeaseToken)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_lease_acquisitions_fence")
            .table(Tables::KnowledgeLeaseAcquisitions)
            .col(Columns::SlotId)
            .col(Columns::Fence)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_lease_acquisitions_attempt")
            .table(Tables::KnowledgeLeaseAcquisitions)
            .col(Columns::SlotId)
            .col(Columns::Attempt)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_computation_outputs_project_id")
            .table(Tables::KnowledgeComputationOutputs)
            .col(Columns::ProjectId)
            .col(Columns::Id)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("uq_computation_outputs_identity")
            .table(Tables::KnowledgeComputationOutputs)
            .col(Columns::ProjectId)
            .col(Columns::ReusableSha256)
            .unique()
            .if_not_exists()
            .to_owned(),
        Index::create()
            .name("idx_computation_outputs_attempt")
            .table(Tables::KnowledgeComputationOutputs)
            .col(Columns::ProjectId)
            .col(Columns::AttemptSha256)
            .if_not_exists()
            .to_owned(),
    ] {
        manager.create_index(index).await?;
    }

    if manager.get_database_backend() == DatabaseBackend::Sqlite {
        for sql in [
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_execution_identity_insert
               BEFORE INSERT ON knowledge_cycle_execution_identities
               WHEN NOT EXISTS (
                 SELECT 1 FROM knowledge_cycles c
                 JOIN knowledge_cycle_events e ON e.project_id = c.project_id AND e.cycle_id = c.id
                 WHERE c.project_id = NEW.project_id AND c.id = NEW.cycle_id
                   AND c.request_key = NEW.delivery_sha256 AND c.analysis_sha256 = NEW.computation_sha256
                   AND e.id = NEW.created_event_id AND e.sequence = 1 AND e.kind = 'created'
               )
               BEGIN SELECT RAISE(ABORT, 'execution identity disagrees with cycle or creation event'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_execution_identity_update
               BEFORE UPDATE ON knowledge_cycle_execution_identities
               BEGIN SELECT RAISE(ABORT, 'cycle execution identities are immutable'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_active_bootstrap_insert
               BEFORE INSERT ON knowledge_active_bootstrap_cycles
               WHEN NOT EXISTS (
                 SELECT 1 FROM knowledge_cycles c
                 JOIN knowledge_cycle_execution_identities i ON i.project_id = c.project_id AND i.cycle_id = c.id
                 WHERE c.project_id = NEW.project_id AND c.id = NEW.cycle_id AND c.kind = 'bootstrap'
                   AND c.status IN ('queued','running','waiting')
               )
               BEGIN SELECT RAISE(ABORT, 'active bootstrap slot requires an identified nonterminal bootstrap cycle'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_active_bootstrap_update
               BEFORE UPDATE ON knowledge_active_bootstrap_cycles
               BEGIN SELECT RAISE(ABORT, 'active bootstrap slots are immutable'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_release_bootstrap_after_terminal
               AFTER UPDATE OF status ON knowledge_cycles
               WHEN NEW.status IN ('completed','failed','cancelled')
               BEGIN DELETE FROM knowledge_active_bootstrap_cycles WHERE project_id = NEW.project_id AND cycle_id = NEW.id; END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_shard_insert
               BEFORE INSERT ON knowledge_cycle_shards
               WHEN NOT EXISTS (
                 SELECT 1 FROM knowledge_cycle_execution_identities i
                 WHERE i.project_id = NEW.project_id AND i.cycle_id = NEW.cycle_id
                   AND i.computation_sha256 = NEW.computation_sha256
               )
               BEGIN SELECT RAISE(ABORT, 'shard computation identity disagrees with cycle'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_cycle_shard_update
               BEFORE UPDATE ON knowledge_cycle_shards
               BEGIN SELECT RAISE(ABORT, 'stable cycle shards are immutable'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_lease_slot_insert
               BEFORE INSERT ON knowledge_lease_slots
               WHEN NEW.logical_key != CASE NEW.subject_kind
                   WHEN 'planner' THEN 'planner'
                   WHEN 'store_writer' THEN 'store_writer:' || NEW.store_uuid
                   WHEN 'coordinator' THEN 'coordinator:' || NEW.cycle_id
                   WHEN 'reducer' THEN 'reducer:' || NEW.cycle_id
                   WHEN 'shard' THEN 'shard:' || NEW.shard_id END
                 OR (NEW.cycle_id IS NOT NULL AND NOT EXISTS (
                   SELECT 1 FROM knowledge_cycles c JOIN knowledge_cycle_execution_identities i
                     ON i.project_id = c.project_id AND i.cycle_id = c.id
                   WHERE c.project_id = NEW.project_id AND c.id = NEW.cycle_id AND c.store_uuid = NEW.store_uuid
                 ))
                 OR (NEW.shard_id IS NOT NULL AND NOT EXISTS (
                   SELECT 1 FROM knowledge_cycle_shards s
                   WHERE s.project_id = NEW.project_id AND s.id = NEW.shard_id AND s.cycle_id = NEW.cycle_id
                 ))
               BEGIN SELECT RAISE(ABORT, 'invalid knowledge lease slot binding'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_lease_acquisition_insert
               BEFORE INSERT ON knowledge_lease_acquisitions
               WHEN NOT EXISTS (
                 SELECT 1 FROM knowledge_lease_slots s WHERE s.project_id = NEW.project_id AND s.id = NEW.slot_id
                   AND s.retired_at IS NULL AND s.current_acquisition_id = NEW.id
                   AND NEW.fence = s.current_fence AND NEW.attempt = s.current_attempt
                   AND ((s.subject_kind = 'store_writer' AND NEW.prepared_schema_version = 1)
                     OR (s.subject_kind != 'store_writer' AND NEW.prepared_schema_version IS NULL))
               )
               BEGIN SELECT RAISE(ABORT, 'invalid knowledge lease acquisition'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_lease_acquisition_immutable
               BEFORE UPDATE OF project_id, slot_id, owner_id, lease_token, fence, attempt, acquired_at, prepared_schema_version, prepared_json, prepared_sha256 ON knowledge_lease_acquisitions
               BEGIN SELECT RAISE(ABORT, 'knowledge lease acquisition identity is immutable'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_computation_output_insert
               BEFORE INSERT ON knowledge_computation_outputs
               WHEN NOT EXISTS (
                 SELECT 1 FROM knowledge_lease_acquisitions a
                 JOIN knowledge_lease_slots l ON l.project_id = a.project_id AND l.id = a.slot_id
                 JOIN knowledge_cycle_shards s ON s.project_id = l.project_id AND s.id = l.shard_id
                 WHERE a.project_id = NEW.project_id AND a.id = NEW.acquisition_id
                   AND a.state = 'active' AND a.fence = NEW.fence
                   AND l.current_acquisition_id = a.id AND l.current_fence = a.fence
                   AND l.subject_kind = 'shard' AND s.id = NEW.shard_id AND s.cycle_id = NEW.cycle_id
               )
               BEGIN SELECT RAISE(ABORT, 'computation output requires the current active shard fence'); END"#,
            r#"CREATE TRIGGER IF NOT EXISTS trg_computation_output_update
               BEFORE UPDATE ON knowledge_computation_outputs
               BEGIN SELECT RAISE(ABORT, 'computation outputs are immutable'); END"#,
        ] {
            execute_migration_sql(manager, sql).await?;
        }
    }
    Ok(())
}

#[derive(DeriveIden)]
enum Tables {
    KnowledgeActiveBootstrapCycles,
    KnowledgeComputationOutputs,
    KnowledgeCycleEvents,
    KnowledgeCycleExecutionIdentities,
    KnowledgeCycleShards,
    KnowledgeCycles,
    KnowledgeExecutionProjects,
    KnowledgeLeaseAcquisitions,
    KnowledgeLeaseSlots,
    Projects,
}

#[derive(DeriveIden)]
enum Columns {
    AcquiredAt,
    AcquisitionId,
    Attempt,
    AttemptJson,
    AttemptSchemaVersion,
    AttemptSha256,
    ComputationJson,
    ComputationSchemaVersion,
    ComputationSha256,
    CreatedAt,
    CreatedEventId,
    CurrentAcquisitionId,
    CurrentAttempt,
    CurrentFence,
    CycleId,
    DeliveryJson,
    DeliverySchemaVersion,
    DeliverySha256,
    ExpiresAt,
    Fence,
    HeartbeatAt,
    Id,
    IdentityJson,
    IdentitySchemaVersion,
    IdentitySha256,
    LeaseToken,
    LogicalKey,
    OutputSha256,
    OwnerId,
    PreparedJson,
    PreparedSchemaVersion,
    PreparedSha256,
    ProjectId,
    ReleasedAt,
    RetiredAt,
    ReusableJson,
    ReusableSchemaVersion,
    ReusableSha256,
    ShardId,
    SlotId,
    State,
    StoreUuid,
    SubjectKind,
    UpdatedAt,
    VerifiedAt,
    Version,
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;

    fn normalize(sql: &str) -> String {
        sql.chars()
            .filter(|c| !c.is_whitespace() && *c != '"')
            .flat_map(char::to_uppercase)
            .collect()
    }

    #[test]
    fn execution_fencing_table_order_and_sql_are_postgres_portable() {
        let sql = knowledge_execution_fencing_tables()
            .iter()
            .map(|table| normalize(&table.to_string(PostgresQueryBuilder)))
            .collect::<Vec<_>>();
        assert_that!(&sql.len()).is_equal_to(7);
        for (index, table) in [
            "knowledge_execution_projects",
            "knowledge_cycle_execution_identities",
            "knowledge_active_bootstrap_cycles",
            "knowledge_cycle_shards",
            "knowledge_lease_slots",
            "knowledge_lease_acquisitions",
            "knowledge_computation_outputs",
        ]
        .into_iter()
        .enumerate()
        {
            assert_that!(&sql[index])
                .contains(normalize(&format!("CREATE TABLE IF NOT EXISTS {table}")));
            assert_that!(&sql[index]).does_not_contain(normalize("CREATE TRIGGER"));
            assert_that!(&sql[index]).does_not_contain(normalize("AUTOINCREMENT"));
        }
        assert_that!(&sql[3]).contains(normalize("UNIQUE (project_id, id)"));
        assert_that!(&sql[4]).contains(normalize("FOREIGN KEY (project_id, shard_id)"));
        assert_that!(&sql[4]).contains(normalize("UNIQUE (project_id, id)"));
        assert_that!(&sql[5]).contains(normalize("FOREIGN KEY (project_id, slot_id)"));
        assert_that!(&sql[5]).contains(normalize("UNIQUE (project_id, id)"));
        assert_that!(&sql[6]).contains(normalize("FOREIGN KEY (project_id, acquisition_id)"));
    }
}
