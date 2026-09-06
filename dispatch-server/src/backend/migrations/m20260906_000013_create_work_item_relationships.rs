use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum WorkItemRelationships {
    Table,
    Id,
    ProjectId,
    SourceWorkItemId,
    TargetWorkItemId,
    Kind,
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
                    .table(WorkItemRelationships::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItemRelationships::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItemRelationships::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemRelationships::SourceWorkItemId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemRelationships::TargetWorkItemId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemRelationships::Kind)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemRelationships::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(WorkItemRelationships::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_relationships_project")
                            .from(
                                WorkItemRelationships::Table,
                                WorkItemRelationships::ProjectId,
                            )
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_relationships_source")
                            .from(
                                WorkItemRelationships::Table,
                                WorkItemRelationships::SourceWorkItemId,
                            )
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_relationships_target")
                            .from(
                                WorkItemRelationships::Table,
                                WorkItemRelationships::TargetWorkItemId,
                            )
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_work_item_relationships_touching_source")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::SourceWorkItemId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_work_item_relationships_touching_target")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::TargetWorkItemId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_work_item_relationships_unique")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::SourceWorkItemId)
                .col(WorkItemRelationships::TargetWorkItemId)
                .col(WorkItemRelationships::Kind)
                .unique()
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_relationships_project_kind")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::Kind)
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkItemRelationships::Table).to_owned())
            .await
    }
}
