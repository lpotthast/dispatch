use crate::backend::automation::routing::controller::RoutingController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router, extract::Path, http::HeaderMap, response::Response, routing::post,
};
use dispatch_types::RoutingExplainRequest;
async fn explain_automation_routing(
    Extension(controller): Extension<std::sync::Arc<RoutingController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<RoutingExplainRequest>,
) -> Response {
    controller
        .explain_automation_routing(project, headers, request)
        .await
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/automation/routing/explain",
            post(explain_automation_routing),
        )
        .route(
            "/operator/api/projects/{project}/automation/routing/explain",
            post(explain_automation_routing),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl RoutingController {
    async fn explain_automation_routing(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: RoutingExplainRequest,
    ) -> Response {
        let result = async {
            crate::backend::attribution::transport::from_headers(
                &self.attribution,
                &project,
                &headers,
            )
            .await?;
            self.routing.explain(&project, request).await
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
        .insert(state.routing_controller.clone());
    next.run(request).await
}
