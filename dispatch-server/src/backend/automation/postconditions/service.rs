use super::{model::SemanticEvaluation, policy, repository::PostconditionRepository};
use crate::backend::{
    items::repository::ItemRepository, projects::repository::ProjectRepository,
    storage::TransactionManager,
};
use dispatch_types::{
    AgentCommitOutcome, AutomationPostconditions, SemanticPostconditionStatus, WorkItemView,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
pub(crate) struct PostconditionService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<PostconditionRepository>,
    items: Arc<ItemRepository>,
}
impl PostconditionService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<PostconditionRepository>,
        items: Arc<ItemRepository>,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            items,
        }
    }
    pub(crate) async fn evaluate(
        &self,
        project: &str,
        run_id: i64,
        baseline_item: Option<&WorkItemView>,
        postconditions: Option<&AutomationPostconditions>,
        commit_outcome: AgentCommitOutcome,
    ) -> Result<SemanticEvaluation> {
        let Some(postconditions) = postconditions else {
            return Ok(SemanticEvaluation {
                status: SemanticPostconditionStatus::NotConfigured,
                failures: Vec::new(),
            });
        };
        if postconditions.any_of.is_empty() {
            bail!("automation postconditions require at least one outcome set");
        }
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        self.repository
            .require_run_in(&transaction, project_id, run_id)
            .await?;
        let current = match baseline_item {
            Some(item) => Some(self.items.get_in(&transaction, project_id, item.id).await?),
            None => None,
        };
        let events = self
            .repository
            .events_in(&transaction, project_id, run_id)
            .await?;
        let ids = self
            .repository
            .created_item_ids_in(&transaction, project_id, run_id)
            .await?;
        let mut created_items = Vec::with_capacity(ids.len());
        for id in ids {
            created_items.push(self.items.get_in(&transaction, project_id, id).await?);
        }
        transaction.commit().await?;
        policy::evaluate(
            baseline_item,
            current.as_ref(),
            &events,
            &created_items,
            postconditions,
            commit_outcome,
        )
    }
}
