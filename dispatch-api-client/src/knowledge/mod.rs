use crate::{ClientResult, ProjectClient, endpoint::Endpoint};
use dispatch_types::knowledge::{KnowledgeOperation, KnowledgeQuery, KnowledgeView};

impl ProjectClient<'_> {
    /// Read the server's current registered working copy without invoking a model.
    pub async fn knowledge_query(
        &self,
        operation: KnowledgeOperation,
        query: &KnowledgeQuery,
    ) -> ClientResult<KnowledgeView> {
        let operation = match operation {
            KnowledgeOperation::Root => "root",
            KnowledgeOperation::Node => "node",
            KnowledgeOperation::Search => "search",
            KnowledgeOperation::Check => "check",
            KnowledgeOperation::List => "documents",
            KnowledgeOperation::Graph => "graph",
        };
        let endpoint = Endpoint::project(self.project, "knowledge").segment(operation);
        self.client
            .send(
                self.client
                    .http
                    .get(self.client.url(endpoint.as_ref()))
                    .query(query),
            )
            .await
    }
}
