use dispatch_types::{AgentRunKind, AgentRunPurposeV1, AgentRunStatus};
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::backend::{
    entities::{
        agent_run::{self, AgentRun},
        automation_trigger::AutomationTrigger,
    },
    runs::launch::model::PersistedLaunchContract,
    runs::launch::repository::{self as agent_run_launch},
    storage::Transaction,
};

/// The run fields needed to validate attribution, decoded at the persistence boundary.
pub(super) struct AttributionRun {
    pub(super) kind: AgentRunKind,
    pub(super) status: AgentRunStatus,
    pub(super) purpose: Option<AgentRunPurposeV1>,
    pub(super) knowledge_job_id: Option<i64>,
    pub(super) trigger_id: Option<i64>,
    pub(super) trigger_revision_id: Option<i64>,
    pub(super) trigger_name: Option<String>,
}

pub(crate) struct AttributionRepository;

impl AttributionRepository {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(super) async fn run_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<AttributionRun> {
        let run = AgentRun::find_by_id(run_id)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to validate request agent run")?
            .ok_or_else(|| report!("agent run {run_id} does not exist in this project"))?;
        Ok(AttributionRun {
            kind: run
                .run_kind
                .parse()
                .context("invalid persisted agent run kind")?,
            status: run
                .status
                .parse()
                .context("invalid persisted agent run status")?,
            purpose: run
                .purpose
                .map(|purpose| purpose.parse())
                .transpose()
                .context("invalid persisted agent run purpose")?,
            knowledge_job_id: run.knowledge_job_id,
            trigger_id: run.trigger_id,
            trigger_revision_id: run.trigger_revision_id,
            trigger_name: run.trigger_name,
        })
    }

    pub(super) async fn contract_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<Option<PersistedLaunchContract>> {
        agent_run_launch::load_contract(transaction.connection(), project_id, run_id).await
    }

    pub(super) async fn bundle_key_in(
        &self,
        transaction: &Transaction,
        trigger_id: i64,
    ) -> Result<Option<String>> {
        Ok(AutomationTrigger::find_by_id(trigger_id)
            .one(transaction.connection())
            .await
            .context("failed to load request automation origin")?
            .and_then(|trigger| trigger.managed_bundle_key))
    }
}
