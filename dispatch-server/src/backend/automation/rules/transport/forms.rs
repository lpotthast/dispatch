use crate::backend::automation::rules::controller::RuleController;
use crate::backend::{
    app_state::AppState,
    automation::rules::{self as automation_triggers, model::CreateAutomationTrigger},
    http::error_response,
};
use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use dispatch_types::{AgentToolName, AutomationActivation, AutomationEffect};
#[derive(serde::Deserialize)]
struct CreateAutomationTriggerForm {
    name: String,
    #[serde(default = "default_automation_activation", alias = "kind")]
    activation: String,
    #[serde(default = "default_automation_effect")]
    effect: String,
    #[serde(default = "default_automation_schedule")]
    schedule: String,
    tool: Option<String>,
    #[serde(default = "default_automation_mutability")]
    mutability: String,
    personality_id: Option<i64>,
    work_item_selector: Option<String>,
    priority: Option<i64>,
    prompt: Option<String>,
}

async fn create_automation_trigger(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path(project): Path<String>,
    Form(form): Form<CreateAutomationTriggerForm>,
) -> Response {
    controller.create_automation_trigger(project, form).await
}

async fn delete_automation_trigger(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, trigger_id)): Path<(String, i64)>,
) -> Response {
    controller
        .delete_automation_trigger((project, trigger_id))
        .await
}

#[derive(serde::Deserialize)]
struct UpdateAutomationTriggerForm {
    name: String,
    #[serde(default = "default_automation_activation", alias = "kind")]
    activation: String,
    #[serde(default = "default_automation_effect")]
    effect: String,
    #[serde(default = "default_automation_schedule")]
    schedule: String,
    #[serde(default = "default_automation_mutability")]
    mutability: String,
    personality_id: Option<i64>,
    enabled: Option<String>,
    work_item_selector: Option<String>,
    priority: Option<i64>,
    prompt: Option<String>,
}

async fn update_automation_trigger(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, trigger_id)): Path<(String, i64)>,
    Form(form): Form<UpdateAutomationTriggerForm>,
) -> Response {
    controller
        .update_automation_trigger((project, trigger_id), form)
        .await
}

async fn schedule_automation_trigger_evaluation(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, trigger_id)): Path<(String, i64)>,
) -> Response {
    controller
        .schedule_automation_trigger_evaluation((project, trigger_id))
        .await
}

fn default_automation_activation() -> String {
    AutomationActivation::WorkItem.as_storage().to_owned()
}

fn default_automation_effect() -> String {
    AutomationEffect::ConsumeWork.as_storage().to_owned()
}

fn default_automation_schedule() -> String {
    "@every 15s".to_owned()
}

fn default_automation_mutability() -> String {
    dispatch_types::AutomationRunMutability::Mutating
        .as_storage()
        .to_owned()
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/projects/{project}/automation/triggers",
            post(create_automation_trigger),
        )
        .route(
            "/projects/{project}/automation/triggers/{trigger_id}/delete",
            post(delete_automation_trigger),
        )
        .route(
            "/projects/{project}/automation/triggers/{trigger_id}/update",
            post(update_automation_trigger),
        )
        .route(
            "/projects/{project}/automation/triggers/{trigger_id}/schedule-evaluation",
            post(schedule_automation_trigger_evaluation),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl RuleController {
    async fn create_automation_trigger(
        self: std::sync::Arc<Self>,
        project: String,
        form: CreateAutomationTriggerForm,
    ) -> Response {
        let activation = match form.activation.parse::<AutomationActivation>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let effect = match form.effect.parse::<AutomationEffect>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let tool_name = match form.tool.filter(|value| !value.trim().is_empty()) {
            Some(tool) => match tool.parse::<AgentToolName>() {
                Ok(value) => Some(value),
                Err(err) => return error_response(err).await,
            },
            None => None,
        };
        let work_item_selector =
            match automation_triggers::repository::encoding::selector_from_storage(
                form.work_item_selector.as_deref(),
            ) {
                Ok(selector) => selector,
                Err(err) => return error_response(err).await,
            };
        let mutability = match form.mutability.parse() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        match self
            .rules
            .create(
                &project,
                CreateAutomationTrigger {
                    name: form.name,
                    enabled: true,
                    activation,
                    effect,
                    schedule: form.schedule,
                    tool_name,
                    mutability,
                    personality_id: form.personality_id,
                    prompt: form.prompt.unwrap_or_default(),
                    work_item_selector,
                    priority: form.priority.unwrap_or_default(),
                },
            )
            .await
        {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn delete_automation_trigger(
        self: std::sync::Arc<Self>,
        (project, trigger_id): (String, i64),
    ) -> Response {
        match self
            .rules
            .delete(
                crate::backend::projects::ProjectReference::Name(&project),
                trigger_id,
            )
            .await
        {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn update_automation_trigger(
        self: std::sync::Arc<Self>,
        (project, trigger_id): (String, i64),
        form: UpdateAutomationTriggerForm,
    ) -> Response {
        let activation = match form.activation.parse::<AutomationActivation>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let effect = match form.effect.parse::<AutomationEffect>() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        let work_item_selector =
            match automation_triggers::repository::encoding::selector_from_storage(
                form.work_item_selector.as_deref(),
            ) {
                Ok(selector) => selector,
                Err(err) => return error_response(err).await,
            };
        let mutability = match form.mutability.parse() {
            Ok(value) => value,
            Err(err) => return error_response(err).await,
        };
        match self
            .rules
            .update(
                &project,
                trigger_id,
                automation_triggers::model::UpdateAutomationTrigger {
                    name: form.name,
                    enabled: form.enabled.is_some(),
                    activation,
                    effect,
                    schedule: form.schedule,
                    mutability,
                    personality_id: form.personality_id,
                    prompt: form.prompt.unwrap_or_default(),
                    work_item_selector,
                    priority: form.priority,
                },
            )
            .await
        {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn schedule_automation_trigger_evaluation(
        self: std::sync::Arc<Self>,
        (project, trigger_id): (String, i64),
    ) -> Response {
        match self.rules.schedule(&project, trigger_id).await {
            Ok(_) => Redirect::to(&format!(
                "/automation?project={}",
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
        .insert(state.rule_controller.clone());
    next.run(request).await
}
