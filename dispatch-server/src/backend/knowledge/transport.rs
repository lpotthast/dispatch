use crate::backend::knowledge::controller::KnowledgeController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Router,
    extract::{Path, Query},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
};
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/knowledge/{operation}",
            get(query_knowledge),
        )
        .layer(axum::middleware::from_fn(controller_context))
}
pub(crate) async fn query_knowledge(
    Extension(controller): Extension<std::sync::Arc<KnowledgeController>>,
    Path((project, operation)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<dispatch_types::knowledge::KnowledgeQuery>,
) -> Response {
    controller
        .query_knowledge((project, operation), headers, query)
        .await
}

impl KnowledgeController {
    async fn query_knowledge(
        self: std::sync::Arc<Self>,
        (project, operation): (String, String),
        headers: HeaderMap,
        query: dispatch_types::knowledge::KnowledgeQuery,
    ) -> Response {
        use dispatch_types::knowledge::KnowledgeOperation;
        let operation = match operation.as_str() {
            "root" => KnowledgeOperation::Root,
            "node" => KnowledgeOperation::Node,
            "search" => KnowledgeOperation::Search,
            "check" => KnowledgeOperation::Check,
            "documents" => KnowledgeOperation::List,
            "graph" => KnowledgeOperation::Graph,
            _ => return StatusCode::NOT_FOUND.into_response(),
        };
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.knowledge_queries
                .query(&project, attribution, operation, query)
                .await
        }
        .await;
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
        .insert(state.knowledge_controller.clone());
    next.run(request).await
}
