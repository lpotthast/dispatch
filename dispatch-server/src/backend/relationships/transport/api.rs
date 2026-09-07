use crate::backend::relationships::controller::RelationshipController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router, extract::Path, http::HeaderMap, response::Response, routing::get,
};
use dispatch_types::{CreateWorkItemRelationshipRequest, UpdateWorkItemRelationshipRequest};
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/items/{item_id}/relationships",
            get(list_item_relationships).post(create_item_relationship),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/relationships/{relationship_id}",
            axum::routing::patch(update_item_relationship).delete(delete_item_relationship),
        )
        .route(
            "/api/projects/{project}/relationships/{relationship_id}",
            axum::routing::patch(update_relationship).delete(delete_relationship),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

pub(crate) async fn list_item_relationships(
    Extension(controller): Extension<std::sync::Arc<RelationshipController>>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    controller.list_item_relationships((project, item_id)).await
}

pub(crate) async fn create_item_relationship(
    Extension(controller): Extension<std::sync::Arc<RelationshipController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemRelationshipRequest>,
) -> Response {
    controller
        .create_item_relationship((project, item_id), headers, request)
        .await
}

pub(crate) async fn update_relationship(
    Extension(controller): Extension<std::sync::Arc<RelationshipController>>,
    Path((project, relationship_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemRelationshipRequest>,
) -> Response {
    controller
        .update_relationship((project, relationship_id), headers, request)
        .await
}

pub(crate) async fn delete_relationship(
    Extension(controller): Extension<std::sync::Arc<RelationshipController>>,
    Path((project, relationship_id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    controller
        .delete_relationship((project, relationship_id), headers)
        .await
}

pub(crate) async fn update_item_relationship(
    Extension(controller): Extension<std::sync::Arc<RelationshipController>>,
    Path((project, item_id, relationship_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemRelationshipRequest>,
) -> Response {
    controller
        .update_item_relationship((project, item_id, relationship_id), headers, request)
        .await
}

pub(crate) async fn delete_item_relationship(
    Extension(controller): Extension<std::sync::Arc<RelationshipController>>,
    Path((project, item_id, relationship_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
) -> Response {
    controller
        .delete_item_relationship((project, item_id, relationship_id), headers)
        .await
}

impl RelationshipController {
    async fn list_item_relationships(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
    ) -> Response {
        json_result(self.relationships.list(&project, item_id).await)
    }
    async fn create_item_relationship(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: CreateWorkItemRelationshipRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.relationships
                .create(
                    &project,
                    item_id,
                    request.target_work_item_id,
                    request.kind,
                    attribution,
                )
                .await
        }
        .await;
        json_result(result)
    }
    async fn update_relationship(
        self: std::sync::Arc<Self>,
        (project, relationship_id): (String, i64),
        headers: HeaderMap,
        request: UpdateWorkItemRelationshipRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.relationships
                .update(&project, None, relationship_id, request.kind, attribution)
                .await
        }
        .await;
        json_result(result)
    }
    async fn delete_relationship(
        self: std::sync::Arc<Self>,
        (project, relationship_id): (String, i64),
        headers: HeaderMap,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.relationships
                .delete(&project, None, relationship_id, attribution)
                .await
        }
        .await;
        json_result(result)
    }
    async fn update_item_relationship(
        self: std::sync::Arc<Self>,
        (project, item_id, relationship_id): (String, i64, i64),
        headers: HeaderMap,
        request: UpdateWorkItemRelationshipRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.relationships
                .update(
                    &project,
                    Some(item_id),
                    relationship_id,
                    request.kind,
                    attribution,
                )
                .await
        }
        .await;
        json_result(result)
    }
    async fn delete_item_relationship(
        self: std::sync::Arc<Self>,
        (project, item_id, relationship_id): (String, i64, i64),
        headers: HeaderMap,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.relationships
                .delete(&project, Some(item_id), relationship_id, attribution)
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
        .insert(state.relationship_controller.clone());
    next.run(request).await
}
