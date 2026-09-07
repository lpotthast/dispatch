use crate::backend::runs::controller::RunController;
use crate::backend::{
    app_state::AppState,
    http::{error_response, safe_return_to},
};
use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
#[derive(serde::Deserialize)]
struct CancelRunForm {
    return_to: Option<String>,
}

async fn cancel_run(
    Extension(controller): Extension<std::sync::Arc<RunController>>,
    Path((project, run_id)): Path<(String, i64)>,
    Form(form): Form<CancelRunForm>,
) -> Response {
    controller.cancel_run((project, run_id), form).await
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/projects/{project}/automation/runs/{run_id}/cancel",
            post(cancel_run),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl RunController {
    async fn cancel_run(
        self: std::sync::Arc<Self>,
        (project, run_id): (String, i64),
        form: CancelRunForm,
    ) -> Response {
        let return_to = safe_return_to(
            form.return_to,
            format!(
                "/projects/{}/automation/runs/{}/log",
                urlencoding::encode(&project),
                run_id
            ),
        );
        let result = self.run_control.cancel(&project, run_id).await;

        match result {
            Ok(()) => Redirect::to(&return_to).into_response(),
            Err(err) => error_response(err).await,
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
        .insert(state.run_controller.clone());
    next.run(request).await
}
