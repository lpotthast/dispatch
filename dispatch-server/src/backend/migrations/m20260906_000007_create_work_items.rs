use sea_orm_migration::prelude::*;

use super::{drop_view, execute_sql};

#[derive(DeriveIden)]
enum WorkItems {
    Table,
    Id,
    ProjectId,
    WorkGroupId,
    Title,
    Description,
    ClaimedBy,
    ClaimedAt,
    ClaimExpiresAt,
    FinishedAt,
    AgentModelOverride,
    AgentReasoningEffortOverride,
    Version,
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
                    .table(WorkItems::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItems::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItems::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkItems::WorkGroupId).big_integer().null())
                    .col(ColumnDef::new(WorkItems::Title).text().not_null())
                    .col(ColumnDef::new(WorkItems::Description).text().not_null())
                    .col(ColumnDef::new(WorkItems::ClaimedBy).text().null())
                    .col(ColumnDef::new(WorkItems::ClaimedAt).text().null())
                    .col(ColumnDef::new(WorkItems::ClaimExpiresAt).text().null())
                    .col(ColumnDef::new(WorkItems::FinishedAt).text().null())
                    .col(ColumnDef::new(WorkItems::AgentModelOverride).text().null())
                    .col(
                        ColumnDef::new(WorkItems::AgentReasoningEffortOverride)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItems::Version)
                            .big_integer()
                            .not_null()
                            .default(1),
                    )
                    .col(
                        ColumnDef::new(WorkItems::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(WorkItems::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_items_project")
                            .from(WorkItems::Table, WorkItems::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_work_items_group")
                            .from(WorkItems::Table, WorkItems::WorkGroupId)
                            .to(Alias::new("work_item_groups"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_work_items_project_group")
                .table(WorkItems::Table)
                .col(WorkItems::ProjectId)
                .col(WorkItems::WorkGroupId)
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_work_items_project_updated_search")
                .table(WorkItems::Table)
                .col(WorkItems::ProjectId)
                .col((WorkItems::UpdatedAt, IndexOrder::Desc))
                .col((WorkItems::Id, IndexOrder::Desc))
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(WorkItems::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000007_create_work_items_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        execute_sql(
            manager,
            r#"CREATE VIEW IF NOT EXISTS "work_items_read_view" AS
               SELECT "work_items".*,
                      (SELECT "work_item_labels"."label_value"
                       FROM "work_item_labels"
                       WHERE "work_item_labels"."project_id" = "work_items"."project_id"
                         AND "work_item_labels"."work_item_id" = "work_items"."id"
                         AND "work_item_labels"."label_key" = 'state'
                       LIMIT 1) AS "state_label",
                      0 AS "has_validation_errors"
               FROM "work_items""#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "work_items_read_view").await
    }
}
