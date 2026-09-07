use crate::backend::execution::codex::controller::CodexController;
use crate::backend::{app_state::AppState, http::error_response};
use axum::{
    Extension, Form,
    response::{IntoResponse, Redirect, Response},
};

#[derive(serde::Deserialize)]
pub(crate) struct DiscoverAgentToolsForm {
    project: Option<String>,
    return_to: Option<String>,
}

pub(crate) async fn discover_agent_tools(
    Extension(controller): Extension<std::sync::Arc<CodexController>>,
    Form(form): Form<DiscoverAgentToolsForm>,
) -> Response {
    controller.discover_agent_tools(form).await
}

pub(crate) async fn logout_codex(
    Extension(controller): Extension<std::sync::Arc<CodexController>>,
    Form(form): Form<DiscoverAgentToolsForm>,
) -> Response {
    controller.logout_codex(form).await
}

fn codex_return_target(return_to: Option<String>, project: Option<String>) -> String {
    return_to
        .filter(|target| target.starts_with('/') && !target.starts_with("//"))
        .or_else(|| {
            project
                .filter(|project| !project.trim().is_empty())
                .map(|project| format!("/?project={}", urlencoding::encode(&project)))
        })
        .unwrap_or_else(|| "/projects".to_owned())
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
    axum::Router::new()
        .route(
            "/agent-tools/discover",
            axum::routing::post(discover_agent_tools),
        )
        .route("/codex/logout", axum::routing::post(logout_codex))
        .layer(axum::middleware::from_fn(controller_context))
}

impl CodexController {
    async fn discover_agent_tools(
        self: std::sync::Arc<Self>,
        form: DiscoverAgentToolsForm,
    ) -> Response {
        match self.codex.discover_tools(&self.codex_status).await {
            Ok(_) => {
                let target = codex_return_target(form.return_to, form.project);
                Redirect::to(&target).into_response()
            }
            Err(err) => error_response(err).await,
        }
    }
    async fn logout_codex(self: std::sync::Arc<Self>, form: DiscoverAgentToolsForm) -> Response {
        match self.codex.logout(&self.codex_status).await {
            Ok(()) => {
                let target = codex_return_target(form.return_to, form.project);
                Redirect::to(&target).into_response()
            }
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
        .insert(state.codex_controller.clone());
    next.run(request).await
}
