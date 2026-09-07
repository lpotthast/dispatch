#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::request::{ServiceFuture, ServiceRequest};
use crate::shared::view_models::RunLogView;
use dispatch_types::{RunLogPage, RunsSection};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct RunService {
    load_section: ServiceRequest<String, RunsSection>,
    load_detail: ServiceRequest<(String, i64), RunLogView>,
    load_log: ServiceRequest<(Option<String>, Option<i64>), RunLogPage>,
    cancel_run: ServiceRequest<(String, i64), ()>,
}

impl RunService {
    pub(crate) fn new(
        http: crate::frontend::http::HttpService,
        load_section: impl Fn(String) -> ServiceFuture<RunsSection> + Send + Sync + 'static,
        load_detail: impl Fn((String, i64)) -> ServiceFuture<RunLogView> + Send + Sync + 'static,
        load_log: impl Fn((Option<String>, Option<i64>)) -> ServiceFuture<RunLogPage>
        + Send
        + Sync
        + 'static,
        cancel_run: impl Fn((String, i64)) -> ServiceFuture<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            load_section: http.request(load_section),
            load_detail: http.request(load_detail),
            load_log: http.request(load_log),
            cancel_run: http.request(cancel_run),
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(
            http.clone(),
            |project| Box::pin(load_runs_section(project)),
            |(project, run_id)| Box::pin(load_run_detail(project, run_id)),
            |(project, run_id)| Box::pin(load_run_log_page(project, run_id)),
            |(project, run_id)| Box::pin(cancel_run(project, run_id)),
        )
    }

    pub(crate) async fn load_section(&self, project: String) -> Result<RunsSection, ServerFnError> {
        self.load_section.execute(project).await
    }

    pub(crate) async fn load_detail(
        &self,
        project: String,
        run_id: i64,
    ) -> Result<RunLogView, ServerFnError> {
        self.load_detail.execute((project, run_id)).await
    }

    pub(crate) async fn load_log(
        &self,
        project: Option<String>,
        run_id: Option<i64>,
    ) -> Result<RunLogPage, ServerFnError> {
        self.load_log.execute((project, run_id)).await
    }

    pub(crate) async fn cancel_run(
        &self,
        project: String,
        run_id: i64,
    ) -> Result<(), ServerFnError> {
        self.cancel_run.execute((project, run_id)).await
    }
}

#[server(prefix = "/leptos")]
async fn load_runs_section(project: String) -> Result<RunsSection, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .runs_section(&project)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_run_detail(project: String, run_id: i64) -> Result<RunLogView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .run_queries
        .log(&project, run_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_run_log_page(
    project: Option<String>,
    run_id: Option<i64>,
) -> Result<RunLogPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    match (project, run_id) {
        (Some(project), Some(run_id)) => state
            .operator_queries
            .run_log_page(&project, run_id)
            .await
            .map_err(|err| ServerFnError::new(err.to_string())),
        _ => Err(ServerFnError::new("Missing run log route parameters")),
    }
}

#[server(prefix = "/leptos")]
async fn cancel_run(project: String, run_id: i64) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .run_control
        .cancel(&project, run_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

pub(crate) fn run_service() -> RunService {
    leptos::prelude::expect_context()
}
