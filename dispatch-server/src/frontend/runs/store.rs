use super::types::*;
use crate::frontend::queries::cache::QueryCache;
use crate::frontend::runs::service::RunService;
use dispatch_types::*;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct RunStore {
    service: RunService,
    section: QueryCache<String, RunsSection>,
    detail: QueryCache<(String, i64), RunLogView>,
    log: QueryCache<(Option<String>, Option<i64>), RunDetail>,
}

impl RunStore {
    pub(crate) fn new(service: RunService) -> Self {
        Self {
            service,
            section: QueryCache::persistent("dispatch.store.runs.v1"),
            detail: QueryCache::in_memory(),
            log: QueryCache::in_memory(),
        }
    }

    pub(crate) fn cached_section(&self, project: &str) -> Option<RunsSection> {
        self.section.get(&project.to_owned())
    }

    pub(crate) fn cached_section_untracked(&self, project: &str) -> Option<RunsSection> {
        self.section.get_untracked(&project.to_owned())
    }

    pub(crate) async fn load_section(&self, project: String) -> Result<RunsSection, ServerFnError> {
        let service = self.service.clone();
        self.section
            .load(project.clone(), move || async move {
                service.load_section(project).await
            })
            .await
    }

    pub(crate) fn seed_section(&self, project: String, value: RunsSection) {
        self.section.seed(project.clone(), value);
    }

    pub(crate) fn cached_detail(&self, project: &str, run_id: i64) -> Option<RunLogView> {
        self.detail.get(&(project.to_owned(), run_id))
    }

    pub(crate) fn cached_detail_untracked(&self, project: &str, run_id: i64) -> Option<RunLogView> {
        self.detail.get_untracked(&(project.to_owned(), run_id))
    }

    pub(crate) async fn load_detail(
        &self,
        project: String,
        run_id: i64,
    ) -> Result<RunLogView, ServerFnError> {
        let service = self.service.clone();
        self.detail
            .load_retained(
                (project.clone(), run_id),
                move || async move { service.load_detail(project, run_id).await },
                |value| !value.active,
            )
            .await
    }

    pub(crate) fn seed_detail(&self, project: String, run_id: i64, value: RunLogView) {
        if !value.active {
            self.detail.seed((project, run_id), value);
        }
    }

    pub(crate) fn cached_log(
        &self,
        project: &Option<String>,
        run_id: Option<i64>,
    ) -> Option<RunDetail> {
        self.log.get(&(project.clone(), run_id))
    }

    pub(crate) fn cached_log_untracked(
        &self,
        project: &Option<String>,
        run_id: Option<i64>,
    ) -> Option<RunDetail> {
        self.log.get_untracked(&(project.clone(), run_id))
    }

    pub(crate) async fn load_log(
        &self,
        project: Option<String>,
        run_id: Option<i64>,
    ) -> Result<RunDetail, ServerFnError> {
        let service = self.service.clone();
        self.log
            .load_retained(
                (project.clone(), run_id),
                move || async move { service.load_log(project, run_id).await.map(Into::into) },
                |value| !value.run_log.active,
            )
            .await
    }

    pub(crate) fn seed_log(&self, project: Option<String>, run_id: Option<i64>, value: RunDetail) {
        if !value.run_log.active {
            self.log.seed((project, run_id), value);
        }
    }

    pub(crate) fn invalidate_section(&self, project: String) {
        self.section.invalidate_key(&project);
    }

    pub(crate) fn invalidate_detail(&self, project: String, run_id: i64) {
        self.detail.invalidate_key(&(project, run_id));
    }

    pub(crate) fn invalidate_log(&self, project: Option<String>, run_id: Option<i64>) {
        self.log.invalidate_key(&(project, run_id));
    }

    pub(crate) fn clear_cache(&self) {
        self.section.clear();
        self.detail.clear();
        self.log.clear();
    }
}

pub(crate) fn run_store() -> RunStore {
    leptos::prelude::expect_context()
}
