use super::*;
use crate::backend::app_state::AppState;
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
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
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    response(
        async {
            operator(&headers)?;
            list(&state.store, &project).await
        }
        .await,
    )
}
async fn start_job(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<StartKnowledgeJob>,
) -> Response {
    response(
        async {
            operator(&headers)?;
            start(&state.store, &project, request).await
        }
        .await,
    )
}
async fn job(
    Extension(state): Extension<AppState>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    response(
        async {
            operator(&headers)?;
            detail(&state.store, &project, id).await
        }
        .await,
    )
}
async fn job_action(
    Extension(state): Extension<AppState>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<JobAction>,
) -> Response {
    response(
        async {
            operator(&headers)?;
            action(&state.store, &state.sessions, &project, id, request).await
        }
        .await,
    )
}
#[derive(Default, Deserialize)]
struct CoverageQuery {
    aspect: Option<String>,
}
async fn job_coverage(
    Extension(state): Extension<AppState>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Query(query): Query<CoverageQuery>,
) -> Response {
    response(
        async {
            if headers.contains_key("x-dispatch-agent-id")
                || headers.contains_key("x-dispatch-agent-run-id")
            {
                let attribution =
                    RequestAttribution::from_knowledge_headers(&state.store, &project, &headers)
                        .await?;
                let (record, _) = active_record(&state.store, &project, id, &attribution).await?;
                if record.job().stage == JobStage::Reader {
                    bail!("independent readers cannot inspect extraction assessments");
                }
            }
            coverage(&state.store, &project, id, query.aspect).await
        }
        .await,
    )
}
async fn source(
    Extension(state): Extension<AppState>,
    Path((project, id, operation)): Path<(String, i64, String)>,
    headers: HeaderMap,
    Query(query): Query<SourceQuery>,
) -> Response {
    response(
        async {
            let a = RequestAttribution::from_knowledge_headers(&state.store, &project, &headers)
                .await?;
            source_query(&state.store, &project, id, &a, operation, query).await
        }
        .await,
    )
}
async fn job_progress(
    Extension(state): Extension<AppState>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<JobProgress>,
) -> Response {
    response(
        async {
            let a = RequestAttribution::from_knowledge_headers(&state.store, &project, &headers)
                .await?;
            progress(&state.store, &project, id, &a, request).await
        }
        .await,
    )
}
async fn job_report(
    Extension(state): Extension<AppState>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<JobReport>,
) -> Response {
    response(
        async {
            let a = RequestAttribution::from_knowledge_headers(&state.store, &project, &headers)
                .await?;
            report(&state.store, &project, id, &a, request).await
        }
        .await,
    )
}
async fn job_assessment(
    Extension(state): Extension<AppState>,
    Path((project, id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<AspectAssessment>,
) -> Response {
    response(
        async {
            let a = RequestAttribution::from_knowledge_headers(&state.store, &project, &headers)
                .await?;
            assessment(&state.store, &project, id, &a, request).await
        }
        .await,
    )
}

async fn get_settings(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    response(
        async {
            operator(&headers)?;
            settings(&state.store, &project).await
        }
        .await,
    )
}
async fn set_settings(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<KnowledgeSettings>,
) -> Response {
    response(
        async {
            operator(&headers)?;
            save_settings(&state.store, &project, request).await
        }
        .await,
    )
}
