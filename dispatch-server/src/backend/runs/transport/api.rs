use crate::backend::runs::controller::RunController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Router,
    extract::{Path, Query},
    response::Response,
    routing::get,
};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
struct ListRunsQuery {
    limit: Option<u64>,
}

async fn list_runs(
    Extension(controller): Extension<std::sync::Arc<RunController>>,
    Path(project): Path<String>,
    Query(query): Query<ListRunsQuery>,
) -> Response {
    controller.list_runs(project, query).await
}

async fn get_run_log(
    Extension(controller): Extension<std::sync::Arc<RunController>>,
    Path((project, run_id)): Path<(String, i64)>,
) -> Response {
    controller.get_run_log((project, run_id)).await
}

async fn active_sessions(
    Extension(controller): Extension<std::sync::Arc<RunController>>,
    Path(project): Path<String>,
) -> Response {
    controller.active_sessions(project).await
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/projects/{project}/automation/runs", get(list_runs))
        .route(
            "/api/projects/{project}/automation/runs/{run_id}/log",
            get(get_run_log),
        )
        .route(
            "/api/projects/{project}/automation/sessions",
            get(active_sessions),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl RunController {
    async fn list_runs(
        self: std::sync::Arc<Self>,
        project: String,
        query: ListRunsQuery,
    ) -> Response {
        json_result(self.run_queries.list(&project, query.limit).await)
    }
    async fn get_run_log(self: std::sync::Arc<Self>, (project, run_id): (String, i64)) -> Response {
        let result = self.run_queries.log(&project, run_id).await;
        json_result(result)
    }
    async fn active_sessions(self: std::sync::Arc<Self>, project: String) -> Response {
        let result = self.run_queries.active_sessions(&project).await;
        json_result(result)
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.run_controller.clone());
    next.run(request).await
}
