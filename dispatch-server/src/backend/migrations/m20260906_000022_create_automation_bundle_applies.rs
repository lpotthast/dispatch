use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum AutomationBundleApplies {
    Table,
    Id,
    ProjectId,
    BundleKey,
    DisplayName,
    ManifestHash,
    AppliedDiffJson,
    ActorType,
    ActorId,
    Status,
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
                    .table(AutomationBundleApplies::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(AutomationBundleApplies::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::BundleKey)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::DisplayName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::ManifestHash)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::AppliedDiffJson)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::ActorType)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::ActorId)
                            .text()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::Status)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(AutomationBundleApplies::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_automation_bundle_applies_project")
                            .from(
                                AutomationBundleApplies::Table,
                                AutomationBundleApplies::ProjectId,
                            )
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_bundle_applies_project_key")
                    .table(AutomationBundleApplies::Table)
                    .col(AutomationBundleApplies::ProjectId)
                    .col(AutomationBundleApplies::BundleKey)
                    .col(AutomationBundleApplies::Id)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(AutomationBundleApplies::Table)
                    .to_owned(),
            )
            .await
    }
}
