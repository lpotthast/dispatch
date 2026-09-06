use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum KnowledgeJobSettings {
    Table,
    ProjectId,
    Payload,
}

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(KnowledgeJobSettings::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(KnowledgeJobSettings::ProjectId)
                            .big_integer()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(KnowledgeJobSettings::Payload)
                            .text()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_knowledge_job_settings_project")
                            .from(KnowledgeJobSettings::Table, KnowledgeJobSettings::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KnowledgeJobSettings::Table).to_owned())
            .await
    }
}
