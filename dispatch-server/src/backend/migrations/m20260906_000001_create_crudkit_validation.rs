use sea_orm_migration::prelude::*;

#[derive(Iden)]
enum CrudkitValidation {
    #[iden = "CrudkitValidation"]
    Table,
    Id,
    ResourceName,
    EntityId,
    ValidatorName,
    ValidatorVersion,
    ViolationSeverity,
    ViolationMessage,
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
                    .table(CrudkitValidation::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(CrudkitValidation::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::ResourceName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::EntityId)
                            .json_binary()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::ValidatorName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::ValidatorVersion)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::ViolationSeverity)
                            .string_len(16)
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::ViolationMessage)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(CrudkitValidation::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_crudkit_validation_resource_name")
                    .table(CrudkitValidation::Table)
                    .col(CrudkitValidation::ResourceName)
                    .if_not_exists()
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(CrudkitValidation::Table).to_owned())
            .await
    }
}
