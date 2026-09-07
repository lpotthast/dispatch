use super::{
    policy::normalize_knowledge_directory, repository::KnowledgeRepository, runtime::KnowledgeFiles,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    projects::repository::ProjectRepository,
    storage::TransactionManager,
};
use dispatch_types::knowledge::*;
use rootcause::{Result, prelude::*};
use std::sync::Arc;
pub(crate) struct KnowledgeService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<KnowledgeRepository>,
    files: Arc<KnowledgeFiles>,
    attribution: Arc<AttributionService>,
}
impl KnowledgeService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<KnowledgeRepository>,
        files: Arc<KnowledgeFiles>,
        attribution: Arc<AttributionService>,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            files,
            attribution,
        }
    }
    pub(crate) async fn query(
        &self,
        project: &str,
        input: AttributionInput,
        operation: KnowledgeOperation,
        query: KnowledgeQuery,
    ) -> Result<KnowledgeView> {
        let tx = self.transactions.begin().await?;
        let project_name = project.to_owned();
        let project = self.projects.by_name_in(&tx, project).await?;
        let attribution = self
            .attribution
            .validate_in(&tx, &project_name, input, true)
            .await?;
        let working = match attribution.agent_run_id {
            Some(run_id) => {
                self.repository
                    .working_directory_in(&tx, project.id, run_id)
                    .await?
            }
            None => project
                .path
                .ok_or_else(|| report!("project has no working directory"))?,
        };
        let directory = normalize_knowledge_directory(&project.knowledge_directory)?;
        tx.commit().await?;
        let result = self
            .files
            .query(project.id, working, directory, operation, query)
            .await?;
        Ok(result)
    }
    pub(crate) async fn save(
        &self,
        project: &str,
        request: KnowledgeSaveRequest,
    ) -> Result<KnowledgeSaveResult> {
        let project = self.projects.by_name(project).await?;
        let working = project
            .path
            .ok_or_else(|| report!("project has no working directory"))?;
        self.files
            .save(working, project.knowledge_directory, request)
            .await
    }
}
