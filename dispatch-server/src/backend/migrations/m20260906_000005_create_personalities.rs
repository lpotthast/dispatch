use sea_orm_migration::prelude::*;

use super::{create_simple_read_view, drop_view, execute_sql};

#[derive(DeriveIden)]
enum Personalities {
    Table,
    Id,
    ProjectId,
    Name,
    PersonalityDescription,
    CurrentRevisionId,
    ManagedBundleKey,
    ManagedObjectKey,
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
                    .table(Personalities::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(Personalities::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Personalities::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Personalities::Name).text().not_null())
                    .col(
                        ColumnDef::new(Personalities::PersonalityDescription)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(Personalities::CurrentRevisionId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Personalities::ManagedBundleKey)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Personalities::ManagedObjectKey)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(Personalities::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(Personalities::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_personalities_project")
                            .from(Personalities::Table, Personalities::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_personalities_project_name_unique")
                    .table(Personalities::Table)
                    .col(Personalities::ProjectId)
                    .col(Personalities::Name)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        execute_sql(
            manager,
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_personality_managed_key"
               ON "personalities" ("project_id", "managed_bundle_key", "managed_object_key")
               WHERE "managed_bundle_key" IS NOT NULL"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Personalities::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000005_create_personalities_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_simple_read_view(manager, "personalities", "personalities_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "personalities_read_view").await
    }
}
