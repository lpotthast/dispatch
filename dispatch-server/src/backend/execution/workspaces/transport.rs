use super::model::WorkspaceOpenTarget;
use crate::backend::execution::workspaces::controller::WorkspaceController;
use crate::backend::{
    app_state::AppState,
    http::{error_response, safe_return_to},
};
use axum::{
    Extension, Form, Json, Router,
    extract::Path,
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use dispatch_types::PickFolderResponse;
#[derive(serde::Deserialize)]
struct OpenWorkspaceForm {
    target: String,
    return_to: Option<String>,
}

#[derive(serde::Deserialize)]
struct OpenDirectoryForm {
    return_to: Option<String>,
}

async fn open_database_directory(
    Extension(controller): Extension<std::sync::Arc<WorkspaceController>>,
    Form(form): Form<OpenDirectoryForm>,
) -> Response {
    controller.open_database_directory(form).await
}

async fn open_project_workspace(
    Extension(controller): Extension<std::sync::Arc<WorkspaceController>>,
    Path(project): Path<String>,
    Form(form): Form<OpenWorkspaceForm>,
) -> Response {
    controller.open_project_workspace(project, form).await
}

async fn open_run_workspace(
    Extension(controller): Extension<std::sync::Arc<WorkspaceController>>,
    Path((project, run_id)): Path<(String, i64)>,
    Form(form): Form<OpenWorkspaceForm>,
) -> Response {
    controller.open_run_workspace((project, run_id), form).await
}

#[derive(serde::Serialize)]
struct ErrorJson {
    error: String,
}

async fn pick_folder(
    Extension(controller): Extension<std::sync::Arc<WorkspaceController>>,
) -> Response {
    controller.pick_folder().await
}

pub(crate) fn routes() -> Router<leptos::prelude::LeptosOptions> {
    Router::new()
        .route("/system/pick-folder", post(pick_folder))
        .route("/system/database/open", post(open_database_directory))
        .route(
            "/projects/{project}/workspace/open",
            post(open_project_workspace),
        )
        .route(
            "/projects/{project}/automation/runs/{run_id}/workspace/open",
            post(open_run_workspace),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl WorkspaceController {
    async fn open_database_directory(
        self: std::sync::Arc<Self>,
        form: OpenDirectoryForm,
    ) -> Response {
        let return_to = safe_return_to(form.return_to, "/".to_owned());
        let result = self.workspaces.open_database_directory().await;

        match result {
            Ok(()) => Redirect::to(&return_to).into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn open_project_workspace(
        self: std::sync::Arc<Self>,
        project: String,
        form: OpenWorkspaceForm,
    ) -> Response {
        let return_to = safe_return_to(
            form.return_to,
            format!("/?project={}", urlencoding::encode(&project)),
        );
        let target = match WorkspaceOpenTarget::parse(&form.target) {
            Ok(target) => target,
            Err(err) => return error_response(err).await,
        };
        let result = self.workspaces.open_project(&project, target).await;

        match result {
            Ok(()) => Redirect::to(&return_to).into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn open_run_workspace(
        self: std::sync::Arc<Self>,
        (project, run_id): (String, i64),
        form: OpenWorkspaceForm,
    ) -> Response {
        let return_to = safe_return_to(
            form.return_to,
            format!(
                "/projects/{}/automation/runs/{}/log",
                urlencoding::encode(&project),
                run_id
            ),
        );
        let target = match WorkspaceOpenTarget::parse(&form.target) {
            Ok(target) => target,
            Err(err) => return error_response(err).await,
        };
        let result = self.workspaces.open_run(&project, run_id, target).await;

        match result {
            Ok(()) => Redirect::to(&return_to).into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn pick_folder(self: std::sync::Arc<Self>) -> Response {
        match self.workspaces.choose_folder().await {
            Ok(path) => Json(PickFolderResponse { path }).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorJson {
                    error: err.to_string(),
                }),
            )
                .into_response(),
        }
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.workspace_controller.clone());
    next.run(request).await
}
