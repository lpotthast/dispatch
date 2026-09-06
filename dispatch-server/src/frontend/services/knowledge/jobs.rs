use super::*;
use dispatch_types::knowledge::jobs::*;
#[derive(Clone)]
pub(crate) struct KnowledgeJobsUiService {
    settings: ServiceRequest<String, KnowledgeSettings>,
    save_settings: ServiceRequest<(String, KnowledgeSettings), KnowledgeSettings>,
    list: ServiceRequest<String, Vec<KnowledgeJob>>,
    start: ServiceRequest<(String, StartKnowledgeJob), KnowledgeJob>,
    detail: ServiceRequest<(String, i64), KnowledgeJobDetail>,
    action: ServiceRequest<(String, i64, JobAction), KnowledgeJob>,
    coverage: ServiceRequest<(String, i64, Option<String>), CoverageView>,
}
impl KnowledgeJobsUiService {
    pub(crate) fn production() -> Self {
        Self {
            settings: ServiceRequest::new(|p| Box::pin(read_job_settings(p))),
            save_settings: ServiceRequest::new(|(p, s)| Box::pin(write_job_settings(p, s))),
            list: ServiceRequest::new(|p| Box::pin(list_jobs(p))),
            start: ServiceRequest::new(|(p, r)| Box::pin(start_job(p, r))),
            detail: ServiceRequest::new(|(p, id)| Box::pin(job_detail(p, id))),
            action: ServiceRequest::new(|(p, id, a)| Box::pin(mutate_knowledge_job(p, id, a))),
            coverage: ServiceRequest::new(|(p, id, a)| Box::pin(job_coverage(p, id, a))),
        }
    }
    pub(crate) async fn settings(&self, p: String) -> Result<KnowledgeSettings, ServerFnError> {
        self.settings.execute_inline(p).await
    }
    pub(crate) async fn save_settings(
        &self,
        p: String,
        s: KnowledgeSettings,
    ) -> Result<KnowledgeSettings, ServerFnError> {
        self.save_settings.execute_inline((p, s)).await
    }
    pub(crate) async fn list(&self, p: String) -> Result<Vec<KnowledgeJob>, ServerFnError> {
        self.list.execute_inline(p).await
    }
    pub(crate) async fn start(
        &self,
        p: String,
        r: StartKnowledgeJob,
    ) -> Result<KnowledgeJob, ServerFnError> {
        self.start.execute_inline((p, r)).await
    }
    pub(crate) async fn detail(
        &self,
        p: String,
        id: i64,
    ) -> Result<KnowledgeJobDetail, ServerFnError> {
        self.detail.execute_inline((p, id)).await
    }
    pub(crate) async fn action(
        &self,
        p: String,
        id: i64,
        a: JobAction,
    ) -> Result<KnowledgeJob, ServerFnError> {
        self.action.execute_inline((p, id, a)).await
    }
    pub(crate) async fn coverage(
        &self,
        p: String,
        id: i64,
        a: Option<String>,
    ) -> Result<CoverageView, ServerFnError> {
        self.coverage.execute_inline((p, id, a)).await
    }
}
#[server(prefix = "/leptos")]
async fn list_jobs(project: String) -> Result<Vec<KnowledgeJob>, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::list(&s.store, &project)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn start_job(
    project: String,
    request: StartKnowledgeJob,
) -> Result<KnowledgeJob, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::start(&s.store, &project, request)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn job_detail(project: String, id: i64) -> Result<KnowledgeJobDetail, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::detail(&s.store, &project, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn mutate_knowledge_job(
    project: String,
    id: i64,
    action: JobAction,
) -> Result<KnowledgeJob, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::action(&s.store, &s.sessions, &project, id, action)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn job_coverage(
    project: String,
    id: i64,
    aspect: Option<String>,
) -> Result<CoverageView, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::coverage(&s.store, &project, id, aspect)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server(prefix = "/leptos")]
async fn read_job_settings(project: String) -> Result<KnowledgeSettings, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::settings(&s.store, &project)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn write_job_settings(
    project: String,
    settings: KnowledgeSettings,
) -> Result<KnowledgeSettings, ServerFnError> {
    let s = crate::backend::app_state::app_state();
    crate::backend::knowledge::jobs::save_settings(&s.store, &project, settings)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
