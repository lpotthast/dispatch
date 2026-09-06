use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum AutomationTriggerRevisions {
    Table,
    Id,
    TriggerId,
    ProjectId,
    TriggerName,
    RevisionNumber,
    ConfigurationJson,
    Sha256,
    ChangeOperation,
    ActorType,
    ActorId,
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
                    .table(AutomationTriggerRevisions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::TriggerId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::TriggerName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::RevisionNumber)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::ConfigurationJson)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::Sha256)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::ChangeOperation)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::ActorType)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::ActorId)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggerRevisions::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_trigger_revisions_trigger")
                            .from(
                                AutomationTriggerRevisions::Table,
                                AutomationTriggerRevisions::TriggerId,
                            )
                            .to(Alias::new("automation_triggers"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_trigger_revisions_project")
                            .from(
                                AutomationTriggerRevisions::Table,
                                AutomationTriggerRevisions::ProjectId,
                            )
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_automation_trigger_revisions_unique")
                .table(AutomationTriggerRevisions::Table)
                .col(AutomationTriggerRevisions::TriggerId)
                .col(AutomationTriggerRevisions::RevisionNumber)
                .unique()
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_trigger_revisions_project_trigger")
                .table(AutomationTriggerRevisions::Table)
                .col(AutomationTriggerRevisions::ProjectId)
                .col(AutomationTriggerRevisions::TriggerId)
                .col(AutomationTriggerRevisions::RevisionNumber)
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AutomationTriggerRevisions::Table)
                    .to_owned(),
            )
            .await
    }
}
