use crate::backend::items::groups::controller::GroupController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router,
    extract::Path,
    http::HeaderMap,
    response::Response,
    routing::{get, post},
};
use dispatch_types::{AssignWorkItemGroupRequest, CreateWorkItemGroupRequest};
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/work-groups",
            get(list_work_groups).post(create_work_group),
        )
        .route(
            "/api/projects/{project}/work-groups/{group_key}/items",
            post(assign_work_group_items),
        )
        .layer(axum::middleware::from_fn(controller_context))
}
async fn list_work_groups(
    Extension(controller): Extension<std::sync::Arc<GroupController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    controller.list_work_groups(project, headers).await
}

async fn create_work_group(
    Extension(controller): Extension<std::sync::Arc<GroupController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemGroupRequest>,
) -> Response {
    controller
        .create_work_group(project, headers, request)
        .await
}

async fn assign_work_group_items(
    Extension(controller): Extension<std::sync::Arc<GroupController>>,
    Path((project, group_key)): Path<(String, String)>,
    headers: HeaderMap,
    Json(request): Json<AssignWorkItemGroupRequest>,
) -> Response {
    controller
        .assign_work_group_items((project, group_key), headers, request)
        .await
}

impl GroupController {
    async fn list_work_groups(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.groups.list(&project, attribution).await
        }
        .await;
        json_result(result)
    }
    async fn create_work_group(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
        request: CreateWorkItemGroupRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.groups.create(&project, request, attribution).await
        }
        .await;
        json_result(result)
    }
    async fn assign_work_group_items(
        self: std::sync::Arc<Self>,
        (project, group_key): (String, String),
        headers: HeaderMap,
        request: AssignWorkItemGroupRequest,
    ) -> Response {
        let result = async {
            let attribution = crate::backend::attribution::transport::parse(&headers)?;
            self.groups
                .assign(&project, &group_key, request.item_ids, attribution)
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
        .insert(state.group_controller.clone());
    next.run(request).await
}
