use crate::backend::{
    items::creation::model::InsertWorkItemOrigin,
    items::events::model::EventAttribution,
    runs::launch::model::{AgentLaunchResolutionV1, AgentLaunchTargetV1, PersistedLaunchContract},
};
use dispatch_types::{AgentRunKind, AgentRunStatus, AuthorType, WorkItemOriginKind};
use rootcause::{Result, prelude::*};

#[derive(Clone, Debug, Default)]
pub(crate) struct RequestAttribution {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_run_id: Option<i64>,
    pub(super) run_kind: Option<AgentRunKind>,
    pub(super) run_status: Option<AgentRunStatus>,
    pub(super) trigger_id: Option<i64>,
    pub(super) trigger_revision_id: Option<i64>,
    pub(super) trigger_name: Option<String>,
    pub(super) bundle_key: Option<String>,
    pub(super) launch_contract: Option<PersistedLaunchContract>,
}

impl RequestAttribution {
    pub(crate) fn cross_check_agent_id(&self, body_agent_id: &str) -> Result<()> {
        if let Some(header_agent_id) = &self.agent_id
            && header_agent_id != body_agent_id
        {
            bail!(
                "request agent id '{header_agent_id}' does not match body agent id '{body_agent_id}'"
            );
        }
        Ok(())
    }

    /// Generic claim is never a valid operation for a contracted run. Its target was resolved by
    /// the server before the process started. Legacy pre-049 runs retain compatibility.
    pub(crate) fn ensure_generic_claim(&self) -> Result<()> {
        if self.launch_contract.is_some() {
            bail!("persisted launch contracts cannot call the generic item claim endpoint");
        }
        Ok(())
    }

    pub(crate) fn ensure_item_mutation(&self, operation: &str) -> Result<()> {
        let Some(contract) = &self.launch_contract else {
            return Ok(());
        };
        if contract.purpose != dispatch_types::AgentRunPurposeV1::Ordinary
            || matches!(contract.target, AgentLaunchTargetV1::None { .. })
            || !matches!(contract.resolution, AgentLaunchResolutionV1::Claimed { .. })
        {
            bail!("this agent run's persisted launch contract cannot {operation}");
        }
        Ok(())
    }

    pub(crate) fn event(&self) -> EventAttribution<'_> {
        EventAttribution {
            actor_type: self.agent_id.as_ref().map(|_| AuthorType::Agent),
            actor_id: self.agent_id.as_deref(),
            agent_run_id: self.agent_run_id,
        }
    }

    pub(crate) fn item_origin(&self) -> InsertWorkItemOrigin {
        match &self.agent_id {
            Some(agent_id) => InsertWorkItemOrigin {
                kind: WorkItemOriginKind::AgentRun,
                actor_id: Some(agent_id.clone()),
                agent_run_id: self.agent_run_id,
                trigger_id: self.trigger_id,
                trigger_revision_id: self.trigger_revision_id,
                trigger_name: self.trigger_name.clone(),
                bundle_key: self.bundle_key.clone(),
                ..InsertWorkItemOrigin::default()
            },
            None => InsertWorkItemOrigin::default(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AttributionInput {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_run_id: Option<i64>,
}
