//! Frozen knowledge migrations restored for databases that already recorded later versions.
//!
//! Each migration keeps its historical name and schema independent of current entities.
//! The larger files intentionally remain single compatibility units: mechanically splitting
//! declarative historical schemas would increase review and replay risk without changing ownership.

use sea_orm_migration::prelude::*;

mod m20260904_000044_migrate_projects_to_newest_gpt_model;
mod m20260904_000045_add_knowledge_operational_foundations;
mod m20260904_000046_generalize_knowledge_source_views;
mod m20260905_000047_add_knowledge_cycle_lifecycle;
mod m20260905_000048_add_knowledge_execution_fencing;
mod m20260905_000049_support_knowledge_agent_runs;
mod m20260905_000050_add_knowledge_inventory_impact;
mod m20260905_000051_add_knowledge_evidence_proposals;

pub(super) use m20260904_000044_migrate_projects_to_newest_gpt_model::Migration as MigrateProjectsToNewestGptModel;
pub(super) use m20260904_000045_add_knowledge_operational_foundations::Migration as AddKnowledgeOperationalFoundations;
pub(super) use m20260904_000046_generalize_knowledge_source_views::Migration as GeneralizeKnowledgeSourceViews;
pub(super) use m20260905_000047_add_knowledge_cycle_lifecycle::Migration as AddKnowledgeCycleLifecycle;
pub(super) use m20260905_000048_add_knowledge_execution_fencing::Migration as AddKnowledgeExecutionFencing;
pub(super) use m20260905_000049_support_knowledge_agent_runs::Migration as SupportKnowledgeAgentRuns;
pub(super) use m20260905_000050_add_knowledge_inventory_impact::Migration as AddKnowledgeInventoryImpact;
pub(super) use m20260905_000051_add_knowledge_evidence_proposals::Migration as AddKnowledgeEvidenceProposals;

async fn add_column_if_missing(
    manager: &SchemaManager<'_>,
    table_name: &str,
    mut column: ColumnDef,
) -> Result<(), DbErr> {
    if manager
        .has_column(table_name, column.get_column_name())
        .await?
    {
        return Ok(());
    }
    manager
        .alter_table(
            Table::alter()
                .table(Alias::new(table_name))
                .add_column(&mut column)
                .to_owned(),
        )
        .await
}
