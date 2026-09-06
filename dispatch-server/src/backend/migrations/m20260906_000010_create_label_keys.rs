use sea_orm_migration::prelude::*;

use super::{drop_view, execute_sql};

#[derive(DeriveIden)]
enum LabelKeys {
    Table,
    Id,
    ProjectId,
    LabelKey,
    AccentColor,
    Persistent,
    BuiltIn,
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
                    .table(LabelKeys::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(LabelKeys::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(LabelKeys::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(LabelKeys::LabelKey).text().not_null())
                    .col(ColumnDef::new(LabelKeys::AccentColor).text().null())
                    .col(
                        ColumnDef::new(LabelKeys::Persistent)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(LabelKeys::BuiltIn)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(LabelKeys::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(LabelKeys::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_label_keys_project")
                            .from(LabelKeys::Table, LabelKeys::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_label_keys_project_key")
                    .table(LabelKeys::Table)
                    .col(LabelKeys::ProjectId)
                    .col(LabelKeys::LabelKey)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(LabelKeys::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000010_create_label_keys_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        execute_sql(
            manager,
            r#"CREATE VIEW IF NOT EXISTS "label_keys_read_view" AS
               SELECT "label_keys".*,
                      COUNT("work_item_labels"."id") AS "usage_count",
                      MAX("work_item_labels"."updated_at") AS "last_used_at",
                      0 AS "has_validation_errors"
               FROM "label_keys"
               LEFT JOIN "work_item_labels"
                 ON "work_item_labels"."project_id" = "label_keys"."project_id"
                AND "work_item_labels"."label_key" = "label_keys"."label_key"
               GROUP BY "label_keys"."id""#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "label_keys_read_view").await
    }
}
