use axum::http::HeaderMap;
use rootcause::{Result, prelude::*};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

use crate::{
    backend::{
        agent_ids,
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
    trigger_id: Option<i64>,
    trigger_revision_id: Option<i64>,
    trigger_name: Option<String>,
    bundle_key: Option<String>,
}

impl RequestAttribution {
    pub(crate) async fn from_headers(
        store: &Store,
        project_name: &str,
        headers: &HeaderMap,
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
        if let Some(agent_id) = &agent_id {
            agent_ids::validate_agent_id(agent_id)?;
        }
        let mut trigger_id = None;
        let mut trigger_revision_id = None;
        let mut trigger_name = None;
        let mut bundle_key = None;
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
            trigger_id = run.trigger_id;
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

        Ok(Self {
            agent_id,
            agent_run_id,
            trigger_id,
            trigger_revision_id,
            trigger_name,
            bundle_key,
        })
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

    pub(crate) fn cross_check_agent_run_id(&self, body_agent_run_id: Option<i64>) -> Result<()> {
        if let Some(header_run_id) = self.agent_run_id
            && body_agent_run_id != Some(header_run_id)
        {
            bail!(
                "request agent run id {header_run_id} does not match body agent run id {:?}",
                body_agent_run_id
            );
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
    use axum::http::HeaderValue;
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    use tempfile::TempDir;

    use super::*;
    use crate::backend::{
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
        assert!(operator.agent_id.is_none());

        let missing_agent =
            RequestAttribution::from_headers(&store, "demo", &headers(None, Some(run_id)))
                .await
                .unwrap_err();
        assert!(missing_agent.to_string().contains("requires"));

        let wrong_agent = RequestAttribution::from_headers(
            &store,
            "demo",
            &headers(Some("agent-wrong"), Some(run_id)),
        )
        .await
        .unwrap_err();
        assert!(wrong_agent.to_string().contains("does not match"));

        let cross_project = RequestAttribution::from_headers(
            &store,
            "other",
            &headers(Some(&agent_id), Some(run_id)),
        )
        .await
        .unwrap_err();
        assert!(
            cross_project
                .to_string()
                .contains("does not exist in this project")
        );

        let attribution = RequestAttribution::from_headers(
            &store,
            "demo",
            &headers(Some(&agent_id), Some(run_id)),
        )
        .await
        .unwrap();
        assert_eq!(attribution.agent_id.as_deref(), Some(agent_id.as_str()));
        assert_eq!(attribution.agent_run_id, Some(run_id));
        assert_eq!(attribution.item_origin().kind, WorkItemOriginKind::AgentRun);
        assert_eq!(attribution.item_origin().agent_run_id, Some(run_id));
        attribution.cross_check_agent_id(&agent_id).unwrap();
        attribution.cross_check_agent_run_id(Some(run_id)).unwrap();
        assert!(attribution.cross_check_agent_id("agent-other").is_err());
        assert!(attribution.cross_check_agent_run_id(None).is_err());
    }
}
