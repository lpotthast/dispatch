use super::RunningRunCounts;
use crate::backend::{
    entities::agent_run::{self, AgentRun},
    storage::Transaction,
};
use dispatch_types::{AgentRunStatus, AutomationRunMutability};
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, PaginatorTrait, QueryFilter, QuerySelect};

pub(crate) struct RunAdmissionRepository;
impl RunAdmissionRepository {
    pub(crate) async fn counts_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
    ) -> Result<RunningRunCounts> {
        let mutabilities = AgentRun::find()
            .select_only()
            .column(agent_run::Column::Mutability)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
            .into_tuple::<String>()
            .all(transaction.connection())
            .await
            .context("failed to load running agent run mutabilities")?;
        let mut counts = RunningRunCounts::default();
        for mutability in mutabilities {
            match mutability.parse::<AutomationRunMutability>()? {
                AutomationRunMutability::Mutating => counts.mutating += 1,
                AutomationRunMutability::ReadOnly => counts.read_only += 1,
            }
        }
        Ok(counts)
    }
    pub(crate) async fn rule_count_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        trigger_id: i64,
    ) -> Result<u64> {
        Ok(AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
            .filter(agent_run::Column::TriggerId.eq(trigger_id))
            .count(transaction.connection())
            .await
            .context("failed to count running automation rule runs")?)
    }
    pub(crate) async fn group_count_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        group: &str,
    ) -> Result<u64> {
        Ok(AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()))
            .filter(agent_run::Column::EffectiveConcurrencyGroup.eq(group))
            .count(transaction.connection())
            .await
            .context("failed to inspect automation concurrency group")?)
    }
}
