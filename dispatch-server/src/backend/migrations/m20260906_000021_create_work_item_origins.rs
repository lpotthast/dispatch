use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum WorkItemOrigins {
    Table,
    WorkItemId,
    ProjectId,
    OriginKind,
    ActorId,
    AgentRunId,
    ProducingEvaluationId,
    TriggerId,
    TriggerRevisionId,
    TriggerName,
    BundleKey,
    DeduplicationKey,
    CreatedAt,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(WorkItemOrigins::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItemOrigins::WorkItemId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItemOrigins::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemOrigins::OriginKind)
                            .text()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkItemOrigins::ActorId).text().null())
                    .col(
                        ColumnDef::new(WorkItemOrigins::AgentRunId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemOrigins::ProducingEvaluationId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemOrigins::TriggerId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemOrigins::TriggerRevisionId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(WorkItemOrigins::TriggerName).text().null())
                    .col(ColumnDef::new(WorkItemOrigins::BundleKey).text().null())
                    .col(
                        ColumnDef::new(WorkItemOrigins::DeduplicationKey)
                            .text()
                            .null(),
                    )
                    .col(ColumnDef::new(WorkItemOrigins::CreatedAt).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_origins_work_item")
                            .from(WorkItemOrigins::Table, WorkItemOrigins::WorkItemId)
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_origins_project")
                            .from(WorkItemOrigins::Table, WorkItemOrigins::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_origins_run")
                            .from(WorkItemOrigins::Table, WorkItemOrigins::AgentRunId)
                            .to(Alias::new("agent_runs"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_origins_evaluation")
                            .from(
                                WorkItemOrigins::Table,
                                WorkItemOrigins::ProducingEvaluationId,
                            )
                            .to(Alias::new("automation_evaluations"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_origins_trigger")
                            .from(WorkItemOrigins::Table, WorkItemOrigins::TriggerId)
                            .to(Alias::new("automation_triggers"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_origins_trigger_revision")
                            .from(WorkItemOrigins::Table, WorkItemOrigins::TriggerRevisionId)
                            .to(Alias::new("automation_trigger_revisions"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_work_item_origins_run")
                .table(WorkItemOrigins::Table)
                .col(WorkItemOrigins::ProjectId)
                .col(WorkItemOrigins::AgentRunId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_work_item_origins_trigger")
                .table(WorkItemOrigins::Table)
                .col(WorkItemOrigins::ProjectId)
                .col(WorkItemOrigins::TriggerId)
                .col(WorkItemOrigins::DeduplicationKey)
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkItemOrigins::Table).to_owned())
            .await
    }
}
