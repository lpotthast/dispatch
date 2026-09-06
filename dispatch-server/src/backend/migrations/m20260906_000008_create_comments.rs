use sea_orm_migration::prelude::*;

use super::{create_simple_read_view, drop_view};

#[derive(DeriveIden)]
enum Comments {
    Table,
    Id,
    WorkItemId,
    AuthorType,
    AuthorName,
    Body,
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
                    .table(Comments::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Comments::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Comments::WorkItemId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Comments::AuthorType).text().not_null())
                    .col(ColumnDef::new(Comments::AuthorName).text().null())
                    .col(ColumnDef::new(Comments::Body).text().not_null())
                    .col(
                        ColumnDef::new(Comments::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_comments_work_item")
                            .from(Comments::Table, Comments::WorkItemId)
                            .to(Alias::new("work_items"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_comments_work_item_created")
                    .table(Comments::Table)
                    .col(Comments::WorkItemId)
                    .col(Comments::CreatedAt)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Comments::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000008_create_comments_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_simple_read_view(manager, "comments", "comments_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "comments_read_view").await
    }
}
