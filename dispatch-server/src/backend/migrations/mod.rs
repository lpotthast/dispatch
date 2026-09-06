use sea_orm_migration::{
    prelude::*,
    sea_orm::{ConnectionTrait, Statement},
};

mod m20260906_000001_create_crudkit_validation;
mod m20260906_000002_create_projects;
mod m20260906_000003_create_knowledge_job_settings;
mod m20260906_000004_create_knowledge_jobs;
mod m20260906_000005_create_personalities;
mod m20260906_000006_create_work_item_groups;
mod m20260906_000007_create_work_items;
mod m20260906_000008_create_comments;
mod m20260906_000009_create_work_item_labels;
mod m20260906_000010_create_label_keys;
mod m20260906_000011_create_swim_lanes;
mod m20260906_000012_create_work_item_states;
mod m20260906_000013_create_work_item_relationships;
mod m20260906_000014_create_agent_tools;
mod m20260906_000015_create_automation_triggers;
mod m20260906_000016_create_automation_trigger_revisions;
mod m20260906_000017_create_personality_revisions;
mod m20260906_000018_create_agent_runs;
mod m20260906_000019_create_work_item_events;
mod m20260906_000020_create_automation_evaluations;
mod m20260906_000021_create_work_item_origins;
mod m20260906_000022_create_automation_bundle_applies;
mod m20260906_000023_create_agent_run_launch_contracts;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260906_000001_create_crudkit_validation::Migration),
            Box::new(m20260906_000002_create_projects::Migration),
            Box::new(m20260906_000003_create_knowledge_job_settings::Migration),
            Box::new(m20260906_000004_create_knowledge_jobs::Migration),
            Box::new(m20260906_000005_create_personalities::Migration),
            Box::new(m20260906_000006_create_work_item_groups::Migration),
            Box::new(m20260906_000007_create_work_items::Migration),
            Box::new(m20260906_000008_create_comments::Migration),
            Box::new(m20260906_000009_create_work_item_labels::Migration),
            Box::new(m20260906_000010_create_label_keys::Migration),
            Box::new(m20260906_000011_create_swim_lanes::Migration),
            Box::new(m20260906_000012_create_work_item_states::Migration),
            Box::new(m20260906_000013_create_work_item_relationships::Migration),
            Box::new(m20260906_000014_create_agent_tools::Migration),
            Box::new(m20260906_000015_create_automation_triggers::Migration),
            Box::new(m20260906_000016_create_automation_trigger_revisions::Migration),
            Box::new(m20260906_000017_create_personality_revisions::Migration),
            Box::new(m20260906_000018_create_agent_runs::Migration),
            Box::new(m20260906_000019_create_work_item_events::Migration),
            Box::new(m20260906_000020_create_automation_evaluations::Migration),
            Box::new(m20260906_000021_create_work_item_origins::Migration),
            Box::new(m20260906_000022_create_automation_bundle_applies::Migration),
            Box::new(m20260906_000023_create_agent_run_launch_contracts::Migration),
            Box::new(m20260906_000002_create_projects::ReadViewMigration),
            Box::new(m20260906_000005_create_personalities::ReadViewMigration),
            Box::new(m20260906_000007_create_work_items::ReadViewMigration),
            Box::new(m20260906_000008_create_comments::ReadViewMigration),
            Box::new(m20260906_000009_create_work_item_labels::ReadViewMigration),
            Box::new(m20260906_000010_create_label_keys::ReadViewMigration),
            Box::new(m20260906_000011_create_swim_lanes::ReadViewMigration),
            Box::new(m20260906_000012_create_work_item_states::ReadViewMigration),
            Box::new(m20260906_000014_create_agent_tools::ReadViewMigration),
            Box::new(m20260906_000015_create_automation_triggers::ReadViewMigration),
            Box::new(m20260906_000018_create_agent_runs::ReadViewMigration),
        ]
    }
}

pub(super) async fn create_simple_read_view(
    manager: &SchemaManager<'_>,
    table_name: &str,
    view_name: &str,
) -> Result<(), DbErr> {
    execute_sql(
        manager,
        &format!(
            r#"CREATE VIEW IF NOT EXISTS "{view_name}" AS
               SELECT "{table_name}".*, 0 AS "has_validation_errors"
               FROM "{table_name}""#
        ),
    )
    .await
}

pub(super) async fn drop_view(manager: &SchemaManager<'_>, view_name: &str) -> Result<(), DbErr> {
    execute_sql(manager, &format!(r#"DROP VIEW IF EXISTS "{view_name}""#)).await
}

pub(super) async fn execute_sql(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            sql.to_owned(),
        ))
        .await
        .map(|_| ())
}
