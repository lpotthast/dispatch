use sea_orm_migration::prelude::*;

use super::{create_simple_read_view, drop_view};

#[derive(DeriveIden)]
enum AgentRuns {
    Table,
    Id,
    ProjectId,
    WorkItemId,
    KnowledgeJobId,
    RunKind,
    Purpose,
    KnowledgeRevision,
    SourceBaselineId,
    SourceSnapshotId,
    KnowledgeViewSha256,
    InputOverlaySha256,
    SourceAuthorityKind,
    SourceRefName,
    SourceRawHead,
    MemoryEventId,
    TriggerId,
    TriggerName,
    TriggerRevisionId,
    PersonalityRevisionId,
    SystemPromptEventId,
    ToolName,
    Mutability,
    Status,
    Command,
    WorkingDir,
    WorktreePath,
    BranchName,
    ProcessId,
    ExitCode,
    LogPath,
    DeveloperInstructionsPath,
    UserPromptPath,
    AgentModel,
    AgentReasoningEffort,
    EffectiveInputSha256,
    EffectiveTimeoutSeconds,
    EffectiveConcurrencyGroup,
    InputTokens,
    CachedInputTokens,
    OutputTokens,
    CommitRequired,
    CommitOutcome,
    CommitShas,
    PrRequested,
    PrUrl,
    CleanupStatus,
    WorktreeCleanedAt,
    ResultSummary,
    SemanticPostconditionStatus,
    SemanticPostconditionFailures,
    StartedAt,
    FinishedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AgentRuns::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AgentRuns::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AgentRuns::WorkItemId).big_integer().null())
                    .col(
                        ColumnDef::new(AgentRuns::KnowledgeJobId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::RunKind)
                            .text()
                            .not_null()
                            .default("task"),
                    )
                    .col(ColumnDef::new(AgentRuns::Purpose).text().null())
                    .col(ColumnDef::new(AgentRuns::KnowledgeRevision).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::SourceBaselineId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(AgentRuns::SourceSnapshotId).text().null())
                    .col(ColumnDef::new(AgentRuns::KnowledgeViewSha256).text().null())
                    .col(ColumnDef::new(AgentRuns::InputOverlaySha256).text().null())
                    .col(ColumnDef::new(AgentRuns::SourceAuthorityKind).text().null())
                    .col(ColumnDef::new(AgentRuns::SourceRefName).text().null())
                    .col(ColumnDef::new(AgentRuns::SourceRawHead).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::MemoryEventId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(AgentRuns::TriggerId).big_integer().null())
                    .col(ColumnDef::new(AgentRuns::TriggerName).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::TriggerRevisionId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::PersonalityRevisionId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::SystemPromptEventId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(AgentRuns::ToolName).text().not_null())
                    .col(
                        ColumnDef::new(AgentRuns::Mutability)
                            .text()
                            .not_null()
                            .default("mutating"),
                    )
                    .col(ColumnDef::new(AgentRuns::Status).text().not_null())
                    .col(ColumnDef::new(AgentRuns::Command).text().not_null())
                    .col(ColumnDef::new(AgentRuns::WorkingDir).text().not_null())
                    .col(ColumnDef::new(AgentRuns::WorktreePath).text().null())
                    .col(ColumnDef::new(AgentRuns::BranchName).text().null())
                    .col(ColumnDef::new(AgentRuns::ProcessId).big_integer().null())
                    .col(ColumnDef::new(AgentRuns::ExitCode).big_integer().null())
                    .col(ColumnDef::new(AgentRuns::LogPath).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::DeveloperInstructionsPath)
                            .text()
                            .null(),
                    )
                    .col(ColumnDef::new(AgentRuns::UserPromptPath).text().null())
                    .col(ColumnDef::new(AgentRuns::AgentModel).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::AgentReasoningEffort)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::EffectiveInputSha256)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::EffectiveTimeoutSeconds)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::EffectiveConcurrencyGroup)
                            .text()
                            .null(),
                    )
                    .col(ColumnDef::new(AgentRuns::InputTokens).big_integer().null())
                    .col(
                        ColumnDef::new(AgentRuns::CachedInputTokens)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(AgentRuns::OutputTokens).big_integer().null())
                    .col(
                        ColumnDef::new(AgentRuns::CommitRequired)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::CommitOutcome)
                            .text()
                            .not_null()
                            .default("not_evaluated"),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::CommitShas)
                            .text()
                            .not_null()
                            .default("[]"),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::PrRequested)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(AgentRuns::PrUrl).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::CleanupStatus)
                            .text()
                            .not_null()
                            .default("not_applicable"),
                    )
                    .col(ColumnDef::new(AgentRuns::WorktreeCleanedAt).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::ResultSummary)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::SemanticPostconditionStatus)
                            .text()
                            .not_null()
                            .default("not_configured"),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::SemanticPostconditionFailures)
                            .text()
                            .not_null()
                            .default("[]"),
                    )
                    .col(ColumnDef::new(AgentRuns::StartedAt).text().null())
                    .col(ColumnDef::new(AgentRuns::FinishedAt).text().null())
                    .col(
                        ColumnDef::new(AgentRuns::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AgentRuns::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_runs_project")
                            .from(AgentRuns::Table, AgentRuns::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_runs_work_item")
                            .from(AgentRuns::Table, AgentRuns::WorkItemId)
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_agent_runs_knowledge_job")
                            .from(AgentRuns::Table, AgentRuns::KnowledgeJobId)
                            .to(Alias::new("knowledge_jobs"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;

        for index in [
            Index::create()
                .name("idx_agent_runs_project_status")
                .table(AgentRuns::Table)
                .col(AgentRuns::ProjectId)
                .col(AgentRuns::Status)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_agent_runs_trigger_id")
                .table(AgentRuns::Table)
                .col(AgentRuns::TriggerId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_agent_runs_project_item_created_id")
                .table(AgentRuns::Table)
                .col(AgentRuns::ProjectId)
                .col(AgentRuns::WorkItemId)
                .col((AgentRuns::CreatedAt, IndexOrder::Desc))
                .col((AgentRuns::Id, IndexOrder::Desc))
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_agent_runs_source_snapshot")
                .table(AgentRuns::Table)
                .col(AgentRuns::ProjectId)
                .col(AgentRuns::SourceSnapshotId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_agent_runs_knowledge_job")
                .table(AgentRuns::Table)
                .col(AgentRuns::KnowledgeJobId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("uq_agent_runs_project_id")
                .table(AgentRuns::Table)
                .col(AgentRuns::ProjectId)
                .col(AgentRuns::Id)
                .unique()
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AgentRuns::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000018_create_agent_runs_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_simple_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "agent_runs_read_view").await
    }
}
