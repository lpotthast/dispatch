use sea_orm_migration::prelude::*;

#[derive(DeriveIden)]
enum PersonalityRevisions {
    Table,
    Id,
    PersonalityId,
    ProjectId,
    PersonalityName,
    RevisionNumber,
    PersonalityDescription,
    Sha256,
    ChangeOperation,
    ActorType,
    ActorId,
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
                    .table(PersonalityRevisions::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(PersonalityRevisions::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::PersonalityId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::PersonalityName)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::RevisionNumber)
                            .big_integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::PersonalityDescription)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::Sha256)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::ChangeOperation)
                            .text()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(PersonalityRevisions::ActorType)
                            .text()
                            .null(),
                    )
                    .col(ColumnDef::new(PersonalityRevisions::ActorId).text().null())
                    .col(
                        ColumnDef::new(PersonalityRevisions::CreatedAt)
                            .text()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_personality_revisions_personality")
                            .from(
                                PersonalityRevisions::Table,
                                PersonalityRevisions::PersonalityId,
                            )
                            .to(Alias::new("personalities"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::SetNull),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_personality_revisions_project")
                            .from(PersonalityRevisions::Table, PersonalityRevisions::ProjectId)
                            .to(Alias::new("projects"), Alias::new("id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        for index in [
            Index::create()
                .name("idx_personality_revisions_unique")
                .table(PersonalityRevisions::Table)
                .col(PersonalityRevisions::PersonalityId)
                .col(PersonalityRevisions::RevisionNumber)
                .unique()
                .if_not_exists()
                .to_owned(),
            Index::create()
                .name("idx_personality_revisions_project_personality")
                .table(PersonalityRevisions::Table)
                .col(PersonalityRevisions::ProjectId)
                .col(PersonalityRevisions::PersonalityId)
                .col(PersonalityRevisions::RevisionNumber)
                .if_not_exists()
                .to_owned(),
        ] {
            manager.create_index(index).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(PersonalityRevisions::Table).to_owned())
            .await
    }
}
