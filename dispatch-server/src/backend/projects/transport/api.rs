use crate::backend::projects::controller::ProjectController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{Extension, Router, extract::Path, response::Response, routing::get};

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{project}", get(get_project))
        .route(
            "/api/projects/{project}/settings",
            get(get_project_settings),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

pub(crate) async fn list_projects(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
) -> Response {
    controller.list_projects().await
}

async fn get_project(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
) -> Response {
    controller.get_project(project).await
}

async fn get_project_settings(
    Extension(controller): Extension<std::sync::Arc<ProjectController>>,
    Path(project): Path<String>,
) -> Response {
    controller.get_project_settings(project).await
}

impl ProjectController {
    async fn list_projects(self: std::sync::Arc<Self>) -> Response {
        json_result(self.projects.list().await)
    }
    async fn get_project(self: std::sync::Arc<Self>, project: String) -> Response {
        json_result(self.projects.get(&project).await)
    }
    async fn get_project_settings(self: std::sync::Arc<Self>, project: String) -> Response {
        json_result(self.projects.settings(&project).await)
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.project_controller.clone());
    next.run(request).await
}
