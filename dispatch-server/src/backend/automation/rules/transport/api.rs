use crate::backend::automation::rules::controller::RuleController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router,
    extract::Path,
    http::HeaderMap,
    response::Response,
    routing::{get, post},
};
use dispatch_types::{AutomationRuleInput, RestoreRevisionRequest};
async fn list_automation_triggers(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    controller.list_automation_triggers(project, headers).await
}
async fn get_automation_trigger(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, id_or_key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    controller
        .get_automation_trigger((project, id_or_key), headers)
        .await
}
async fn list_automation_revisions(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, trigger_id)): Path<(String, i64)>,
) -> Response {
    controller
        .list_automation_revisions((project, trigger_id))
        .await
}
async fn operator_list_rules(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path(project): Path<String>,
) -> Response {
    controller.operator_list_rules(project).await
}
pub(crate) async fn operator_get_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, rule_id)): Path<(String, String)>,
) -> Response {
    controller.operator_get_rule((project, rule_id)).await
}
pub(crate) async fn operator_create_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path(project): Path<String>,
    Json(input): Json<AutomationRuleInput>,
) -> Response {
    controller.operator_create_rule(project, input).await
}
pub(crate) async fn operator_update_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, rule_id)): Path<(String, i64)>,
    Json(input): Json<AutomationRuleInput>,
) -> Response {
    controller
        .operator_update_rule((project, rule_id), input)
        .await
}
async fn operator_delete_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, rule_id)): Path<(String, i64)>,
) -> Response {
    controller.operator_delete_rule((project, rule_id)).await
}
async fn operator_schedule_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, rule_id)): Path<(String, i64)>,
) -> Response {
    controller.operator_schedule_rule((project, rule_id)).await
}
async fn operator_restore_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, rule_id)): Path<(String, i64)>,
    Json(request): Json<RestoreRevisionRequest>,
) -> Response {
    controller
        .operator_restore_rule((project, rule_id), request)
        .await
}
async fn operator_detach_rule(
    Extension(controller): Extension<std::sync::Arc<RuleController>>,
    Path((project, rule_id)): Path<(String, i64)>,
) -> Response {
    controller.operator_detach_rule((project, rule_id)).await
}
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/api/projects/{project}/automation/triggers",
            get(list_automation_triggers),
        )
        .route(
            "/api/projects/{project}/automation/triggers/{id_or_key}",
            get(get_automation_trigger),
        )
        .route(
            "/operator/api/projects/{project}/automation/triggers/{trigger_id}/revisions",
            get(list_automation_revisions),
        )
        .route(
            "/operator/api/projects/{project}/automation/rules",
            get(operator_list_rules).post(operator_create_rule),
        )
        .route(
            "/operator/api/projects/{project}/automation/rules/{rule_id}",
            get(operator_get_rule)
                .put(operator_update_rule)
                .delete(operator_delete_rule),
        )
        .route(
            "/operator/api/projects/{project}/automation/rules/{rule_id}/schedule",
            post(operator_schedule_rule),
        )
        .route(
            "/operator/api/projects/{project}/automation/rules/{rule_id}/restore",
            post(operator_restore_rule),
        )
        .route(
            "/operator/api/projects/{project}/automation/rules/{rule_id}/detach",
            post(operator_detach_rule),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl RuleController {
    async fn list_automation_triggers(
        self: std::sync::Arc<Self>,
        project: String,
        headers: HeaderMap,
    ) -> Response {
        let result = async {
            crate::backend::attribution::transport::from_headers(
                &self.attribution,
                &project,
                &headers,
            )
            .await?;
            self.rules.list(&project).await
        }
        .await;
        json_result(result)
    }
    async fn get_automation_trigger(
        self: std::sync::Arc<Self>,
        (project, id_or_key): (String, String),
        headers: HeaderMap,
    ) -> Response {
        let result = async {
            crate::backend::attribution::transport::from_headers(
                &self.attribution,
                &project,
                &headers,
            )
            .await?;
            self.rules.get(&project, &id_or_key).await
        }
        .await;
        json_result(result)
    }
    async fn list_automation_revisions(
        self: std::sync::Arc<Self>,
        (project, trigger_id): (String, i64),
    ) -> Response {
        let result = async { self.rules.revisions(&project, trigger_id).await }.await;
        json_result(result)
    }
    async fn operator_list_rules(self: std::sync::Arc<Self>, project: String) -> Response {
        json_result(self.rules.list(&project).await)
    }
    async fn operator_get_rule(
        self: std::sync::Arc<Self>,
        (project, rule_id): (String, String),
    ) -> Response {
        json_result(self.rules.get(&project, &rule_id).await)
    }
    async fn operator_create_rule(
        self: std::sync::Arc<Self>,
        project: String,
        input: AutomationRuleInput,
    ) -> Response {
        json_result(self.rules.create_from_input(&project, input).await)
    }
    async fn operator_update_rule(
        self: std::sync::Arc<Self>,
        (project, rule_id): (String, i64),
        input: AutomationRuleInput,
    ) -> Response {
        json_result(self.rules.update_from_input(&project, rule_id, input).await)
    }
    async fn operator_delete_rule(
        self: std::sync::Arc<Self>,
        (project, rule_id): (String, i64),
    ) -> Response {
        json_result(
            self.rules
                .delete(
                    crate::backend::projects::ProjectReference::Name(&project),
                    rule_id,
                )
                .await
                .map(|_| serde_json::json!({ "deleted": true })),
        )
    }
    async fn operator_schedule_rule(
        self: std::sync::Arc<Self>,
        (project, rule_id): (String, i64),
    ) -> Response {
        json_result(self.rules.schedule(&project, rule_id).await)
    }
    async fn operator_restore_rule(
        self: std::sync::Arc<Self>,
        (project, rule_id): (String, i64),
        request: RestoreRevisionRequest,
    ) -> Response {
        let result = async {
            self.rules
                .restore(&project, rule_id, request.revision_id)
                .await
        }
        .await;
        json_result(result)
    }
    async fn operator_detach_rule(
        self: std::sync::Arc<Self>,
        (project, rule_id): (String, i64),
    ) -> Response {
        json_result(self.rules.detach(&project, rule_id).await)
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
