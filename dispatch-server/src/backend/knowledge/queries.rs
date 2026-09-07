//! Query composition records independent-reader cost after deterministic document reads.
use super::{jobs::service::JobService, service::KnowledgeService};
use crate::backend::attribution::model::AttributionInput;
use dispatch_types::knowledge::{KnowledgeOperation, KnowledgeQuery, KnowledgeView};
use rootcause::Result;
use std::sync::Arc;
pub(crate) struct KnowledgeQueryService {
    documents: Arc<KnowledgeService>,
    jobs: Arc<JobService>,
}
impl KnowledgeQueryService {
    pub(crate) fn new(documents: Arc<KnowledgeService>, jobs: Arc<JobService>) -> Self {
        Self { documents, jobs }
    }
    pub(crate) async fn query(
        &self,
        project: &str,
        input: AttributionInput,
        operation: KnowledgeOperation,
        query: KnowledgeQuery,
    ) -> Result<KnowledgeView> {
        let result = self
            .documents
            .query(project, input.clone(), operation, query)
            .await?;
        if input.agent_run_id.is_some() {
            self.jobs
                .record_read(project, input, &serde_json::to_vec(&result)?)
                .await?;
        }
        Ok(result)
    }
}
