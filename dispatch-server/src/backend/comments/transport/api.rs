use super::super::{AddCommentRequest, CommentTarget};
use crate::backend::comments::controller::CommentController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router, extract::Path, http::HeaderMap, response::Response, routing::get,
};
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/items/{item_id}/comments",
            get(list_comments).post(add_comment),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

pub(crate) async fn list_comments(
    Extension(controller): Extension<std::sync::Arc<CommentController>>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    controller.list_comments((project, item_id)).await
}

pub(crate) async fn add_comment(
    Extension(controller): Extension<std::sync::Arc<CommentController>>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<AddCommentRequest>,
) -> Response {
    controller
        .add_comment((project, item_id), headers, request)
        .await
}

impl CommentController {
    async fn list_comments(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
    ) -> Response {
        json_result(self.comments.list(&project, item_id).await)
    }
    async fn add_comment(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        headers: HeaderMap,
        request: AddCommentRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.comments
                .add(
                    CommentTarget::ProjectItem {
                        project: &project,
                        item_id,
                    },
                    request,
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
        .insert(state.comment_controller.clone());
    next.run(request).await
}
