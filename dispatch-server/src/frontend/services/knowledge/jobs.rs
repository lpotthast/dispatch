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
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .list(&project)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn start_job(
    project: String,
    request: StartKnowledgeJob,
) -> Result<KnowledgeJob, ServerFnError> {
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .start(&project, request)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn job_detail(project: String, id: i64) -> Result<KnowledgeJobDetail, ServerFnError> {
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .detail(&project, id)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn mutate_knowledge_job(
    project: String,
    id: i64,
    action: JobAction,
) -> Result<KnowledgeJob, ServerFnError> {
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .action(&project, id, action)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn job_coverage(
    project: String,
    id: i64,
    aspect: Option<String>,
) -> Result<CoverageView, ServerFnError> {
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .coverage(&project, id, aspect, None)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[server(prefix = "/leptos")]
async fn read_job_settings(project: String) -> Result<KnowledgeSettings, ServerFnError> {
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .settings(&project)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}
#[server(prefix = "/leptos")]
async fn write_job_settings(
    project: String,
    settings: KnowledgeSettings,
) -> Result<KnowledgeSettings, ServerFnError> {
    let s = leptos::prelude::expect_context::<crate::backend::app_state::AppState>();
    s.jobs
        .save_settings(&project, settings)
        .await
        .map_err(|e| ServerFnError::new(e.to_string()))
}

#[cfg(all(test, feature = "ssr"))]
mod backend_adapter_tests {
    use super::*;
    use assertr::prelude::*;
    use leptos::reactive::computed::ScopedFuture;
    #[tokio::test]
    async fn job_json_and_leptos_adapters_share_history_and_request_instances() {
        let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
        let owner = Owner::new();
        owner.with(|| provide_context(app.state.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!(
            "http://{}/api/projects/demo/knowledge",
            listener.local_addr().unwrap()
        );
        let router = crate::backend::knowledge::jobs::api::routes::<()>()
            .layer(axum::Extension(app.state.clone()));
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let settings = KnowledgeSettings {
            application_mode: ApplicationMode::Automatic,
        };
        let saved = owner
            .with(|| ScopedFuture::new(write_job_settings("demo".into(), settings.clone())))
            .await
            .unwrap();
        let via_json: KnowledgeSettings = client
            .get(format!("{url}/job-settings"))
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_that!(&saved).is_equal_to(via_json);
        let request = StartKnowledgeJob {
            request_id: "adapter-job".into(),
            budget_seconds: 60,
            ..Default::default()
        };
        let job = owner
            .with(|| ScopedFuture::new(start_job("demo".into(), request.clone())))
            .await
            .unwrap();
        let repeated: KnowledgeJob = client
            .post(format!("{url}/jobs"))
            .json(&request)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_that!(&job).is_equal_to(repeated);
        assert_that!(
            &owner
                .with(|| ScopedFuture::new(list_jobs("demo".into())))
                .await
                .unwrap()
        )
        .is_equal_to(app.state.jobs.list("demo").await.unwrap());
        let cancelled = owner
            .with(|| {
                ScopedFuture::new(mutate_knowledge_job(
                    "demo".into(),
                    job.id,
                    JobAction::Cancel,
                ))
            })
            .await
            .unwrap();
        assert_that!(&cancelled.status).is_equal_to(JobStatus::Cancelled);
        assert_that!(&app.state.jobs.detail("demo", job.id).await.unwrap().job)
            .is_equal_to(Some(cancelled));
        let other_temp = tempfile::tempdir().unwrap();
        let other = crate::backend::application::Application::open(
            other_temp.path().join("other.db"),
            "http://127.0.0.1:4102".into(),
        )
        .await
        .unwrap();
        let other_owner = Owner::new();
        other_owner.with(|| provide_context(other.state.clone()));
        assert_that!(
            &other_owner
                .with(|| ScopedFuture::new(job_detail("demo".into(), job.id)))
                .await
                .is_err()
        )
        .is_true();
        server.abort();
    }
}
