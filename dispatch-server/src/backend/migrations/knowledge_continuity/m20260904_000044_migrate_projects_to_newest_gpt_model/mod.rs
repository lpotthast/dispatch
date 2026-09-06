use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::Statement;

pub struct Migration;

impl MigrationName for Migration {
    fn name(&self) -> &str {
        "m20260904_000044_migrate_projects_to_newest_gpt_model"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // This historical migration legitimately updated every project when it first ran.
        // Some databases recorded later knowledge-mutation migrations while 044 was absent;
        // replaying the old blanket update during that narrow gap repair would overwrite
        // current operator choices.
        let later_mutation_migration_is_applied = manager
            .get_connection()
            .query_one(Statement::from_string(
                manager.get_database_backend(),
                r#"
                SELECT EXISTS (
                    SELECT 1
                    FROM seaql_migrations
                    WHERE version IN (
                        'm20260905_000052_add_knowledge_mutation_operations',
                        'm20260905_000053_add_knowledge_mutation_binding'
                    )
                ) AS applied
                "#
                .to_owned(),
            ))
            .await?
            .ok_or_else(|| DbErr::Migration("failed to inspect migration history".to_owned()))?
            .try_get::<i64>("", "applied")?
            != 0;
        if later_mutation_migration_is_applied {
            return Ok(());
        }

        manager
            .get_connection()
            .execute(
                manager.get_database_backend().build(
                    &Query::update()
                        .table(Tables::Projects)
                        .value(Columns::DefaultAgentModel, "gpt-5.6-sol")
                        .value(Columns::DefaultAgentReasoningEffort, "xhigh")
                        .to_owned(),
                ),
            )
            .await
            .map(|_| ())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        // The prior project-specific model/effort pairs were overwritten and cannot be recovered.
        // A rollback therefore removes only the migration record and intentionally keeps the data.
        Ok(())
    }
}

#[derive(DeriveIden)]
enum Tables {
    Projects,
}

#[derive(DeriveIden)]
enum Columns {
    DefaultAgentModel,
    DefaultAgentReasoningEffort,
}
