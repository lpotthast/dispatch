use super::repository::RevisionQueryRepository;
use crate::backend::{
    automation::rules::repository::RuleRepository, projects::repository::ProjectRepository,
    storage::TransactionManager,
};
use crate::shared::page_data::AutomationRuleInspectorView;
use dispatch_types::{AutomationEvaluationView, RevisionAnalyticsView};
use rootcause::Result;
use std::sync::Arc;
pub(crate) struct RevisionQueryService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    rules: Arc<RuleRepository>,
    repository: Arc<RevisionQueryRepository>,
}
impl RevisionQueryService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        rules: Arc<RuleRepository>,
        repository: Arc<RevisionQueryRepository>,
    ) -> Self {
        Self {
            transactions,
            projects,
            rules,
            repository,
        }
    }
    pub(crate) async fn evaluations(
        &self,
        project: &str,
        trigger_id: Option<i64>,
        limit: u64,
    ) -> Result<Vec<AutomationEvaluationView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self
            .repository
            .evaluations_in(&transaction, project_id, trigger_id, limit)
            .await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn analytics(
        &self,
        project: &str,
        revision_id: i64,
    ) -> Result<RevisionAnalyticsView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let result = self
            .repository
            .analytics_in(&transaction, project_id, revision_id)
            .await?;
        transaction.commit().await?;
        Ok(result)
    }
    pub(crate) async fn inspect_rule(
        &self,
        project: &str,
        id: i64,
    ) -> Result<AutomationRuleInspectorView> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project).await?;
        let trigger = self.rules.get_in(&transaction, project_id, id).await?;
        let revisions = self
            .rules
            .revisions_in(&transaction, project_id, id)
            .await?;
        let evaluations = self
            .repository
            .evaluations_in(&transaction, project_id, Some(id), 100)
            .await?;
        let current_revision_analytics = match trigger.current_revision_id {
            Some(revision_id) => Some(
                self.repository
                    .analytics_in(&transaction, project_id, revision_id)
                    .await?,
            ),
            None => None,
        };
        transaction.commit().await?;
        Ok(AutomationRuleInspectorView {
            trigger,
            revisions,
            evaluations,
            current_revision_analytics,
        })
    }
}
#[cfg(test)]
pub(crate) fn service(store: &crate::backend::storage::Store) -> RevisionQueryService {
    RevisionQueryService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(ProjectRepository::new(store.db())),
        Arc::new(RuleRepository),
        Arc::new(RevisionQueryRepository),
    )
}
