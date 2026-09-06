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

use dispatch_types::knowledge::jobs::*;
impl ProjectClient<'_> {
    pub async fn knowledge_settings(&self) -> ClientResult<KnowledgeSettings> {
        let e = Endpoint::project(self.project, "knowledge").segment("job-settings");
        self.client
            .send(self.client.http.get(self.client.url(e.as_ref())))
            .await
    }
    pub async fn save_knowledge_settings(
        &self,
        settings: &KnowledgeSettings,
    ) -> ClientResult<KnowledgeSettings> {
        let e = Endpoint::project(self.project, "knowledge").segment("job-settings");
        self.client
            .send(
                self.client
                    .http
                    .post(self.client.url(e.as_ref()))
                    .json(settings),
            )
            .await
    }
    pub async fn knowledge_jobs(&self) -> ClientResult<Vec<KnowledgeJob>> {
        let e = Endpoint::project(self.project, "knowledge").segment("jobs");
        self.client
            .send(self.client.http.get(self.client.url(e.as_ref())))
            .await
    }
    pub async fn start_knowledge_job(
        &self,
        request: &StartKnowledgeJob,
    ) -> ClientResult<KnowledgeJob> {
        let e = Endpoint::project(self.project, "knowledge").segment("jobs");
        self.client
            .send(
                self.client
                    .http
                    .post(self.client.url(e.as_ref()))
                    .json(request),
            )
            .await
    }
    pub async fn knowledge_job(&self, id: i64) -> ClientResult<KnowledgeJobDetail> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string());
        self.client
            .send(self.client.http.get(self.client.url(e.as_ref())))
            .await
    }
    pub async fn knowledge_job_action(
        &self,
        id: i64,
        action: &JobAction,
    ) -> ClientResult<KnowledgeJob> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string())
            .segment("action");
        self.client
            .send(
                self.client
                    .http
                    .post(self.client.url(e.as_ref()))
                    .json(action),
            )
            .await
    }
    pub async fn knowledge_coverage(
        &self,
        id: i64,
        aspect: Option<&str>,
    ) -> ClientResult<CoverageView> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string())
            .segment("coverage");
        self.client
            .send(
                self.client
                    .http
                    .get(self.client.url(e.as_ref()))
                    .query(&[("aspect", aspect)]),
            )
            .await
    }
    pub async fn knowledge_source(
        &self,
        id: i64,
        operation: &str,
        query: &SourceQuery,
    ) -> ClientResult<SourceResponse> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string())
            .segment("source")
            .segment(operation);
        self.client
            .send(
                self.client
                    .http
                    .get(self.client.url(e.as_ref()))
                    .query(query),
            )
            .await
    }
    pub async fn knowledge_job_progress(
        &self,
        id: i64,
        progress: &JobProgress,
    ) -> ClientResult<()> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string())
            .segment("progress");
        self.client
            .send(
                self.client
                    .http
                    .post(self.client.url(e.as_ref()))
                    .json(progress),
            )
            .await
    }
    pub async fn knowledge_job_report(&self, id: i64, report: &JobReport) -> ClientResult<()> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string())
            .segment("report");
        self.client
            .send(
                self.client
                    .http
                    .post(self.client.url(e.as_ref()))
                    .json(report),
            )
            .await
    }
    pub async fn knowledge_job_assess(
        &self,
        id: i64,
        assessment: &AspectAssessment,
    ) -> ClientResult<()> {
        let e = Endpoint::project(self.project, "knowledge")
            .segment("jobs")
            .segment(id.to_string())
            .segment("assess");
        self.client
            .send(
                self.client
                    .http
                    .post(self.client.url(e.as_ref()))
                    .json(assessment),
            )
            .await
    }
}
