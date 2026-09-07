use crate::backend::automation::controller::AutomationController;
use crate::backend::{
    app_state::AppState, automation::launch::model::StartAutomation, http::error_response,
    runs::launch::model::AgentLaunchTargetV1,
};
use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use dispatch_types::AgentToolName;
#[derive(serde::Deserialize)]
struct StartAutomationForm {
    tool: Option<String>,
    item_id: Option<i64>,
    prompt: Option<String>,
    mutability: Option<String>,
}

async fn start_automation(
    Extension(controller): Extension<std::sync::Arc<AutomationController>>,
    Path(project): Path<String>,
    Form(form): Form<StartAutomationForm>,
) -> Response {
    controller.start_automation(project, form).await
}

async fn stop_automation(
    Extension(controller): Extension<std::sync::Arc<AutomationController>>,
    Path(project): Path<String>,
) -> Response {
    controller.stop_automation(project).await
}

async fn recover_stale_claims(
    Extension(controller): Extension<std::sync::Arc<AutomationController>>,
    Path(project): Path<String>,
) -> Response {
    controller.recover_stale_claims(project).await
}

async fn cleanup_worktrees(
    Extension(controller): Extension<std::sync::Arc<AutomationController>>,
    Path(project): Path<String>,
) -> Response {
    controller.cleanup_worktrees(project).await
}

pub(crate) fn routes() -> Router<leptos::prelude::LeptosOptions> {
    Router::new()
        .route(
            "/projects/{project}/automation/start",
            post(start_automation),
        )
        .route("/projects/{project}/automation/stop", post(stop_automation))
        .route(
            "/projects/{project}/automation/recover-stale-claims",
            post(recover_stale_claims),
        )
        .route(
            "/projects/{project}/automation/cleanup-worktrees",
            post(cleanup_worktrees),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl AutomationController {
    async fn start_automation(
        self: std::sync::Arc<Self>,
        project: String,
        form: StartAutomationForm,
    ) -> Response {
        let tool = match form.tool.filter(|value| !value.trim().is_empty()) {
            Some(tool) => match tool.parse::<AgentToolName>() {
                Ok(value) => Some(value),
                Err(err) => return error_response(err).await,
            },
            None => None,
        };
        let is_one_shot = tool.is_some()
            || form.item_id.is_some()
            || form
                .mutability
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty())
            || form
                .prompt
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty());
        let mutability = match form.mutability.filter(|value| !value.trim().is_empty()) {
            Some(value) => match value.parse() {
                Ok(value) => Some(value),
                Err(err) => return error_response(err).await,
            },
            None => None,
        };

        let result = if is_one_shot {
            self.launch
                .start_background(
                    project.clone(),
                    StartAutomation {
                        tool,
                        launch_target: AgentLaunchTargetV1::none(),
                        work_item_selector: None,
                        extra_prompt: form.prompt.filter(|value| !value.trim().is_empty()),
                        mutability,
                        personality_id: None,
                        trigger: None,
                        execution: Default::default(),
                        postconditions: None,
                    },
                    form.item_id,
                )
                .await
                .map(|_| ())
        } else {
            self.automation_supervisor
                .start_project(project.clone())
                .await
        };

        match result {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn stop_automation(self: std::sync::Arc<Self>, project: String) -> Response {
        match self.automation_supervisor.stop_project(&project).await {
            Ok(()) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(error) => error_response(error).await,
        }
    }
    async fn recover_stale_claims(self: std::sync::Arc<Self>, project: String) -> Response {
        match self.claims.recover_configured(&project, None).await {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn cleanup_worktrees(self: std::sync::Arc<Self>, project: String) -> Response {
        match self.launch.cleanup_worktrees(&project, None).await {
            Ok(_) => Redirect::to(&format!(
                "/project?project={}",
                urlencoding::encode(&project)
            ))
            .into_response(),
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
        .insert(state.automation_controller.clone());
    next.run(request).await
}
