use rootcause::{Result, prelude::*};
use sea_orm::{ActiveModelTrait, ActiveValue::Set, ConnectionTrait};

use crate::{
    backend::{
        entities::{
            work_item::{WorkItemActiveModel, WorkItemModel},
            work_item_origin::WorkItemOriginActiveModel,
        },
        item_labels, projects, work_item_events, work_item_labels, work_item_updates,
        workflow_labels,
    },
    shared::view_models::{
        AgentReasoningEffort, CreateWorkItemLabelRequest, WorkItemEventType, WorkItemOriginKind,
    },
};

#[derive(Clone, Debug)]
pub(crate) struct InsertWorkItemOrigin {
    pub(crate) kind: WorkItemOriginKind,
    pub(crate) actor_id: Option<String>,
    pub(crate) agent_run_id: Option<i64>,
    pub(crate) producing_evaluation_id: Option<i64>,
    pub(crate) trigger_id: Option<i64>,
    pub(crate) trigger_revision_id: Option<i64>,
    pub(crate) trigger_name: Option<String>,
    pub(crate) bundle_key: Option<String>,
    pub(crate) deduplication_key: Option<String>,
}

impl Default for InsertWorkItemOrigin {
    fn default() -> Self {
        Self {
            kind: WorkItemOriginKind::Operator,
            actor_id: None,
            agent_run_id: None,
            producing_evaluation_id: None,
            trigger_id: None,
            trigger_revision_id: None,
            trigger_name: None,
            bundle_key: None,
            deduplication_key: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CreateWorkItem {
    pub title: String,
    pub description: String,
    pub state: String,
    pub agent_model_override: Option<String>,
    pub agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    pub initial_labels: Vec<CreateWorkItemLabelRequest>,
}

#[derive(Debug)]
pub(crate) struct CreateWorkItemPlan {
    title: String,
    description: String,
    state_label: String,
    agent_model_override: Option<String>,
    agent_reasoning_effort_override: Option<AgentReasoningEffort>,
    initial_labels: Vec<item_labels::NormalizedLabel>,
}

pub(crate) struct PlannedWorkItemInsert {
    pub(crate) active: WorkItemActiveModel,
    pub(crate) state_label: String,
    pub(crate) initial_labels: Vec<item_labels::NormalizedLabel>,
}

impl CreateWorkItemPlan {
    pub(crate) fn new(create: CreateWorkItem) -> Result<Self> {
        work_item_updates::validate_item_text(&create.title, &create.description)?;
        let state_label = workflow_labels::normalize_state_value(create.state)?;
        let agent_model_override = projects::normalize_optional(create.agent_model_override);
        projects::validate_agent_model_field(
            "agent model override",
            agent_model_override.as_deref(),
        )?;
        let initial_labels = item_labels::normalize_initial_labels(
            create
                .initial_labels
                .into_iter()
                .map(|label| (label.key, label.value)),
        )
        .context("invalid initial labels")?;

        Ok(Self {
            title: create.title,
            description: create.description,
            state_label,
            agent_model_override,
            agent_reasoning_effort_override: create.agent_reasoning_effort_override,
            initial_labels,
        })
    }

    pub(crate) fn agent_model_override(&self) -> Option<&str> {
        self.agent_model_override.as_deref()
    }

    pub(crate) fn agent_reasoning_effort_override(&self) -> Option<AgentReasoningEffort> {
        self.agent_reasoning_effort_override
    }

    pub(crate) fn into_insert(self, project_id: i64, created_at: String) -> PlannedWorkItemInsert {
        let active = WorkItemActiveModel {
            project_id: Set(project_id),
            title: Set(self.title),
            description: Set(self.description),
            agent_model_override: Set(self.agent_model_override),
            agent_reasoning_effort_override: Set(self
                .agent_reasoning_effort_override
                .map(|effort| effort.as_storage().to_owned())),
            version: Set(1),
            created_at: Set(created_at.clone()),
            updated_at: Set(created_at),
            ..Default::default()
        };
        PlannedWorkItemInsert {
            active,
            state_label: self.state_label,
            initial_labels: self.initial_labels,
        }
    }
}

pub(crate) async fn insert_planned_in_tx<C>(
    conn: &C,
    project_id: i64,
    plan: CreateWorkItemPlan,
    created_at: String,
) -> Result<WorkItemModel>
where
    C: ConnectionTrait,
{
    insert_planned_with_origin_in_tx(
        conn,
        project_id,
        plan,
        created_at,
        InsertWorkItemOrigin::default(),
        work_item_events::EventAttribution::default(),
    )
    .await
}

pub(crate) async fn insert_planned_with_origin_in_tx<C>(
    conn: &C,
    project_id: i64,
    plan: CreateWorkItemPlan,
    created_at: String,
    origin: InsertWorkItemOrigin,
    attribution: work_item_events::EventAttribution<'_>,
) -> Result<WorkItemModel>
where
    C: ConnectionTrait,
{
    let insert = plan.into_insert(project_id, created_at);
    let item = insert
        .active
        .insert(conn)
        .await
        .context("failed to create work item")?;
    workflow_labels::apply_plan_in_tx(
        conn,
        project_id,
        item.id,
        workflow_labels::state_workflow_label_plan(&insert.state_label),
    )
    .await?;
    for label in &insert.initial_labels {
        work_item_labels::insert_in_tx(
            conn,
            project_id,
            item.id,
            &label.key,
            label.value.as_deref(),
        )
        .await?;
    }
    WorkItemOriginActiveModel {
        work_item_id: Set(item.id),
        project_id: Set(project_id),
        origin_kind: Set(origin.kind.as_storage().to_owned()),
        actor_id: Set(origin.actor_id),
        agent_run_id: Set(origin.agent_run_id),
        producing_evaluation_id: Set(origin.producing_evaluation_id),
        trigger_id: Set(origin.trigger_id),
        trigger_revision_id: Set(origin.trigger_revision_id),
        trigger_name: Set(origin.trigger_name),
        bundle_key: Set(origin.bundle_key),
        deduplication_key: Set(origin.deduplication_key),
        created_at: Set(item.created_at.clone()),
    }
    .insert(conn)
    .await
    .context("failed to create work item origin")?;
    work_item_events::record_event_with_attribution_in_tx(
        conn,
        project_id,
        Some(item.id),
        WorkItemEventType::ItemCreated,
        "Created item",
        attribution,
    )
    .await?;

    Ok(item)
}

#[cfg(test)]
mod tests {
    use sea_orm::ActiveValue::Set;

    use super::*;
    use crate::shared::view_models::STATE_LABEL_KEY;

    fn create_work_item() -> CreateWorkItem {
        CreateWorkItem {
            title: "Create me".to_owned(),
            description: "Exercise create planning".to_owned(),
            state: " open ".to_owned(),
            agent_model_override: Some("  ".to_owned()),
            agent_reasoning_effort_override: Some(AgentReasoningEffort::Medium),
            initial_labels: vec![
                CreateWorkItemLabelRequest {
                    key: " priority ".to_owned(),
                    value: Some(" high ".to_owned()),
                },
                CreateWorkItemLabelRequest {
                    key: "needs-verification".to_owned(),
                    value: Some("  ".to_owned()),
                },
            ],
        }
    }

    #[test]
    fn create_plan_normalizes_state_overrides_and_initial_labels() {
        let plan = CreateWorkItemPlan::new(create_work_item()).unwrap();
        let insert = plan.into_insert(42, "2026-06-19T00:00:00Z".to_owned());

        assert_eq!(insert.state_label, "open");
        assert_eq!(
            insert.initial_labels,
            vec![
                item_labels::NormalizedLabel {
                    key: "priority".to_owned(),
                    value: Some("high".to_owned()),
                },
                item_labels::NormalizedLabel {
                    key: "needs-verification".to_owned(),
                    value: None,
                },
            ]
        );

        let active = insert.active;
        assert_eq!(active.project_id, Set(42));
        assert_eq!(active.title, Set("Create me".to_owned()));
        assert_eq!(active.agent_model_override, Set(None));
        assert_eq!(
            active.agent_reasoning_effort_override,
            Set(Some("medium".to_owned()))
        );
        assert_eq!(active.version, Set(1));
    }

    #[test]
    fn create_plan_rejects_invalid_text_state_and_labels() {
        let err = CreateWorkItemPlan::new(CreateWorkItem {
            title: " ".to_owned(),
            ..create_work_item()
        })
        .unwrap_err();
        assert!(err.to_string().contains("item title cannot be empty"));

        let err = CreateWorkItemPlan::new(CreateWorkItem {
            state: " ".to_owned(),
            ..create_work_item()
        })
        .unwrap_err();
        assert!(
            err.to_string()
                .contains("state label value cannot be empty")
        );

        let err = CreateWorkItemPlan::new(CreateWorkItem {
            initial_labels: vec![CreateWorkItemLabelRequest {
                key: STATE_LABEL_KEY.to_owned(),
                value: Some("review".to_owned()),
            }],
            ..create_work_item()
        })
        .unwrap_err();
        assert!(err.to_string().contains("invalid initial labels"));
        assert!(err.to_string().contains("use the state selector"));
    }

    #[test]
    fn create_plan_rejects_unknown_model_override() {
        let err = CreateWorkItemPlan::new(CreateWorkItem {
            agent_model_override: Some("gpt-4.1-codex".to_owned()),
            ..create_work_item()
        })
        .unwrap_err();

        assert!(
            err.to_string()
                .contains("agent model override must be one of")
        );
    }
}
