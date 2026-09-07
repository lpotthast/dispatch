use crate::frontend::knowledge::service::KnowledgeUiService;
use crate::frontend::queries::cache::QueryCache;
use dispatch_types::knowledge::{KnowledgeDocument, KnowledgeView, jobs::*};
use leptos::prelude::*;

/// Current-file knowledge is session-local. Every open revalidates with the server:
/// a disk snapshot cannot establish the current generation or ignore policy.
#[derive(Clone)]
pub(crate) struct KnowledgeStore {
    jobs: QueryCache<String, Vec<KnowledgeJob>>,
    settings: QueryCache<String, KnowledgeSettings>,
    details: QueryCache<(String, i64), KnowledgeJobDetail>,
    coverage: QueryCache<(String, i64, Option<String>), CoverageView>,
    service: KnowledgeUiService,
    graphs: QueryCache<String, KnowledgeView>,
    documents: QueryCache<(String, String), KnowledgeDocument>,
}

impl KnowledgeStore {
    pub(crate) fn new(service: KnowledgeUiService) -> Self {
        Self {
            jobs: QueryCache::in_memory(),
            settings: QueryCache::in_memory(),
            details: QueryCache::in_memory(),
            coverage: QueryCache::in_memory(),
            service,
            graphs: QueryCache::in_memory(),
            documents: QueryCache::in_memory(),
        }
    }

    pub(crate) async fn graph(&self, project: String) -> Result<KnowledgeView, ServerFnError> {
        let service = self.service.clone();
        let requested_project = project.clone();
        let graph = self
            .graphs
            .load(project.clone(), move || async move {
                service.graph(requested_project).await
            })
            .await?;
        // Only a current graph may evict cached bodies after an ignore-policy change.
        self.documents.retain_keys(|(name, path)| {
            name != &project
                || graph
                    .documents
                    .iter()
                    .any(|document| &document.path == path)
        });
        Ok(graph)
    }

    pub(crate) async fn document(
        &self,
        project: String,
        path: String,
    ) -> Result<KnowledgeDocument, ServerFnError> {
        let service = self.service.clone();
        self.documents
            .load((project.clone(), path.clone()), move || async move {
                service.document(project, path).await
            })
            .await
    }

    pub(crate) async fn list_jobs(
        &self,
        project: String,
    ) -> Result<Vec<KnowledgeJob>, ServerFnError> {
        let service = self.service.jobs.clone();
        self.jobs
            .load(project.clone(), move || async move {
                service.list(project).await
            })
            .await
    }

    pub(crate) async fn job_settings(
        &self,
        project: String,
    ) -> Result<KnowledgeSettings, ServerFnError> {
        let service = self.service.jobs.clone();
        self.settings
            .load(project.clone(), move || async move {
                service.settings(project).await
            })
            .await
    }

    pub(crate) async fn job_detail(
        &self,
        project: String,
        id: i64,
    ) -> Result<KnowledgeJobDetail, ServerFnError> {
        let service = self.service.jobs.clone();
        self.details
            .load((project.clone(), id), move || async move {
                service.detail(project, id).await
            })
            .await
    }

    pub(crate) async fn job_coverage(
        &self,
        project: String,
        id: i64,
        aspect: Option<String>,
    ) -> Result<CoverageView, ServerFnError> {
        let service = self.service.jobs.clone();
        self.coverage
            .load((project.clone(), id, aspect.clone()), move || async move {
                service.coverage(project, id, aspect).await
            })
            .await
    }

    pub(crate) fn cached_jobs(&self, project: &str) -> Option<Vec<KnowledgeJob>> {
        self.jobs.get(&project.to_owned())
    }

    pub(crate) fn invalidate_graph(&self, project: &str) {
        self.graphs.invalidate_key(&project.to_owned());
    }
    pub(crate) fn invalidate_document(&self, project: &str, path: &str) {
        self.documents
            .invalidate_key(&(project.to_owned(), path.to_owned()));
    }
    pub(crate) fn invalidate_jobs(&self, project: &str) {
        self.jobs.invalidate_key(&project.to_owned());
    }

    pub(crate) fn clear_cache(&self) {
        self.jobs.clear();
        self.settings.clear();
        self.details.clear();
        self.coverage.clear();
        self.graphs.clear();
        self.documents.clear();
    }
}

pub(crate) fn knowledge_store() -> KnowledgeStore {
    leptos::prelude::expect_context()
}
