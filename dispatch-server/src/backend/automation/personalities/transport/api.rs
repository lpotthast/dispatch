use crate::backend::automation::personalities::controller::PersonalityController;
use crate::backend::{api::json_result, app_state::AppState, projects::ProjectReference};
use axum::{Extension, Json, extract::Path, response::Response};
use dispatch_types::{AutomationPersonalityInput, RestoreRevisionRequest};

pub(crate) async fn operator_list_personalities(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path(project): Path<String>,
) -> Response {
    controller.operator_list_personalities(project).await
}

pub(crate) async fn operator_get_personality(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path((project, personality_id)): Path<(String, String)>,
) -> Response {
    controller
        .operator_get_personality((project, personality_id))
        .await
}

pub(crate) async fn operator_create_personality(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path(project): Path<String>,
    Json(input): Json<AutomationPersonalityInput>,
) -> Response {
    controller.operator_create_personality(project, input).await
}

pub(crate) async fn operator_update_personality(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path((project, personality_id)): Path<(String, i64)>,
    Json(input): Json<AutomationPersonalityInput>,
) -> Response {
    controller
        .operator_update_personality((project, personality_id), input)
        .await
}

pub(crate) async fn operator_delete_personality(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path((project, personality_id)): Path<(String, i64)>,
) -> Response {
    controller
        .operator_delete_personality((project, personality_id))
        .await
}

pub(crate) async fn operator_list_personality_revisions(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path((project, personality_id)): Path<(String, i64)>,
) -> Response {
    controller
        .operator_list_personality_revisions((project, personality_id))
        .await
}

pub(crate) async fn operator_restore_personality(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path((project, personality_id)): Path<(String, i64)>,
    Json(request): Json<RestoreRevisionRequest>,
) -> Response {
    controller
        .operator_restore_personality((project, personality_id), request)
        .await
}

pub(crate) async fn operator_detach_personality(
    Extension(controller): Extension<std::sync::Arc<PersonalityController>>,
    Path((project, personality_id)): Path<(String, i64)>,
) -> Response {
    controller
        .operator_detach_personality((project, personality_id))
        .await
}

pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> axum::Router<S> {
    use axum::routing::{get, post};
    axum::Router::new()
        .route(
            "/operator/api/projects/{project}/automation/personalities",
            get(operator_list_personalities).post(operator_create_personality),
        )
        .route(
            "/operator/api/projects/{project}/automation/personalities/{personality_id}",
            get(operator_get_personality)
                .put(operator_update_personality)
                .delete(operator_delete_personality),
        )
        .route(
            "/operator/api/projects/{project}/automation/personalities/{personality_id}/revisions",
            get(operator_list_personality_revisions),
        )
        .route(
            "/operator/api/projects/{project}/automation/personalities/{personality_id}/restore",
            post(operator_restore_personality),
        )
        .route(
            "/operator/api/projects/{project}/automation/personalities/{personality_id}/detach",
            post(operator_detach_personality),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl PersonalityController {
    async fn operator_list_personalities(self: std::sync::Arc<Self>, project: String) -> Response {
        json_result(self.personalities.list(&project).await)
    }
    async fn operator_get_personality(
        self: std::sync::Arc<Self>,
        (project, personality_id): (String, String),
    ) -> Response {
        json_result(self.personalities.get(&project, &personality_id).await)
    }
    async fn operator_create_personality(
        self: std::sync::Arc<Self>,
        project: String,
        input: AutomationPersonalityInput,
    ) -> Response {
        json_result(
            self.personalities
                .create(ProjectReference::Name(&project), input)
                .await,
        )
    }
    async fn operator_update_personality(
        self: std::sync::Arc<Self>,
        (project, personality_id): (String, i64),
        input: AutomationPersonalityInput,
    ) -> Response {
        json_result(
            self.personalities
                .update(ProjectReference::Name(&project), personality_id, input)
                .await,
        )
    }
    async fn operator_delete_personality(
        self: std::sync::Arc<Self>,
        (project, personality_id): (String, i64),
    ) -> Response {
        json_result(
            self.personalities
                .delete(ProjectReference::Name(&project), personality_id)
                .await
                .map(|_| serde_json::json!({ "deleted": true })),
        )
    }
    async fn operator_list_personality_revisions(
        self: std::sync::Arc<Self>,
        (project, personality_id): (String, i64),
    ) -> Response {
        let result = async { self.personalities.revisions(&project, personality_id).await }.await;
        json_result(result)
    }
    async fn operator_restore_personality(
        self: std::sync::Arc<Self>,
        (project, personality_id): (String, i64),
        request: RestoreRevisionRequest,
    ) -> Response {
        json_result(
            self.personalities
                .restore(&project, personality_id, request.revision_id)
                .await,
        )
    }
    async fn operator_detach_personality(
        self: std::sync::Arc<Self>,
        (project, personality_id): (String, i64),
    ) -> Response {
        json_result(self.personalities.detach(&project, personality_id).await)
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.personality_controller.clone());
    next.run(request).await
}
