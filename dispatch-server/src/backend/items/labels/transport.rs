use crate::backend::items::labels::controller::LabelController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router,
    extract::{Path, Query},
    http::HeaderMap,
    response::Response,
    routing::get,
};
use dispatch_types::{CreateWorkItemLabelRequest, UpdateWorkItemLabelRequest};
use serde::Deserialize;
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/projects/{project}/labels", get(list_project_labels))
        .route(
            "/api/projects/{project}/items/{item_id}/labels",
            get(list_item_labels).post(add_item_label),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/labels/{label_id}",
            axum::routing::patch(update_item_label).delete(delete_item_label),
        )
        .layer(axum::middleware::from_fn(controller_context))
}
#[derive(Debug, Deserialize)]
pub(crate) struct LabelMutationQuery {
    pub(crate) expect_version: Option<i64>,
}

pub(crate) async fn list_project_labels(
    Extension(controller): Extension<std::sync::Arc<LabelController>>,
    Path(project): Path<String>,
) -> Response {
    controller.list_project_labels(project).await
}

pub(crate) async fn list_item_labels(
    Extension(controller): Extension<std::sync::Arc<LabelController>>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    controller.list_item_labels((project, item_id)).await
}

pub(crate) async fn add_item_label(
    Extension(controller): Extension<std::sync::Arc<LabelController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Query(query): Query<LabelMutationQuery>,
    Json(request): Json<CreateWorkItemLabelRequest>,
) -> Response {
    controller
        .add_item_label((project, item_id), headers, query, request)
        .await
}

pub(crate) async fn update_item_label(
    Extension(controller): Extension<std::sync::Arc<LabelController>>,
    Path((project, item_id, label_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemLabelRequest>,
) -> Response {
    controller
        .update_item_label((project, item_id, label_id), headers, request)
        .await
}

pub(crate) async fn delete_item_label(
    Extension(controller): Extension<std::sync::Arc<LabelController>>,
    Path((project, item_id, label_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Query(query): Query<LabelMutationQuery>,
) -> Response {
    controller
        .delete_item_label((project, item_id, label_id), headers, query)
        .await
}

impl LabelController {
    async fn list_project_labels(self: std::sync::Arc<Self>, project: String) -> Response {
        json_result(self.labels.project_labels(&project).await)
    }
    async fn list_item_labels(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
    ) -> Response {
        json_result(self.labels.list(&project, item_id).await)
    }
    async fn add_item_label(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        query: LabelMutationQuery,
        request: CreateWorkItemLabelRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.labels
                .add(
                    &project,
                    item_id,
                    crate::shared::view_models::CreateWorkItemLabelRequest {
                        key: request.key,
                        value: request.value,
                    },
                    query.expect_version,
                    attribution,
                )
                .await
        }
        .await;
        json_result(result)
    }
    async fn update_item_label(
        self: std::sync::Arc<Self>,
        (project, item_id, label_id): (String, i64, i64),
        headers: HeaderMap,
        request: UpdateWorkItemLabelRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.labels
                .update(
                    &project,
                    item_id,
                    label_id,
                    dispatch_types::UpdateWorkItemLabelRequest {
                        key: request.key,
                        value: request.value,
                        expect_version: request.expect_version,
                    },
                    attribution,
                )
                .await
        }
        .await;
        json_result(result)
    }
    async fn delete_item_label(
        self: std::sync::Arc<Self>,
        (project, item_id, label_id): (String, i64, i64),
        headers: HeaderMap,
        query: LabelMutationQuery,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.labels
                .delete(
                    &project,
                    item_id,
                    label_id,
                    query.expect_version,
                    attribution,
                )
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
        .insert(state.label_controller.clone());
    next.run(request).await
}
