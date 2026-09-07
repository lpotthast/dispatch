use crate::backend::app_state::AppState;
use crate::backend::knowledge::jobs::controller::KnowledgeJobController;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use dispatch_types::knowledge::jobs::*;
use rootcause::{Result, prelude::*};
use serde::{Deserialize, Serialize};
fn response<T: Serialize>(result: Result<T>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(dispatch_types::ApiError {
                error: e.to_string(),
                code: None,
                details: None,
            }),
        )
            .into_response(),
    }
}
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/knowledge/job-settings",
            get(get_settings).post(set_settings),
        )
        .route(
            "/api/projects/{project}/knowledge/jobs",
            get(jobs).post(start_job),
        )
        .route("/api/projects/{project}/knowledge/jobs/{id}", get(job))
        .route(
            "/api/projects/{project}/knowledge/jobs/{id}/action",
            post(job_action),
        )
        .route(
            "/api/projects/{project}/knowledge/jobs/{id}/coverage",
            get(job_coverage),
        )
        .route(
            "/api/projects/{project}/knowledge/jobs/{id}/source/{operation}",
            get(source),
        )
        .route(
            "/api/projects/{project}/knowledge/jobs/{id}/progress",
            post(job_progress),
        )
        .route(
            "/api/projects/{project}/knowledge/jobs/{id}/report",
            post(job_report),
        )
        .route(
            "/api/projects/{project}/knowledge/jobs/{id}/assess",
            post(job_assessment),
        )
        .layer(axum::middleware::from_fn(controller_context))
}
fn operator(headers: &HeaderMap) -> Result<()> {
    if headers.contains_key("x-dispatch-agent-id")
        || headers.contains_key("x-dispatch-agent-run-id")
    {
        bail!(
            "knowledge agents use scoped source/progress/report operations; user job controls and extraction history are unavailable"
        );
    }
    Ok(())
}
async fn jobs(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    controller.jobs(project, headers).await
}
async fn start_job(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<StartKnowledgeJob>,
) -> Response {
    controller.start_job(project, headers, request).await
}
async fn job(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    controller.job((project, id), headers).await
}
async fn job_action(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<JobAction>,
) -> Response {
    controller.job_action((project, id), headers, request).await
}
#[derive(Default, Deserialize)]
struct CoverageQuery {
    aspect: Option<String>,
}
async fn job_coverage(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Query(query): Query<CoverageQuery>,
) -> Response {
    controller.job_coverage((project, id), headers, query).await
}
async fn source(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id, operation)): Path<(String, i64, String)>,
    headers: HeaderMap,
    Query(query): Query<SourceQuery>,
) -> Response {
    controller
        .source((project, id, operation), headers, query)
        .await
}
async fn job_progress(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<JobProgress>,
) -> Response {
    controller
        .job_progress((project, id), headers, request)
        .await
}
async fn job_report(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<JobReport>,
) -> Response {
    controller.job_report((project, id), headers, request).await
}
async fn job_assessment(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<AspectAssessment>,
) -> Response {
    controller
        .job_assessment((project, id), headers, request)
        .await
}

async fn get_settings(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    controller.get_settings(project, headers).await
}
async fn set_settings(
    Extension(controller): Extension<std::sync::Arc<KnowledgeJobController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<KnowledgeSettings>,
) -> Response {
    controller.set_settings(project, headers, request).await
}

impl KnowledgeJobController {
    async fn jobs(self: std::sync::Arc<Self>, project: String, headers: HeaderMap) -> Response {
        response(
            async {
                operator(&headers)?;
                self.jobs.list(&project).await
            }
            .await,
        )
    }
    async fn start_job(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: StartKnowledgeJob,
    ) -> Response {
        response(
            async {
                operator(&headers)?;
                self.jobs.start(&project, request).await
            }
            .await,
        )
    }
    async fn job(
        self: std::sync::Arc<Self>,
        (project, id): (String, i64),
        headers: HeaderMap,
    ) -> Response {
        response(
            async {
                operator(&headers)?;
                self.jobs.detail(&project, id).await
            }
            .await,
        )
    }
    async fn job_action(
        self: std::sync::Arc<Self>,
        (project, id): (String, i64),
        headers: HeaderMap,
        request: JobAction,
    ) -> Response {
        response(
            async {
                operator(&headers)?;
                self.jobs.action(&project, id, request).await
            }
            .await,
        )
    }
    async fn job_coverage(
        self: std::sync::Arc<Self>,
        (project, id): (String, i64),
        headers: HeaderMap,
        query: CoverageQuery,
    ) -> Response {
        response(
            async {
                let attribution = if headers.contains_key("x-dispatch-agent-id")
                    || headers.contains_key("x-dispatch-agent-run-id")
                {
                    Some(crate::backend::attribution::transport::parse(&headers)?)
                } else {
                    None
                };
                self.jobs
                    .coverage(&project, id, query.aspect, attribution)
                    .await
            }
            .await,
        )
    }
    async fn source(
        self: std::sync::Arc<Self>,
        (project, id, operation): (String, i64, String),
        headers: HeaderMap,
        query: SourceQuery,
    ) -> Response {
        response(
            async {
                let a = crate::backend::attribution::transport::parse(&headers)?;
                self.jobs
                    .source_query(&project, id, a, operation, query)
                    .await
            }
            .await,
        )
    }
    async fn job_progress(
        self: std::sync::Arc<Self>,
        (project, id): (String, i64),
        headers: HeaderMap,
        request: JobProgress,
    ) -> Response {
        response(
            async {
                let a = crate::backend::attribution::transport::parse(&headers)?;
                self.jobs.progress(&project, id, a, request).await
            }
            .await,
        )
    }
    async fn job_report(
        self: std::sync::Arc<Self>,
        (project, id): (String, i64),
        headers: HeaderMap,
        request: JobReport,
    ) -> Response {
        response(
            async {
                let a = crate::backend::attribution::transport::parse(&headers)?;
                self.jobs.report(&project, id, a, request).await
            }
            .await,
        )
    }
    async fn job_assessment(
        self: std::sync::Arc<Self>,
        (project, id): (String, i64),
        headers: HeaderMap,
        request: AspectAssessment,
    ) -> Response {
        response(
            async {
                let a = crate::backend::attribution::transport::parse(&headers)?;
                self.jobs.assessment(&project, id, a, request).await
            }
            .await,
        )
    }
    async fn get_settings(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
    ) -> Response {
        response(
            async {
                operator(&headers)?;
                self.jobs.settings(&project).await
            }
            .await,
        )
    }
    async fn set_settings(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: KnowledgeSettings,
    ) -> Response {
        response(
            async {
                operator(&headers)?;
                self.jobs.save_settings(&project, request).await
            }
            .await,
        )
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.knowledge_job_controller.clone());
    next.run(request).await
}
