use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum AutomationEvaluations {
    Table,
    Id,
    ProjectId,
    TriggerId,
    TriggerRevisionId,
    TriggerName,
    ActivationCause,
    Outcome,
    WorkItemId,
    RunId,
    Error,
    CreatedAt,
    CompletedAt,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(AutomationEvaluations::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AutomationEvaluations::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::TriggerId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::TriggerRevisionId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::TriggerName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::ActivationCause)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::Outcome)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::WorkItemId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::RunId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(AutomationEvaluations::Error).text().null())
                    .col(
                        ColumnDef::new(AutomationEvaluations::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationEvaluations::CompletedAt)
                            .text()
                            .null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_evaluations_project")
                            .from(
                                AutomationEvaluations::Table,
                                AutomationEvaluations::ProjectId,
                            )
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_evaluations_trigger")
                            .from(
                                AutomationEvaluations::Table,
                                AutomationEvaluations::TriggerId,
                            )
                            .to(Alias::new("automation_triggers"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_evaluations_trigger_revision")
                            .from(
                                AutomationEvaluations::Table,
                                AutomationEvaluations::TriggerRevisionId,
                            )
                            .to(Alias::new("automation_trigger_revisions"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_evaluations_work_item")
                            .from(
                                AutomationEvaluations::Table,
                                AutomationEvaluations::WorkItemId,
                            )
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_evaluations_run")
                            .from(AutomationEvaluations::Table, AutomationEvaluations::RunId)
                            .to(Alias::new("agent_runs"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_automation_evaluations_project_trigger")
                    .table(AutomationEvaluations::Table)
                    .col(AutomationEvaluations::ProjectId)
                    .col(AutomationEvaluations::TriggerId)
                    .col(AutomationEvaluations::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AutomationEvaluations::Table).to_owned())
            .await
    }
}
