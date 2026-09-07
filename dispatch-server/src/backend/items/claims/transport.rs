use super::model as item_claims;
use crate::backend::items::claims::controller::ClaimController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router, extract::Path, http::HeaderMap, response::Response, routing::post,
};
use dispatch_types::{
    ClaimWorkItemRequest, ClaimWorkItemResponse, FinishWorkItemRequest, ProgressWorkItemRequest,
    ReleaseWorkItemRequest, RequestFeedbackWorkItemRequest,
};
pub(crate) async fn claim_item(
    Extension(controller): Extension<std::sync::Arc<ClaimController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ClaimWorkItemRequest>,
) -> Response {
    controller.claim_item(project, headers, request).await
}

pub(crate) async fn progress_item(
    Extension(controller): Extension<std::sync::Arc<ClaimController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<ProgressWorkItemRequest>,
) -> Response {
    controller
        .progress_item((project, item_id), headers, request)
        .await
}

pub(crate) async fn finish_item(
    Extension(controller): Extension<std::sync::Arc<ClaimController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<FinishWorkItemRequest>,
) -> Response {
    controller
        .finish_item((project, item_id), headers, request)
        .await
}

pub(crate) async fn release_item(
    Extension(controller): Extension<std::sync::Arc<ClaimController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<ReleaseWorkItemRequest>,
) -> Response {
    controller
        .release_item((project, item_id), headers, request)
        .await
}

pub(crate) async fn request_item_feedback(
    Extension(controller): Extension<std::sync::Arc<ClaimController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<RequestFeedbackWorkItemRequest>,
) -> Response {
    controller
        .request_item_feedback((project, item_id), headers, request)
        .await
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/projects/{project}/items/claim", post(claim_item))
        .route(
            "/api/projects/{project}/items/{item_id}/progress",
            post(progress_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/finish",
            post(finish_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/release",
            post(release_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/request-feedback",
            post(request_item_feedback),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl ClaimController {
    async fn claim_item(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: ClaimWorkItemRequest,
    ) -> Response {
        let attribution = match crate::backend::attribution::transport::parse(&headers) {
            Ok(attribution) => attribution,
            Err(err) => return json_result::<()>(Err(err)),
        };
        json_result(
            self.claims
                .claim_item(&project, &request.agent_id, &request.state, attribution)
                .await
                .map(|item| ClaimWorkItemResponse { item }),
        )
    }
    async fn progress_item(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: ProgressWorkItemRequest,
    ) -> Response {
        let attribution = match crate::backend::attribution::transport::parse(&headers) {
            Ok(attribution) => attribution,
            Err(err) => return json_result::<()>(Err(err)),
        };
        json_result(
            self.claims
                .progress_item(
                    &project,
                    item_id,
                    &request.agent_id,
                    &request.body,
                    attribution,
                )
                .await,
        )
    }
    async fn finish_item(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: FinishWorkItemRequest,
    ) -> Response {
        let attribution = match crate::backend::attribution::transport::parse(&headers) {
            Ok(attribution) => attribution,
            Err(err) => return json_result::<()>(Err(err)),
        };
        json_result(
            self.claims
                .finish_item(
                    &project,
                    item_id,
                    &request.agent_id,
                    &request.report,
                    attribution,
                )
                .await,
        )
    }
    async fn release_item(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: ReleaseWorkItemRequest,
    ) -> Response {
        let attribution = match crate::backend::attribution::transport::parse(&headers) {
            Ok(attribution) => attribution,
            Err(err) => return json_result::<()>(Err(err)),
        };
        json_result(
            self.claims
                .release_item(
                    &project,
                    item_id,
                    &request.agent_id,
                    request.comment,
                    item_claims::ReleaseAutomationDisposition::Blocked,
                    attribution,
                )
                .await,
        )
    }
    async fn request_item_feedback(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: RequestFeedbackWorkItemRequest,
    ) -> Response {
        let attribution = match crate::backend::attribution::transport::parse(&headers) {
            Ok(attribution) => attribution,
            Err(err) => return json_result::<()>(Err(err)),
        };
        json_result(
            self.claims
                .request_feedback(
                    &project,
                    item_id,
                    &request.agent_id,
                    &request.body,
                    attribution,
                )
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
        .insert(state.claim_controller.clone());
    next.run(request).await
}
