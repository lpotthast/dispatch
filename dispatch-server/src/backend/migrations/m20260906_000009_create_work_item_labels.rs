use sea_orm_migration::prelude::*;

use super::{create_simple_read_view, drop_view};

#[derive(DeriveIden)]
enum WorkItemLabels {
    Table,
    Id,
    ProjectId,
    WorkItemId,
    LabelKey,
    LabelValue,
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
                    .table(WorkItemLabels::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItemLabels::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItemLabels::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemLabels::WorkItemId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkItemLabels::LabelKey).text().not_null())
                    .col(ColumnDef::new(WorkItemLabels::LabelValue).text().null())
                    .col(
                        ColumnDef::new(WorkItemLabels::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(WorkItemLabels::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_labels_project")
                            .from(WorkItemLabels::Table, WorkItemLabels::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_item_labels_work_item")
                            .from(WorkItemLabels::Table, WorkItemLabels::WorkItemId)
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_work_item_labels_project_key_value")
                .table(WorkItemLabels::Table)
                .col(WorkItemLabels::ProjectId)
                .col(WorkItemLabels::LabelKey)
                .col(WorkItemLabels::LabelValue)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_work_item_labels_unique_item_key")
                .table(WorkItemLabels::Table)
                .col(WorkItemLabels::WorkItemId)
                .col(WorkItemLabels::LabelKey)
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
            .drop_table(Table::drop().table(WorkItemLabels::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000009_create_work_item_labels_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_simple_read_view(manager, "work_item_labels", "work_item_labels_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "work_item_labels_read_view").await
    }
}
