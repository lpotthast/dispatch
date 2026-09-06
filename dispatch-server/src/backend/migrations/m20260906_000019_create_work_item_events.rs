use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum WorkItemEvents {
    Table,
    Id,
    ProjectId,
    WorkItemId,
    EventType,
    Body,
    ActorType,
    ActorId,
    AgentRunId,
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
                    .table(WorkItemEvents::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItemEvents::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItemEvents::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemEvents::WorkItemId)
                            .big_integer()
                            .null(),
                    )
                    .col(ColumnDef::new(WorkItemEvents::EventType).text().not_null())
                    .col(ColumnDef::new(WorkItemEvents::Body).text().not_null())
                    .col(ColumnDef::new(WorkItemEvents::ActorType).text().null())
                    .col(ColumnDef::new(WorkItemEvents::ActorId).text().null())
                    .col(
                        ColumnDef::new(WorkItemEvents::AgentRunId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemEvents::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_events_project")
                            .from(WorkItemEvents::Table, WorkItemEvents::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_events_work_item")
                            .from(WorkItemEvents::Table, WorkItemEvents::WorkItemId)
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_work_item_events_project_created")
                .table(WorkItemEvents::Table)
                .col(WorkItemEvents::ProjectId)
                .col(WorkItemEvents::CreatedAt)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_work_item_events_project_type_id")
                .table(WorkItemEvents::Table)
                .col(WorkItemEvents::ProjectId)
                .col(WorkItemEvents::EventType)
                .col(WorkItemEvents::Id)
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkItemEvents::Table).to_owned())
            .await
    }
}
