use sea_orm_migration::prelude::*;

use super::{create_simple_read_view, drop_view};

#[derive(DeriveIden)]
enum SwimLanes {
    Table,
    Id,
    ProjectId,
    Identifier,
    Name,
    Position,
    Filter,
    ItemOrder,
    CanCreateItems,
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
                    .table(SwimLanes::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(SwimLanes::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(SwimLanes::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(SwimLanes::Identifier).text().not_null())
                    .col(ColumnDef::new(SwimLanes::Name).text().not_null())
                    .col(
                        ColumnDef::new(SwimLanes::Position)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(SwimLanes::Filter)
                            .text()
                            .not_null()
                            .default(r#"{"All":[]}"#),
                    )
                    .col(
                        ColumnDef::new(SwimLanes::ItemOrder)
                            .text()
                            .not_null()
                            .default("updated_desc"),
                    )
                    .col(
                        ColumnDef::new(SwimLanes::CanCreateItems)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(SwimLanes::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(SwimLanes::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_swim_lanes_project")
                            .from(SwimLanes::Table, SwimLanes::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_swim_lanes_unique_project_identifier")
                    .table(SwimLanes::Table)
                    .col(SwimLanes::ProjectId)
                    .col(SwimLanes::Identifier)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(SwimLanes::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000011_create_swim_lanes_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_simple_read_view(manager, "swim_lanes", "swim_lanes_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "swim_lanes_read_view").await
    }
}
