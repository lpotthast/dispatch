use crate::backend::items::controller::ItemController;
use crate::backend::{
    app_state::AppState,
    http::{error_response, parse_optional_reasoning_effort},
    items::CreateWorkItem,
};
use axum::{
    Extension, Form, Router,
    extract::Path,
    response::{IntoResponse, Redirect, Response},
    routing::post,
};
use dispatch_types::{DEFAULT_STATE_LABEL, UpdateWorkItemRequest};
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/projects/{project}/items", post(create_item))
        .route(
            "/projects/{project}/items/{item_id}/update",
            post(update_item),
        )
        .route(
            "/projects/{project}/items/{item_id}/delete",
            post(delete_item),
        )
        .layer(axum::middleware::from_fn(controller_context))
}
#[derive(serde::Deserialize)]
struct CreateItemForm {
    title: String,
    description: String,
    state: Option<String>,
    agent_model_override: Option<String>,
    agent_reasoning_effort_override: Option<String>,
}

async fn create_item(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path(project): Path<String>,
    Form(form): Form<CreateItemForm>,
) -> Response {
    controller.create_item_from_form(project, form).await
}

#[derive(serde::Deserialize)]
struct UpdateItemForm {
    title: String,
    description: String,
    version: i64,
    agent_model_override: Option<String>,
    agent_reasoning_effort_override: Option<String>,
}

async fn update_item(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path((project, item_id)): Path<(String, i64)>,
    Form(form): Form<UpdateItemForm>,
) -> Response {
    controller
        .update_item_from_form((project, item_id), form)
        .await
}

fn parse_optional_state_label(value: Option<String>) -> String {
    value
        .and_then(|value| {
            let value = value.trim().to_owned();
            (!value.is_empty()).then_some(value)
        })
        .unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned())
}

async fn delete_item(
    Extension(controller): Extension<std::sync::Arc<ItemController>>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    controller.delete_item((project, item_id)).await
}

fn item_redirect(project: &str, item_id: i64) -> Response {
    Redirect::to(&format!(
        "/projects/{}/items/{}",
        urlencoding::encode(project),
        item_id
    ))
    .into_response()
}

impl ItemController {
    async fn create_item_from_form(
        self: std::sync::Arc<Self>,
        project: String,
        form: CreateItemForm,
    ) -> Response {
        let item_state = parse_optional_state_label(form.state);
        match self
            .item_creation
            .create(
                crate::backend::projects::ProjectReference::Name(&project),
                CreateWorkItem {
                    title: form.title,
                    description: form.description,
                    state: item_state,
                    agent_model_override: form
                        .agent_model_override
                        .filter(|value| !value.trim().is_empty()),
                    agent_reasoning_effort_override: match parse_optional_reasoning_effort(
                        form.agent_reasoning_effort_override,
                    ) {
                        Ok(value) => value,
                        Err(err) => return error_response(err).await,
                    },
                    initial_labels: Vec::new(),
                },
                Default::default(),
            )
            .await
        {
            Ok(_) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
                .into_response(),
            Err(err) => error_response(err).await,
        }
    }
    async fn update_item_from_form(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
        form: UpdateItemForm,
    ) -> Response {
        let agent_reasoning_effort_override =
            match parse_optional_reasoning_effort(form.agent_reasoning_effort_override) {
                Ok(value) => value,
                Err(err) => return error_response(err).await,
            };
        match self
            .items
            .update(
                crate::backend::projects::ProjectReference::Name(&project),
                item_id,
                UpdateWorkItemRequest {
                    title: Some(form.title),
                    description: Some(form.description),
                    state: None,
                    agent_model_override: Some(
                        form.agent_model_override
                            .filter(|value| !value.trim().is_empty()),
                    ),
                    agent_reasoning_effort_override: Some(agent_reasoning_effort_override),
                    expect_version: Some(form.version),
                },
                Default::default(),
            )
            .await
        {
            Ok(_) => item_redirect(&project, item_id),
            Err(err) => error_response(err).await,
        }
    }
    async fn delete_item(
        self: std::sync::Arc<Self>,
        (project, item_id): (String, i64),
    ) -> Response {
        match self
            .items
            .delete(
                crate::backend::projects::ProjectReference::Name(&project),
                item_id,
            )
            .await
            .map(|_| ())
        {
            Ok(()) => Redirect::to(&format!("/?project={}", urlencoding::encode(&project)))
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
        .insert(state.item_controller.clone());
    next.run(request).await
}
