use crate::backend::automation::revisions::controller::RevisionController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Router,
    extract::{Path, Query},
    response::Response,
    routing::get,
};
use serde::Deserialize;
#[derive(Debug, Deserialize)]
struct ListEvaluationsQuery {
    trigger_id: Option<i64>,
    limit: Option<u64>,
}
async fn operator_revision_analytics(
    Extension(controller): Extension<std::sync::Arc<RevisionController>>,
    Path((project, revision_id)): Path<(String, i64)>,
) -> Response {
    controller
        .operator_revision_analytics((project, revision_id))
        .await
}
async fn operator_list_evaluations(
    Extension(controller): Extension<std::sync::Arc<RevisionController>>,
    Path(project): Path<String>,
    Query(query): Query<ListEvaluationsQuery>,
) -> Response {
    controller.operator_list_evaluations(project, query).await
}
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/operator/api/projects/{project}/automation/revisions/{revision_id}/analytics",
            get(operator_revision_analytics),
        )
        .route(
            "/operator/api/projects/{project}/automation/evaluations",
            get(operator_list_evaluations),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl RevisionController {
    async fn operator_revision_analytics(
        self: std::sync::Arc<Self>,
        (project, revision_id): (String, i64),
    ) -> Response {
        json_result(self.revision_queries.analytics(&project, revision_id).await)
    }
    async fn operator_list_evaluations(
        self: std::sync::Arc<Self>,
        project: String,
        query: ListEvaluationsQuery,
    ) -> Response {
        json_result(
            self.revision_queries
                .evaluations(&project, query.trigger_id, query.limit.unwrap_or(100))
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
        .insert(state.revision_controller.clone());
    next.run(request).await
}
