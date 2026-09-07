use crate::backend::automation::bundles::controller::BundleController;
use crate::backend::{api::json_result, app_state::AppState};
use axum::{
    Extension, Json, Router,
    extract::Path,
    response::Response,
    routing::{get, post},
};
use dispatch_types::{
    AutomationBundleExportView, AutomationBundleValidationView, BundleYamlRequest,
    RemoveAutomationBundleRequest,
};
async fn validate_automation_bundle(Json(request): Json<BundleYamlRequest>) -> Response {
    json_result(
        crate::backend::automation::bundles::policy::validate_yaml(&request.yaml).map(|bundle| {
            AutomationBundleValidationView {
                manifest: bundle.manifest,
                manifest_hash: bundle.manifest_hash,
            }
        }),
    )
}
async fn diff_automation_bundle(
    Extension(controller): Extension<std::sync::Arc<BundleController>>,
    Path(project): Path<String>,
    Json(request): Json<BundleYamlRequest>,
) -> Response {
    controller.diff_automation_bundle(project, request).await
}
async fn apply_automation_bundle(
    Extension(controller): Extension<std::sync::Arc<BundleController>>,
    Path(project): Path<String>,
    Json(request): Json<BundleYamlRequest>,
) -> Response {
    controller.apply_automation_bundle(project, request).await
}
async fn export_automation_bundle(
    Extension(controller): Extension<std::sync::Arc<BundleController>>,
    Path((project, bundle_key)): Path<(String, String)>,
) -> Response {
    controller
        .export_automation_bundle((project, bundle_key))
        .await
}
async fn list_installed_automation_bundles(
    Extension(controller): Extension<std::sync::Arc<BundleController>>,
    Path(project): Path<String>,
) -> Response {
    controller.list_installed_automation_bundles(project).await
}
async fn remove_automation_bundle(
    Extension(controller): Extension<std::sync::Arc<BundleController>>,
    Path((project, bundle_key)): Path<(String, String)>,
    Json(request): Json<RemoveAutomationBundleRequest>,
) -> Response {
    controller
        .remove_automation_bundle((project, bundle_key), request)
        .await
}
pub(crate) fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route(
            "/operator/api/automation/bundles/validate",
            post(validate_automation_bundle),
        )
        .route(
            "/operator/api/projects/{project}/automation/bundles/diff",
            post(diff_automation_bundle),
        )
        .route(
            "/operator/api/projects/{project}/automation/bundles/apply",
            post(apply_automation_bundle),
        )
        .route(
            "/operator/api/projects/{project}/automation/bundles",
            get(list_installed_automation_bundles),
        )
        .route(
            "/operator/api/projects/{project}/automation/bundles/{bundle_key}",
            axum::routing::delete(remove_automation_bundle),
        )
        .route(
            "/operator/api/projects/{project}/automation/bundles/{bundle_key}/export",
            get(export_automation_bundle),
        )
        .layer(axum::middleware::from_fn(controller_context))
}

impl BundleController {
    async fn diff_automation_bundle(
        self: std::sync::Arc<Self>,
        project: String,
        request: BundleYamlRequest,
    ) -> Response {
        json_result(self.bundles.diff(&project, &request.yaml).await)
    }
    async fn apply_automation_bundle(
        self: std::sync::Arc<Self>,
        project: String,
        request: BundleYamlRequest,
    ) -> Response {
        json_result(
            self.bundles
                .apply(
                    &project,
                    &request.yaml,
                    request.expected_current_hash.as_deref(),
                )
                .await,
        )
    }
    async fn export_automation_bundle(
        self: std::sync::Arc<Self>,
        (project, bundle_key): (String, String),
    ) -> Response {
        json_result(
            self.bundles
                .export(&project, &bundle_key)
                .await
                .map(|yaml| AutomationBundleExportView { yaml }),
        )
    }
    async fn list_installed_automation_bundles(
        self: std::sync::Arc<Self>,
        project: String,
    ) -> Response {
        json_result(self.bundles.list_installed(&project).await)
    }
    async fn remove_automation_bundle(
        self: std::sync::Arc<Self>,
        (project, bundle_key): (String, String),
        request: RemoveAutomationBundleRequest,
    ) -> Response {
        json_result(
            self.bundles
                .remove(
                    &project,
                    &bundle_key,
                    request.expected_current_hash.as_deref(),
                )
                .await,
        )
    }
}

async fn controller_context(
    Extension(state): Extension<AppState>,
    mut request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    request
        .extensions_mut()
        .insert(state.bundle_controller.clone());
    next.run(request).await
}
