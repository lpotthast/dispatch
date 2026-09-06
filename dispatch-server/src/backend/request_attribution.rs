use axum::http::HeaderMap;
use dispatch_types::{AgentRunKind, AgentRunPurposeV1, AgentRunStatus};
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    backend::{
        agent_ids,
        agent_run_launch::{
            AgentLaunchResolutionV1, AgentLaunchTargetV1, AgentRunLaunchRepository,
            PersistedLaunchContract,
        },
        entities::{
            agent_run::{self, AgentRun},
            automation_trigger::AutomationTrigger,
        },
        projects,
        storage::Store,
        work_item_creation::InsertWorkItemOrigin,
        work_item_events::EventAttribution,
    },
    shared::view_models::{AuthorType, WorkItemOriginKind},
};

pub(crate) const AGENT_ID_HEADER: &str = "x-dispatch-agent-id";
pub(crate) const AGENT_RUN_ID_HEADER: &str = "x-dispatch-agent-run-id";

#[derive(Clone, Debug, Default)]
pub(crate) struct RequestAttribution {
    pub(crate) agent_id: Option<String>,
    pub(crate) agent_run_id: Option<i64>,
    run_kind: Option<AgentRunKind>,
    run_status: Option<AgentRunStatus>,
    trigger_id: Option<i64>,
    trigger_revision_id: Option<i64>,
    trigger_name: Option<String>,
    bundle_key: Option<String>,
    launch_contract: Option<PersistedLaunchContract>,
}

impl RequestAttribution {
    pub(crate) async fn from_headers(
        store: &Store,
        project_name: &str,
        headers: &HeaderMap,
    ) -> Result<Self> {
        Self::from_headers_inner(store, project_name, headers, false).await
    }

    pub(crate) async fn from_knowledge_headers(
        store: &Store,
        project_name: &str,
        headers: &HeaderMap,
    ) -> Result<Self> {
        Self::from_headers_inner(store, project_name, headers, true).await
    }

    async fn from_headers_inner(
        store: &Store,
        project_name: &str,
        headers: &HeaderMap,
        knowledge_read: bool,
    ) -> Result<Self> {
        let agent_id = header_value(headers, AGENT_ID_HEADER)?;

        let agent_run_id = header_value(headers, AGENT_RUN_ID_HEADER)?
            .map(|value| {
                value
                    .parse::<i64>()
                    .context_with(|| format!("invalid {AGENT_RUN_ID_HEADER} '{value}'"))
            })
            .transpose()?;

        if agent_run_id.is_some() && agent_id.is_none() {
            bail!("{AGENT_RUN_ID_HEADER} requires {AGENT_ID_HEADER}");
        }
        if knowledge_read && agent_id.is_some() && agent_run_id.is_none() {
            bail!("knowledge agent attribution requires {AGENT_RUN_ID_HEADER}");
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
            let project_id = projects::project_id(store, project_name).await?;
            let run = AgentRun::find_by_id(run_id)
                .filter(agent_run::Column::ProjectId.eq(project_id))
                .one(store.db().as_ref())
                .await
                .context("failed to validate request agent run")?
                .ok_or_else(|| report!("agent run {run_id} does not exist in this project"))?;
            let expected_agent_id = agent_ids::dispatch_run_agent_id(run_id);
            if agent_id.as_deref() != Some(expected_agent_id.as_str()) {
                bail!(
                    "request agent id does not match agent run {run_id}; expected {expected_agent_id}"
                );
            }
            knowledge_job_id = run.knowledge_job_id;
            trigger_id = run.trigger_id;
            run_kind = Some(
                run.run_kind
                    .parse()
                    .context("invalid persisted agent run kind")?,
            );
            run_status = Some(
                run.status
                    .parse()
                    .context("invalid persisted agent run status")?,
            );
            launch_contract = AgentRunLaunchRepository::new(store)
                .load(project_id, run_id)
                .await?;
            if run.purpose.is_some() && launch_contract.is_none() {
                bail!("post-049 agent run {run_id} is missing its launch contract");
            }
            if let Some(purpose) = run.purpose {
                let purpose: AgentRunPurposeV1 = purpose
                    .parse()
                    .context("invalid persisted agent run purpose")?;
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
                bundle_key = AutomationTrigger::find_by_id(id)
                    .one(store.db().as_ref())
                    .await
                    .context("failed to load request automation origin")?
                    .and_then(|trigger| trigger.managed_bundle_key);
            }
        }

        let attribution = Self {
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

fn header_value(headers: &HeaderMap, name: &str) -> Result<Option<String>> {
    headers
        .get(name)
        .map(|value| {
            let value = value
                .to_str()
                .context_with(|| format!("invalid {name} header"))?
                .trim();
            if value.is_empty() {
                bail!("{name} header cannot be empty");
            }
            Ok(value.to_owned())
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use axum::http::HeaderValue;
    use dispatch_types::AutomationRunMutability;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, TransactionTrait};
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
        agent_run_launch::{
            AgentCapabilitySetV1, AgentLaunchTargetV1, insert_contract_in_tx, mark_spawned_in_tx,
            mark_terminal_in_tx,
        },
        entities::agent_run::AgentRunActiveModel,
        projects::{CreateProject, create_project},
        storage::utc_now,
    };

    async fn test_store() -> (TempDir, Store) {
        let temp = TempDir::new().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        for name in ["demo", "other"] {
            create_project(
                &store,
                CreateProject {
                    name: name.to_owned(),
                    display_name: None,
                    path: temp.path().to_path_buf(),
                    default_agent_model: None,
                    default_agent_reasoning_effort: None,
                    system_prompt: None,
                    memory: None,
                },
            )
            .await
            .unwrap();
        }
        (temp, store)
    }

    async fn insert_run(store: &Store, project_name: &str) -> i64 {
        let project_id = projects::project_id(store, project_name).await.unwrap();
        let now = utc_now();
        AgentRunActiveModel {
            project_id: Set(project_id),
            trigger_name: Set(Some("lineage-rule".to_owned())),
            tool_name: Set("codex".to_owned()),
            mutability: Set("read_only".to_owned()),
            status: Set("running".to_owned()),
            command: Set(String::new()),
            working_dir: Set(String::new()),
            created_at: Set(now.clone()),
            updated_at: Set(now),
            ..Default::default()
        }
        .insert(store.db().as_ref())
        .await
        .unwrap()
        .id
    }

    async fn insert_contracted_run(
        store: &Store,
        project_name: &str,
        mutability: AutomationRunMutability,
        status: AgentRunStatus,
        resolved: bool,
    ) -> i64 {
        let project_id = projects::project_id(store, project_name).await.unwrap();
        let now = utc_now();
        let txn = store.db().begin().await.unwrap();
        let run = AgentRunActiveModel {
            project_id: Set(project_id),
            run_kind: Set(AgentRunKind::Task.as_storage().to_owned()),
            purpose: Set(Some(AgentRunPurposeV1::Ordinary.as_storage().to_owned())),
            tool_name: Set("codex".to_owned()),
            mutability: Set(mutability.as_storage().to_owned()),
            status: Set(status.as_storage().to_owned()),
            command: Set(String::new()),
            working_dir: Set(String::new()),
            created_at: Set(now.clone()),
            updated_at: Set(now.clone()),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .unwrap();
        let target = if resolved {
            AgentLaunchTargetV1::none()
        } else {
            AgentLaunchTargetV1::next_open("open").unwrap()
        };
        insert_contract_in_tx(
            &txn,
            project_id,
            run.id,
            AgentRunPurposeV1::Ordinary,
            &target,
            &AgentCapabilitySetV1::ordinary(),
            &now,
        )
        .await
        .unwrap();
        if resolved {
            mark_spawned_in_tx(&txn, project_id, run.id, &now)
                .await
                .unwrap();
            if status != AgentRunStatus::Running {
                mark_terminal_in_tx(&txn, project_id, run.id, &now)
                    .await
                    .unwrap();
            }
        }
        txn.commit().await.unwrap();
        run.id
    }

    fn headers(agent_id: Option<&str>, run_id: Option<i64>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(agent_id) = agent_id {
            headers.insert(AGENT_ID_HEADER, HeaderValue::from_str(agent_id).unwrap());
        }
        if let Some(run_id) = run_id {
            headers.insert(
                AGENT_RUN_ID_HEADER,
                HeaderValue::from_str(&run_id.to_string()).unwrap(),
            );
        }
        headers
    }

    #[tokio::test]
    async fn request_attribution_validates_run_project_and_derived_agent() {
        let (_temp, store) = test_store().await;
        let run_id = insert_run(&store, "demo").await;
        let agent_id = agent_ids::dispatch_run_agent_id(run_id);

        let operator = RequestAttribution::from_headers(&store, "demo", &HeaderMap::new())
            .await
            .unwrap();
        assert_that!(&(operator.agent_id.is_none())).is_true();

        let missing_agent =
            RequestAttribution::from_headers(&store, "demo", &headers(None, Some(run_id)))
                .await
                .unwrap_err();
        assert_that!(&(missing_agent.to_string().contains("requires"))).is_true();

        let wrong_agent = RequestAttribution::from_headers(
            &store,
            "demo",
            &headers(Some("agent-wrong"), Some(run_id)),
        )
        .await
        .unwrap_err();
        assert_that!(&(wrong_agent.to_string().contains("does not match"))).is_true();

        let cross_project = RequestAttribution::from_headers(
            &store,
            "other",
            &headers(Some(&agent_id), Some(run_id)),
        )
        .await
        .unwrap_err();
        assert_that!(
            &(cross_project
                .to_string()
                .contains("does not exist in this project"))
        )
        .is_true();

        let attribution = RequestAttribution::from_headers(
            &store,
            "demo",
            &headers(Some(&agent_id), Some(run_id)),
        )
        .await
        .unwrap();
        assert_that!(&(attribution.agent_id.as_deref())).is_equal_to(Some(agent_id.as_str()));
        assert_that!(&(attribution.agent_run_id)).is_equal_to(Some(run_id));
        assert_that!(&(attribution.item_origin().kind)).is_equal_to(WorkItemOriginKind::AgentRun);
        assert_that!(&(attribution.item_origin().agent_run_id)).is_equal_to(Some(run_id));
        attribution.cross_check_agent_id(&agent_id).unwrap();
        assert_that!(&(attribution.cross_check_agent_id("agent-other").is_err())).is_true();
    }

    #[tokio::test]
    async fn knowledge_agent_headers_require_a_project_scoped_persisted_run() {
        let (_temp, store) = test_store().await;
        let arbitrary = RequestAttribution::from_knowledge_headers(
            &store,
            "demo",
            &headers(Some("agent-arbitrary"), None),
        )
        .await
        .unwrap_err();
        assert_that!(&arbitrary.to_string()).contains(AGENT_RUN_ID_HEADER);

        let run_id = insert_contracted_run(
            &store,
            "demo",
            AutomationRunMutability::Mutating,
            AgentRunStatus::Running,
            true,
        )
        .await;
        let cross_project = RequestAttribution::from_knowledge_headers(
            &store,
            "other",
            &headers(
                Some(&agent_ids::dispatch_run_agent_id(run_id)),
                Some(run_id),
            ),
        )
        .await
        .unwrap_err();
        assert_that!(&cross_project.to_string()).contains("does not exist in this project");
    }
}
