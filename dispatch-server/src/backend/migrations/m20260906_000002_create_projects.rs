use sea_orm_migration::prelude::*;

use super::{create_simple_read_view, drop_view};

#[derive(DeriveIden)]
enum Projects {
    Table,
    Id,
    Name,
    DisplayName,
    Path,
    KnowledgeDirectory,
    KnowledgeSourceLineageId,
    PathExists,
    PathCheckedAt,
    SystemPrompt,
    Memory,
    WorkspaceMode,
    MaxCodeEditAgents,
    MaxReadOnlyAgents,
    CreatePr,
    AutoCommit,
    CommitStandard,
    RevertStrategy,
    StaleClaimMinutes,
    WorktreeCleanupPolicy,
    DefaultAgentTool,
    DefaultAgentModel,
    DefaultAgentReasoningEffort,
    AgentSandboxMode,
    AgentExtraWritableRoots,
    AgentGitCommandPolicy,
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
                    .table(Projects::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Projects::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(Projects::Name).text().not_null().unique_key())
                    .col(ColumnDef::new(Projects::DisplayName).text().not_null())
                    .col(ColumnDef::new(Projects::Path).text().null())
                    .col(
                        ColumnDef::new(Projects::KnowledgeDirectory)
                            .text()
                            .not_null()
                            .default("knowledge"),
                    )
                    .col(ColumnDef::new(Projects::KnowledgeSourceLineageId).text().null())
                    .col(
                        ColumnDef::new(Projects::PathExists)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(ColumnDef::new(Projects::PathCheckedAt).text().null())
                    .col(
                        ColumnDef::new(Projects::SystemPrompt)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Projects::Memory)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Projects::WorkspaceMode)
                            .text()
                            .not_null()
                            .default("current_branch"),
                    )
                    .col(
                        ColumnDef::new(Projects::MaxCodeEditAgents)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(Projects::MaxReadOnlyAgents)
                            .big_integer()
                            .not_null()
                            .default(2),
                    )
                    .col(
                        ColumnDef::new(Projects::CreatePr)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(Projects::AutoCommit)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(Projects::CommitStandard)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Projects::RevertStrategy)
                            .text()
                            .not_null()
                            .default("manual"),
                    )
                    .col(
                        ColumnDef::new(Projects::StaleClaimMinutes)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(Projects::WorktreeCleanupPolicy)
                            .text()
                            .not_null()
                            .default("manual"),
                    )
                    .col(
                        ColumnDef::new(Projects::DefaultAgentTool)
                            .text()
                            .not_null()
                            .default("codex"),
                    )
                    .col(ColumnDef::new(Projects::DefaultAgentModel).text().null())
                    .col(ColumnDef::new(Projects::DefaultAgentReasoningEffort).text().null())
                    .col(
                        ColumnDef::new(Projects::AgentSandboxMode)
                            .text()
                            .not_null()
                            .default("workspace_write"),
                    )
                    .col(
                        ColumnDef::new(Projects::AgentExtraWritableRoots)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Projects::AgentGitCommandPolicy)
                            .text()
                            .not_null()
                            .default(
                                r#"{"add":true,"commit":true,"push":true,"reset":true,"hard_reset":"isolated_workspaces"}"#,
                            ),
                    )
                    .col(
                        ColumnDef::new(Projects::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Projects::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("uq_projects_knowledge_source_lineage")
                    .table(Projects::Table)
                    .col(Projects::KnowledgeSourceLineageId)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Projects::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000002_create_projects_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_simple_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "projects_read_view").await
    }
}
