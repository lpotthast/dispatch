use sea_orm_migration::prelude::*;

use super::execute_sql;

#[derive(DeriveIden)]
enum KnowledgeJobs {
    Table,
    Id,
    ProjectId,
    RequestId,
    Unresolved,
    Version,
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
                    .table(KnowledgeJobs::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(KnowledgeJobs::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(KnowledgeJobs::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(KnowledgeJobs::RequestId).text().not_null())
                    .col(
                        ColumnDef::new(KnowledgeJobs::Unresolved)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(KnowledgeJobs::Version)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(ColumnDef::new(KnowledgeJobs::Payload).text().not_null())
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_knowledge_jobs_project")
                            .from(KnowledgeJobs::Table, KnowledgeJobs::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_knowledge_jobs_project_request")
                    .table(KnowledgeJobs::Table)
                    .col(KnowledgeJobs::ProjectId)
                    .col(KnowledgeJobs::RequestId)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        execute_sql(
            manager,
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_knowledge_jobs_unresolved ON knowledge_jobs(project_id) WHERE unresolved = 1",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(KnowledgeJobs::Table).to_owned())
            .await
    }
}
