use super::{
    model::{AttributionInput, RequestAttribution},
    repository::AttributionRepository,
};
use crate::backend::{
    execution::identity as agent_ids,
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{AgentRunKind, AgentRunPurposeV1, AgentRunStatus};
use rootcause::{Result, prelude::*};
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct AttributionService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<AttributionRepository>,
}

impl AttributionService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<AttributionRepository>,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
        }
    }

    pub(crate) async fn validate(
        &self,
        project_name: &str,
        input: AttributionInput,
    ) -> Result<RequestAttribution> {
        self.validate_inner(project_name, input, false).await
    }

    pub(crate) async fn validate_knowledge(
        &self,
        project_name: &str,
        input: AttributionInput,
    ) -> Result<RequestAttribution> {
        self.validate_inner(project_name, input, true).await
    }

    async fn validate_inner(
        &self,
        project_name: &str,
        input: AttributionInput,
        knowledge_read: bool,
    ) -> Result<RequestAttribution> {
        let transaction = self.transactions.begin().await?;
        let attribution = self
            .validate_in(&transaction, project_name, input, knowledge_read)
            .await?;
        transaction.commit().await?;
        Ok(attribution)
    }

    pub(crate) async fn validate_in(
        &self,
        transaction: &Transaction,
        project_name: &str,
        input: AttributionInput,
        knowledge_read: bool,
    ) -> Result<RequestAttribution> {
        let AttributionInput {
            agent_id,
            agent_run_id,
        } = input;
        if agent_run_id.is_some() && agent_id.is_none() {
            bail!("x-dispatch-agent-run-id requires x-dispatch-agent-id");
        }
        if knowledge_read && agent_id.is_some() && agent_run_id.is_none() {
            bail!("knowledge agent attribution requires x-dispatch-agent-run-id");
        }
        if let Some(agent_id) = &agent_id {
            agent_ids::validate_agent_id(agent_id)?;
        }
        let mut trigger_id = None;
        let mut trigger_revision_id = None;
        let mut trigger_name = None;
        let mut bundle_key = None;
        let mut run_kind = None;
        let mut run_status = None;
        let mut launch_contract = None;
        let mut knowledge_job_id = None;
        if let Some(run_id) = agent_run_id {
            let project_id = self.projects.id_in(transaction, project_name).await?;
            let run = self
                .repository
                .run_in(transaction, project_id, run_id)
                .await?;
            let expected_agent_id = agent_ids::dispatch_run_agent_id(run_id);
            if agent_id.as_deref() != Some(expected_agent_id.as_str()) {
                bail!(
                    "request agent id does not match agent run {run_id}; expected {expected_agent_id}"
                );
            }
            knowledge_job_id = run.knowledge_job_id;
            trigger_id = run.trigger_id;
            run_kind = Some(run.kind);
            run_status = Some(run.status);
            launch_contract = self
                .repository
                .contract_in(transaction, project_id, run_id)
                .await?;
            if run.purpose.is_some() && launch_contract.is_none() {
                bail!("post-049 agent run {run_id} is missing its launch contract");
            }
            if let Some(purpose) = run.purpose {
                if launch_contract
                    .as_ref()
                    .is_none_or(|contract| contract.purpose != purpose)
                {
                    bail!("agent run {run_id} purpose disagrees with its launch contract");
                }
                let kind_matches_purpose = match purpose {
                    AgentRunPurposeV1::KnowledgeAnswer => {
                        run_kind == Some(AgentRunKind::KnowledgeAnswer)
                    }
                    AgentRunPurposeV1::Ordinary | AgentRunPurposeV1::KnowledgeCycle => {
                        run_kind == Some(AgentRunKind::Task)
                    }
                };
                if !kind_matches_purpose {
                    bail!("agent run {run_id} kind disagrees with its launch purpose");
                }
            }
            trigger_revision_id = run.trigger_revision_id;
            trigger_name = run.trigger_name;
            if let Some(id) = trigger_id {
                bundle_key = self.repository.bundle_key_in(transaction, id).await?;
            }
        }

        let attribution = RequestAttribution {
            agent_id,
            agent_run_id,
            run_kind,
            run_status,
            trigger_id,
            trigger_revision_id,
            trigger_name,
            bundle_key,
            launch_contract,
        };
        if knowledge_read
            && attribution.agent_run_id.is_some()
            && attribution.run_status != Some(AgentRunStatus::Running)
        {
            bail!("knowledge run context is no longer active");
        }
        if (attribution.run_kind == Some(AgentRunKind::KnowledgeAnswer)
            || attribution
                .launch_contract
                .as_ref()
                .is_some_and(|contract| contract.purpose != AgentRunPurposeV1::Ordinary))
            && (!knowledge_read || knowledge_job_id.is_none())
        {
            bail!(
                "legacy knowledge runs are retired; active knowledge jobs cannot issue item requests"
            );
        }
        Ok(attribution)
    }
}
