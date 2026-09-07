use super::repository::ProductionRepository;
use crate::backend::{
    events::UiEventBus,
    items::{
        CreateWorkItem,
        creation::{model::InsertWorkItemOrigin, service::ItemCreationService},
        repository::ItemRepository,
    },
    projects::repository::ProjectRepository,
    storage::{Transaction, TransactionManager},
};
use dispatch_types::{
    AutomationEvaluationOutcome, AutomationTriggerView, ProduceDeduplication, ProducedWorkSpec,
    WorkItemOriginKind, WorkItemView,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Owns the producer's deduplication check and item/evaluation transaction.
pub(crate) struct ProductionService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    items: Arc<ItemRepository>,
    creation: Arc<ItemCreationService>,
    repository: Arc<ProductionRepository>,
    events: UiEventBus,
    coordination: Mutex<()>,
}
impl ProductionService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        items: Arc<ItemRepository>,
        creation: Arc<ItemCreationService>,
        repository: Arc<ProductionRepository>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            items,
            creation,
            repository,
            events,
            coordination: Mutex::new(()),
        }
    }
    pub(crate) async fn acquire(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.coordination.lock().await
    }
    pub(crate) async fn produce(
        &self,
        project_name: &str,
        automation: &AutomationTriggerView,
    ) -> Result<WorkItemView> {
        let _permit = self.acquire().await;
        let tx = self.transactions.begin().await?;
        let (item, created) = self.produce_in(&tx, project_name, automation).await?;
        tx.commit().await?;
        if created {
            self.events.publish_work_item_changed(project_name, item.id);
        }
        Ok(item)
    }
    pub(crate) async fn produce_in(
        &self,
        transaction: &Transaction,
        project_name: &str,
        automation: &AutomationTriggerView,
    ) -> Result<(WorkItemView, bool)> {
        let spec = automation
            .produced_work
            .clone()
            .unwrap_or(ProducedWorkSpec {
                title: None,
                state: dispatch_types::DEFAULT_STATE_LABEL.to_owned(),
                initial_labels: vec![],
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                deduplication: ProduceDeduplication::Always,
            });
        crate::backend::automation::rules::policy::validate_produced_work_spec(&spec)?;
        let project_id = self.projects.id_in(transaction, project_name).await?;
        if project_id != automation.project_id {
            bail!("producing automation does not belong to this project");
        }
        self.repository
            .ensure_rule_in(transaction, automation)
            .await?;
        if let Some(id) = self
            .repository
            .unfinished_duplicate_in(transaction, project_id, automation.id, &spec.deduplication)
            .await?
        {
            self.repository
                .record_in(
                    transaction,
                    automation,
                    AutomationEvaluationOutcome::SkippedDuplicate,
                    Some(id),
                )
                .await?;
            let item = self.items.get_in(transaction, project_id, id).await?;
            return Ok((item, false));
        }
        let evaluation_id = self
            .repository
            .record_in(
                transaction,
                automation,
                AutomationEvaluationOutcome::CreatedWork,
                None,
            )
            .await?;
        let item = self
            .creation
            .create_in(
                transaction,
                project_id,
                CreateWorkItem {
                    title: spec.title.unwrap_or_else(|| automation.name.clone()),
                    description: automation.prompt.clone(),
                    state: spec.state,
                    agent_model_override: spec.agent_model_override,
                    agent_reasoning_effort_override: spec.agent_reasoning_effort_override,
                    initial_labels: spec.initial_labels,
                },
                InsertWorkItemOrigin {
                    kind: WorkItemOriginKind::ProducingAutomation,
                    producing_evaluation_id: Some(evaluation_id),
                    trigger_id: Some(automation.id),
                    trigger_revision_id: automation.current_revision_id,
                    trigger_name: Some(automation.name.clone()),
                    bundle_key: automation.managed_bundle_key.clone(),
                    deduplication_key: match spec.deduplication {
                        ProduceDeduplication::WhileUnfinishedForKey { key } => Some(key),
                        _ => None,
                    },
                    ..Default::default()
                },
                Default::default(),
            )
            .await?;
        self.repository
            .attach_item_in(transaction, evaluation_id, item.id)
            .await?;
        Ok((item, true))
    }
}
