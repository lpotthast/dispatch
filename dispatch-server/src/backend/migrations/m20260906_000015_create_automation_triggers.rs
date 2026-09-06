use sea_orm_migration::prelude::*;

use super::{drop_view, execute_sql};

#[derive(DeriveIden)]
enum AutomationTriggers {
    Table,
    Id,
    ProjectId,
    Name,
    Enabled,
    Activation,
    Effect,
    Schedule,
    ToolName,
    Mutability,
    PersonalityId,
    Prompt,
    WorkItemSelector,
    Priority,
    Exclusive,
    ProducedWorkSpecJson,
    PostconditionsJson,
    ModelOverride,
    ReasoningEffortOverride,
    TimeoutSeconds,
    MaxConcurrentRuns,
    ConcurrencyGroup,
    CurrentRevisionId,
    ManagedBundleKey,
    ManagedObjectKey,
    EvaluationCount,
    PendingEvaluationCount,
    LastEvaluationQueuedAt,
    LastEvaluatedAt,
    NextEvaluationAt,
    LastEventId,
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
                    .table(AutomationTriggers::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AutomationTriggers::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(AutomationTriggers::Name).text().not_null())
                    .col(
                        ColumnDef::new(AutomationTriggers::Enabled)
                            .boolean()
                            .not_null()
                            .default(true),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Activation)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Effect)
                            .text()
                            .not_null()
                            .default("consume_work"),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Schedule)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ToolName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Mutability)
                            .text()
                            .not_null()
                            .default("mutating"),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::PersonalityId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Prompt)
                            .text()
                            .not_null()
                            .default(""),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::WorkItemSelector)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Priority)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::Exclusive)
                            .boolean()
                            .not_null()
                            .default(false),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ProducedWorkSpecJson)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::PostconditionsJson)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ModelOverride)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ReasoningEffortOverride)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::TimeoutSeconds)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::MaxConcurrentRuns)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ConcurrencyGroup)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::CurrentRevisionId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ManagedBundleKey)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::ManagedObjectKey)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::EvaluationCount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::PendingEvaluationCount)
                            .big_integer()
                            .not_null()
                            .default(0),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::LastEvaluationQueuedAt)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::LastEvaluatedAt)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::NextEvaluationAt)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::LastEventId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::CreatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(AutomationTriggers::UpdatedAt)
                            .text()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_triggers_project")
                            .from(AutomationTriggers::Table, AutomationTriggers::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_triggers_personality")
                            .from(AutomationTriggers::Table, AutomationTriggers::PersonalityId)
                            .to(Alias::new("personalities"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_automation_triggers_project_activation")
                    .table(AutomationTriggers::Table)
                    .col(AutomationTriggers::ProjectId)
                    .col(AutomationTriggers::Activation)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        execute_sql(
            manager,
            r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_automation_trigger_managed_key"
               ON "automation_triggers" ("project_id", "managed_bundle_key", "managed_object_key")
               WHERE "managed_bundle_key" IS NOT NULL"#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(AutomationTriggers::Table).to_owned())
            .await
    }
}

pub struct ReadViewMigration;

impl MigrationName for ReadViewMigration {
    fn name(&self) -> &str {
        "m20260906_000015_create_automation_triggers_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for ReadViewMigration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        execute_sql(
            manager,
            r#"CREATE VIEW IF NOT EXISTS "automation_triggers_read_view" AS
               SELECT "automation_triggers".*,
                      "personalities"."name" AS "personality_name",
                      0 AS "has_validation_errors"
               FROM "automation_triggers"
               LEFT JOIN "personalities"
                 ON "personalities"."id" = "automation_triggers"."personality_id"
                AND "personalities"."project_id" = "automation_triggers"."project_id""#,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_view(manager, "automation_triggers_read_view").await
    }
}
