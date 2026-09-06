use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum WorkItemGroups {
    Table,
    Id,
    ProjectId,
    GroupKey,
    Name,
    ActorId,
    AgentRunId,
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
                    .table(WorkItemGroups::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItemGroups::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItemGroups::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkItemGroups::GroupKey).text().not_null())
                    .col(ColumnDef::new(WorkItemGroups::Name).text().not_null())
                    .col(ColumnDef::new(WorkItemGroups::ActorId).text().null())
                    .col(
                        ColumnDef::new(WorkItemGroups::AgentRunId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemGroups::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(WorkItemGroups::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_groups_project")
                            .from(WorkItemGroups::Table, WorkItemGroups::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_work_item_groups_project_key")
                    .table(WorkItemGroups::Table)
                    .col(WorkItemGroups::ProjectId)
                    .col(WorkItemGroups::GroupKey)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkItemGroups::Table).to_owned())
            .await
    }
}
