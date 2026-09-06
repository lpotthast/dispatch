use std::{convert::Infallible, time::Duration};

use async_stream::stream;
use axum::{
    Extension, Json, Router,
    extract::{
        Path, Query,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{HeaderMap, StatusCode},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
    routing::{get, post},
};
use dispatch_types::{
    AddCommentRequest, ApiError, AssignWorkItemGroupRequest, AutomationBundleExportView,
    AutomationBundleValidationView, AutomationPersonalityInput, AutomationRuleInput,
    BundleYamlRequest, ClaimWorkItemRequest, ClaimWorkItemResponse, CreateWorkItemGroupRequest,
    CreateWorkItemLabelRequest, CreateWorkItemRelationshipRequest, CreateWorkItemRequest,
    DEFAULT_STATE_LABEL, FinishWorkItemRequest, ProgressWorkItemRequest, ReleaseWorkItemRequest,
    RemoveAutomationBundleRequest, RequestFeedbackWorkItemRequest, RestoreRevisionRequest,
    RoutingExplainRequest, UpdateWorkItemLabelRequest, UpdateWorkItemRelationshipRequest,
    UpdateWorkItemRequest, WorkItemSearchRequest,
};
use futures_core::Stream;
use rootcause::Result;
use serde::{Deserialize, Serialize};

use crate::{
    backend::{
        app_state::AppState,
        automation, automation_bundles, automation_revisions, automation_routing,
        automation_triggers, comments,
        comments::AddComment,
        events, item_claims, item_label_service, items,
        items::{CreateWorkItem, UpdateWorkItem},
        personalities, projects, relationships,
        request_attribution::RequestAttribution,
        storage::Store,
        work_item_groups,
    },
    shared::view_models::ProcessSessionView,
};

pub(crate) fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route(
            "/api/projects/{project}/knowledge/{operation}",
            get(query_knowledge),
        )
        .route("/api/projects", get(list_projects))
        .route("/api/projects/{project}", get(get_project))
        .route(
            "/api/projects/{project}/settings",
            get(get_project_settings),
        )
        .route(
            "/api/projects/{project}/items",
            get(list_items).post(create_item),
        )
        .route("/api/projects/{project}/items/search", post(search_items))
        .route(
            "/api/projects/{project}/work-groups",
            get(list_work_groups).post(create_work_group),
        )
        .route(
            "/api/projects/{project}/work-groups/{group_key}/items",
            post(assign_work_group_items),
        )
        .route("/api/projects/{project}/labels", get(list_project_labels))
        .route("/api/projects/{project}/items/claim", post(claim_item))
        .route(
            "/api/projects/{project}/items/{item_id}",
            get(get_item).patch(update_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/labels",
            get(list_item_labels).post(add_item_label),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/labels/{label_id}",
            axum::routing::patch(update_item_label).delete(delete_item_label),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/relationships",
            get(list_item_relationships).post(create_item_relationship),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/relationships/{relationship_id}",
            axum::routing::patch(update_item_relationship).delete(delete_item_relationship),
        )
        .route(
            "/api/projects/{project}/relationships/{relationship_id}",
            axum::routing::patch(update_relationship).delete(delete_relationship),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/progress",
            post(progress_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/finish",
            post(finish_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/release",
            post(release_item),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/request-feedback",
            post(request_item_feedback),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/comments",
            get(list_comments).post(add_comment),
        )
        .route("/api/projects/{project}/automation/runs", get(list_runs))
        .route(
            "/api/projects/{project}/automation/triggers",
            get(list_automation_triggers),
        )
        .route(
            "/api/projects/{project}/automation/triggers/{id_or_key}",
            get(get_automation_trigger),
        )
        .route(
            "/api/projects/{project}/automation/routing/explain",
            post(explain_automation_routing),
        )
        .route(
            "/api/projects/{project}/automation/runs/{run_id}/log",
            get(get_run_log),
        )
        .route("/api/projects/{project}/events", get(project_events))
        .route(
            "/api/projects/{project}/automation/sessions",
            get(active_sessions),
        )
        .route(
            "/api/projects/{project}/items/{item_id}/events",
            get(item_events),
        )
        .route("/api/events/ws", get(ui_events_ws))
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
        .route(
            "/operator/api/projects/{project}/automation/revisions/{revision_id}/analytics",
            get(operator_revision_analytics),
        )
        .route(
            "/operator/api/projects/{project}/automation/evaluations",
            get(operator_list_evaluations),
        )
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
        .route(
            "/operator/api/projects/{project}/automation/routing/explain",
            post(explain_automation_routing),
        )
}

#[derive(Debug, Deserialize)]
struct ListItemsQuery {
    state: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LabelMutationQuery {
    expect_version: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct ListRunsQuery {
    limit: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct ListEvaluationsQuery {
    trigger_id: Option<i64>,
    limit: Option<u64>,
}

async fn list_projects(Extension(state): Extension<AppState>) -> Response {
    json_result(projects::list_projects(&state.store).await)
}

async fn get_project(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(projects::get_project(&state.store, &project).await)
}

async fn get_project_settings(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(projects::get_settings(&state.store, &project).await)
}

async fn list_items(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Query(query): Query<ListItemsQuery>,
) -> Response {
    json_result(items::list_items(&state.store, &project, query.state).await)
}

async fn list_project_labels(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(item_label_service::list_project_labels(&state.store, &project).await)
}

async fn search_items(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<WorkItemSearchRequest>,
) -> Response {
    let result = async {
        RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        items::search_items(&state.store, &project, request).await
    }
    .await;
    json_result(result)
}

async fn list_automation_triggers(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        automation_triggers::list_triggers(&state.store, &project).await
    }
    .await;
    json_result(result)
}

async fn get_automation_trigger(
    Extension(state): Extension<AppState>,
    Path((project, id_or_key)): Path<(String, String)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        automation_triggers::get_trigger(&state.store, &project, &id_or_key).await
    }
    .await;
    json_result(result)
}

async fn explain_automation_routing(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<RoutingExplainRequest>,
) -> Response {
    let result = async {
        RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        automation_routing::explain(&state.store, &project, request).await
    }
    .await;
    json_result(result)
}

async fn validate_automation_bundle(Json(request): Json<BundleYamlRequest>) -> Response {
    json_result(
        automation_bundles::validate_yaml(&request.yaml).map(|bundle| {
            AutomationBundleValidationView {
                manifest: bundle.manifest,
                manifest_hash: bundle.manifest_hash,
            }
        }),
    )
}

async fn diff_automation_bundle(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Json(request): Json<BundleYamlRequest>,
) -> Response {
    json_result(automation_bundles::diff_yaml(&state.store, &project, &request.yaml).await)
}

async fn apply_automation_bundle(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Json(request): Json<BundleYamlRequest>,
) -> Response {
    json_result(
        automation_bundles::apply_yaml(
            &state.store,
            &project,
            &request.yaml,
            request.expected_current_hash.as_deref(),
        )
        .await,
    )
}

async fn export_automation_bundle(
    Extension(state): Extension<AppState>,
    Path((project, bundle_key)): Path<(String, String)>,
) -> Response {
    json_result(
        automation_bundles::export_yaml(&state.store, &project, &bundle_key)
            .await
            .map(|yaml| AutomationBundleExportView { yaml }),
    )
}

async fn list_installed_automation_bundles(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(automation_bundles::list_installed(&state.store, &project).await)
}

async fn remove_automation_bundle(
    Extension(state): Extension<AppState>,
    Path((project, bundle_key)): Path<(String, String)>,
    Json(request): Json<RemoveAutomationBundleRequest>,
) -> Response {
    json_result(
        automation_bundles::remove_bundle(
            &state.store,
            &project,
            &bundle_key,
            request.expected_current_hash.as_deref(),
        )
        .await,
    )
}

async fn list_automation_revisions(
    Extension(state): Extension<AppState>,
    Path((project, trigger_id)): Path<(String, i64)>,
) -> Response {
    let result = async {
        let project_id = projects::project_id(&state.store, &project).await?;
        automation_revisions::list_trigger_revisions(&state.store, project_id, trigger_id).await
    }
    .await;
    json_result(result)
}

async fn operator_list_rules(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(automation_triggers::list_triggers(&state.store, &project).await)
}

async fn operator_get_rule(
    Extension(state): Extension<AppState>,
    Path((project, rule_id)): Path<(String, String)>,
) -> Response {
    json_result(automation_triggers::get_trigger(&state.store, &project, &rule_id).await)
}

async fn operator_create_rule(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Json(input): Json<AutomationRuleInput>,
) -> Response {
    json_result(automation_triggers::create_trigger_from_input(&state.store, &project, input).await)
}

async fn operator_update_rule(
    Extension(state): Extension<AppState>,
    Path((project, rule_id)): Path<(String, i64)>,
    Json(input): Json<AutomationRuleInput>,
) -> Response {
    json_result(
        automation_triggers::update_trigger_from_input(&state.store, &project, rule_id, input)
            .await,
    )
}

async fn operator_delete_rule(
    Extension(state): Extension<AppState>,
    Path((project, rule_id)): Path<(String, i64)>,
) -> Response {
    json_result(
        automation_triggers::delete_trigger(&state.store, &project, rule_id)
            .await
            .map(|()| serde_json::json!({ "deleted": true })),
    )
}

async fn operator_schedule_rule(
    Extension(state): Extension<AppState>,
    Path((project, rule_id)): Path<(String, i64)>,
) -> Response {
    json_result(
        automation_triggers::schedule_trigger_evaluation(&state.store, &project, rule_id).await,
    )
}

async fn operator_restore_rule(
    Extension(state): Extension<AppState>,
    Path((project, rule_id)): Path<(String, i64)>,
    Json(request): Json<RestoreRevisionRequest>,
) -> Response {
    let result = async {
        automation_revisions::restore_trigger_revision(
            &state.store,
            &project,
            rule_id,
            request.revision_id,
        )
        .await
        .and_then(automation_triggers::model_to_view)
    }
    .await;
    json_result(result)
}

async fn operator_detach_rule(
    Extension(state): Extension<AppState>,
    Path((project, rule_id)): Path<(String, i64)>,
) -> Response {
    json_result(automation_triggers::detach_trigger(&state.store, &project, rule_id).await)
}

async fn operator_revision_analytics(
    Extension(state): Extension<AppState>,
    Path((project, revision_id)): Path<(String, i64)>,
) -> Response {
    json_result(
        automation_revisions::trigger_revision_analytics(&state.store, &project, revision_id).await,
    )
}

async fn operator_list_evaluations(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Query(query): Query<ListEvaluationsQuery>,
) -> Response {
    json_result(
        automation_revisions::list_evaluations(
            &state.store,
            &project,
            query.trigger_id,
            query.limit.unwrap_or(100),
        )
        .await,
    )
}

async fn operator_list_personalities(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(personalities::list_personalities(&state.store, &project).await)
}

async fn operator_get_personality(
    Extension(state): Extension<AppState>,
    Path((project, personality_id)): Path<(String, String)>,
) -> Response {
    json_result(personalities::get_personality(&state.store, &project, &personality_id).await)
}

async fn operator_create_personality(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Json(input): Json<AutomationPersonalityInput>,
) -> Response {
    json_result(personalities::create_personality(&state.store, &project, input).await)
}

async fn operator_update_personality(
    Extension(state): Extension<AppState>,
    Path((project, personality_id)): Path<(String, i64)>,
    Json(input): Json<AutomationPersonalityInput>,
) -> Response {
    json_result(
        personalities::update_personality(&state.store, &project, personality_id, input).await,
    )
}

async fn operator_delete_personality(
    Extension(state): Extension<AppState>,
    Path((project, personality_id)): Path<(String, i64)>,
) -> Response {
    json_result(
        personalities::delete_personality(&state.store, &project, personality_id)
            .await
            .map(|()| serde_json::json!({ "deleted": true })),
    )
}

async fn operator_list_personality_revisions(
    Extension(state): Extension<AppState>,
    Path((project, personality_id)): Path<(String, i64)>,
) -> Response {
    let result = async {
        let project_id = projects::project_id(&state.store, &project).await?;
        automation_revisions::list_personality_revisions(&state.store, project_id, personality_id)
            .await
    }
    .await;
    json_result(result)
}

async fn operator_restore_personality(
    Extension(state): Extension<AppState>,
    Path((project, personality_id)): Path<(String, i64)>,
    Json(request): Json<RestoreRevisionRequest>,
) -> Response {
    json_result(
        automation_revisions::restore_personality_revision(
            &state.store,
            &project,
            personality_id,
            request.revision_id,
        )
        .await
        .map(dispatch_types::PersonalityView::from),
    )
}

async fn operator_detach_personality(
    Extension(state): Extension<AppState>,
    Path((project, personality_id)): Path<(String, i64)>,
) -> Response {
    json_result(personalities::detach_personality(&state.store, &project, personality_id).await)
}

async fn list_work_groups(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        work_item_groups::list_groups(&state.store, &project).await
    }
    .await;
    json_result(result)
}

async fn create_work_group(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemGroupRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("create work groups")?;
        work_item_groups::create_group(&state.store, &project, request, &attribution).await
    }
    .await;
    json_result(result)
}

async fn assign_work_group_items(
    Extension(state): Extension<AppState>,
    Path((project, group_key)): Path<(String, String)>,
    headers: HeaderMap,
    Json(request): Json<AssignWorkItemGroupRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("assign work-group items")?;
        work_item_groups::assign_items(
            &state.store,
            &project,
            &group_key,
            request.item_ids,
            &attribution,
        )
        .await
    }
    .await;
    json_result(result)
}

async fn create_item(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("create work items")?;
        items::create_item_with_attribution(
            &state.store,
            &project,
            CreateWorkItem {
                title: request.title,
                description: request.description,
                state: request
                    .state
                    .unwrap_or_else(|| DEFAULT_STATE_LABEL.to_owned()),
                agent_model_override: request.agent_model_override,
                agent_reasoning_effort_override: request.agent_reasoning_effort_override,
                initial_labels: request.initial_labels,
            },
            &attribution,
        )
        .await
    }
    .await;
    json_result(result)
}

async fn get_item(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        items::get_item(&state.store, &project, item_id).await
    }
    .await;
    json_result(result)
}

async fn update_item(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("update work items")?;
        items::update_item_with_attribution(
            &state.store,
            &project,
            item_id,
            UpdateWorkItem {
                title: request.title,
                description: request.description,
                state: request.state,
                agent_model_override: request.agent_model_override,
                agent_reasoning_effort_override: request.agent_reasoning_effort_override,
                expect_version: request.expect_version,
            },
            &attribution,
        )
        .await
    }
    .await;
    json_result(result)
}

async fn claim_item(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<ClaimWorkItemRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.ensure_generic_claim() {
        return json_result::<()>(Err(err));
    }
    json_result(
        item_claims::claim_item(&state.store, &project, &request.agent_id, &request.state)
            .await
            .map(|item| ClaimWorkItemResponse { item }),
    )
}

async fn progress_item(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<ProgressWorkItemRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.ensure_item_mutation("record item progress") {
        return json_result::<()>(Err(err));
    }
    json_result(
        item_claims::progress_item(
            &state.store,
            &project,
            item_id,
            &request.agent_id,
            &request.body,
        )
        .await,
    )
}

async fn finish_item(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<FinishWorkItemRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.ensure_item_mutation("finish work items") {
        return json_result::<()>(Err(err));
    }
    json_result(
        item_claims::finish_item(
            &state.store,
            &project,
            item_id,
            &request.agent_id,
            &request.report,
        )
        .await,
    )
}

async fn release_item(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<ReleaseWorkItemRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.ensure_item_mutation("release work items") {
        return json_result::<()>(Err(err));
    }
    json_result(
        item_claims::release_item(
            &state.store,
            &project,
            item_id,
            &request.agent_id,
            request.comment,
            item_claims::ReleaseAutomationDisposition::Blocked,
        )
        .await,
    )
}

async fn request_item_feedback(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<RequestFeedbackWorkItemRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.ensure_item_mutation("request item feedback") {
        return json_result::<()>(Err(err));
    }
    json_result(
        item_claims::request_feedback(
            &state.store,
            &project,
            item_id,
            &request.agent_id,
            &request.body,
        )
        .await,
    )
}

async fn list_comments(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    json_result(comments::list_comments(&state.store, &project, item_id).await)
}

async fn add_comment(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<AddCommentRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("add item comments")?;
        comments::add_comment_with_attribution(
            &state.store,
            &project,
            item_id,
            AddComment {
                author_type: request.author_type,
                author_name: request.author_name,
                body: request.body,
            },
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn list_runs(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Query(query): Query<ListRunsQuery>,
) -> Response {
    json_result(automation::list_runs(&state.store, &project, query.limit).await)
}

async fn get_run_log(
    Extension(state): Extension<AppState>,
    Path((project, run_id)): Path<(String, i64)>,
) -> Response {
    let result = automation::read_run_log_with_active_session(
        &state.store,
        &state.sessions,
        &project,
        run_id,
    )
    .await;
    json_result(result)
}

async fn active_sessions(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    let result: Result<Vec<ProcessSessionView>> = async {
        let project_id = projects::project_id(&state.store, &project).await?;
        Ok(state.sessions.list_for_project(project_id))
    }
    .await;
    json_result(result)
}

#[derive(Debug, Deserialize)]
struct EventsQuery {
    since: Option<i64>,
}

async fn project_events(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    event_stream(state.store.clone(), project, None, query.since)
}

async fn item_events(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    Query(query): Query<EventsQuery>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    event_stream(state.store.clone(), project, Some(item_id), query.since)
}

async fn ui_events_ws(ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(handle_ui_events_socket).into_response()
}

async fn handle_ui_events_socket(mut socket: WebSocket) {
    let mut receiver = events::subscribe();
    loop {
        match receiver.recv().await {
            Ok(event) => match serde_json::to_string(&event) {
                Ok(body) => {
                    if socket.send(Message::Text(body.into())).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    tracing::warn!("failed to serialize UI event: {err}");
                }
            },
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {
                continue;
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

fn event_stream(
    store: Store,
    project: String,
    item_id: Option<i64>,
    since: Option<i64>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let events = stream! {
        let mut last_id = since;
        loop {
            match items::list_events(&store, &project, item_id, last_id).await {
                Ok(new_events) => {
                    for event in new_events {
                        last_id = Some(event.id);
                        let response = Event::default()
                            .id(event.id.to_string())
                            .event(event.event_type.as_storage())
                            .json_data(&event)
                            .unwrap_or_else(|err| {
                                Event::default()
                                    .event("error")
                                    .data(format!("failed to serialize event: {err}"))
                            });
                        yield Ok(response);
                    }
                }
                Err(err) => {
                    yield Ok(Event::default().event("error").data(err.to_string()));
                }
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    };

    Sse::new(events).keep_alive(KeepAlive::default())
}

fn json_result<T>(result: Result<T>) -> Response
where
    T: Serialize,
{
    match result {
        Ok(value) => Json(value).into_response(),
        Err(err) => (
            StatusCode::BAD_REQUEST,
            Json(ApiError {
                error: err.to_string(),
                code: None,
                details: None,
            }),
        )
            .into_response(),
    }
}

async fn list_item_labels(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    json_result(item_label_service::list_item_labels(&state.store, &project, item_id).await)
}

async fn add_item_label(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Query(query): Query<LabelMutationQuery>,
    Json(request): Json<CreateWorkItemLabelRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("add item labels")?;
        item_label_service::add_label_with_attribution(
            &state.store,
            &project,
            item_id,
            request.key,
            request.value,
            query.expect_version,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn update_item_label(
    Extension(state): Extension<AppState>,
    Path((project, item_id, label_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemLabelRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("update item labels")?;
        item_label_service::update_label_with_attribution(
            &state.store,
            &project,
            item_id,
            item_label_service::UpdateLabelInput {
                label_id,
                key: request.key,
                value: request.value,
                expect_version: request.expect_version,
            },
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn delete_item_label(
    Extension(state): Extension<AppState>,
    Path((project, item_id, label_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Query(query): Query<LabelMutationQuery>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("delete item labels")?;
        item_label_service::delete_label_with_attribution(
            &state.store,
            &project,
            item_id,
            label_id,
            query.expect_version,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn list_item_relationships(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
) -> Response {
    json_result(relationships::list_item_relationships(&state.store, &project, item_id).await)
}

async fn create_item_relationship(
    Extension(state): Extension<AppState>,
    Path((project, item_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<CreateWorkItemRelationshipRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("create item relationships")?;
        relationships::create_relationship_with_attribution(
            &state.store,
            &project,
            item_id,
            request.target_work_item_id,
            request.kind,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn update_relationship(
    Extension(state): Extension<AppState>,
    Path((project, relationship_id)): Path<(String, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemRelationshipRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("update item relationships")?;
        relationships::update_relationship_with_attribution(
            &state.store,
            &project,
            relationship_id,
            request.kind,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn delete_relationship(
    Extension(state): Extension<AppState>,
    Path((project, relationship_id)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("delete item relationships")?;
        relationships::delete_relationship_with_attribution(
            &state.store,
            &project,
            relationship_id,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn update_item_relationship(
    Extension(state): Extension<AppState>,
    Path((project, item_id, relationship_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
    Json(request): Json<UpdateWorkItemRelationshipRequest>,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("update item relationships")?;
        relationships::update_relationship_for_item_with_attribution(
            &state.store,
            &project,
            item_id,
            relationship_id,
            request.kind,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn delete_item_relationship(
    Extension(state): Extension<AppState>,
    Path((project, item_id, relationship_id)): Path<(String, i64, i64)>,
    headers: HeaderMap,
) -> Response {
    let result = async {
        let attribution =
            RequestAttribution::from_headers(&state.store, &project, &headers).await?;
        attribution.ensure_item_mutation("delete item relationships")?;
        relationships::delete_relationship_for_item_with_attribution(
            &state.store,
            &project,
            item_id,
            relationship_id,
            attribution.event(),
        )
        .await
    }
    .await;
    json_result(result)
}

async fn query_knowledge(
    Extension(state): Extension<AppState>,
    Path((project, operation)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<dispatch_types::knowledge::KnowledgeQuery>,
) -> Response {
    use dispatch_types::knowledge::KnowledgeOperation;
    let operation = match operation.as_str() {
        "root" => KnowledgeOperation::Root,
        "node" => KnowledgeOperation::Node,
        "search" => KnowledgeOperation::Search,
        "check" => KnowledgeOperation::Check,
        "documents" => KnowledgeOperation::List,
        "graph" => KnowledgeOperation::Graph,
        _ => return StatusCode::NOT_FOUND.into_response(),
    };
    let result = async {
        let attribution =
            RequestAttribution::from_knowledge_headers(&state.store, &project, &headers).await?;
        crate::backend::knowledge::query(&state.store, &project, &attribution, operation, query)
            .await
    }
    .await;
    json_result(result)
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::http::HeaderValue;
    use dispatch_types::{
        AUTOMATION_BLOCKED_LABEL_KEY, AgentRunKind, AgentRunPurposeV1, AgentRunStatus,
        AutomationRunMutability, ClaimWorkItemResponse, CommentView,
        CreateWorkItemRelationshipRequest, DeleteWorkItemRelationshipResponse,
        FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView, ProjectView,
        UpdateWorkItemRelationshipRequest, WorkItemLabelView, WorkItemRelationshipDirection,
        WorkItemRelationshipListEntry, WorkItemRelationshipView, WorkItemView,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, TransactionTrait};
    use serde::de::DeserializeOwned;
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::backend::{
        agent_ids,
        agent_run_launch::{
            AgentCapabilitySetV1, AgentLaunchTargetV1, insert_contract_in_tx, mark_spawned_in_tx,
            mark_terminal_in_tx,
        },
        automation_controller::AutomationController,
        entities::agent_run::AgentRunActiveModel,
        process_sessions::ProcessSessionRegistry,
        projects::{CreateProject, create_project},
        storage::{Store, utc_now},
    };

    async fn test_state() -> (TempDir, AppState, i64) {
        let temp = tempdir().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        create_project(
            &store,
            CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            },
        )
        .await
        .unwrap();
        let item = items::create_item(
            &store,
            "demo",
            CreateWorkItem {
                title: "Endpoint work".to_owned(),
                description: "Exercise workflow API endpoints".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap();
        let sessions = ProcessSessionRegistry::new();
        let automation_controller = AutomationController::new();
        let project_deletion = crate::backend::project_deletion::ProjectDeletionService::new(
            store.clone(),
            automation_controller.clone(),
            sessions.clone(),
        );
        let state = AppState {
            store: store.clone(),
            sessions: sessions.clone(),
            automation_controller,
            project_deletion,
            codex_status: Arc::new(tokio::sync::RwLock::new(
                dispatch_types::CodexAppServerStatusView {
                    available: true,
                    usable: true,
                    message: String::new(),
                    install_prompt: String::new(),
                    checked_at: utc_now(),
                    ..Default::default()
                },
            )),
            codex_status_refresh: crate::backend::codex_app_server::CodexStatusRefresh::default(),
        };
        (temp, state, item.id)
    }

    async fn ordinary_run_headers(
        state: &AppState,
        mutability: AutomationRunMutability,
        status: AgentRunStatus,
        resolved: bool,
    ) -> HeaderMap {
        let project = projects::find_project_by_name(&state.store, "demo")
            .await
            .unwrap();
        let project_id = project.id;
        let working_dir = project.path.unwrap();
        let now = utc_now();
        let txn = state.store.db().begin().await.unwrap();
        let run = AgentRunActiveModel {
            project_id: Set(project_id),
            run_kind: Set(AgentRunKind::Task.as_storage().to_owned()),
            purpose: Set(Some(AgentRunPurposeV1::Ordinary.as_storage().to_owned())),
            tool_name: Set("codex".to_owned()),
            mutability: Set(mutability.as_storage().to_owned()),
            status: Set(status.as_storage().to_owned()),
            command: Set(String::new()),
            working_dir: Set(working_dir),
            created_at: Set(now.clone()),
            updated_at: Set(now.clone()),
            ..Default::default()
        }
        .insert(&txn)
        .await
        .unwrap();
        let target = if resolved {
            AgentLaunchTargetV1::none()
        } else {
            AgentLaunchTargetV1::next_open("open").unwrap()
        };
        insert_contract_in_tx(
            &txn,
            project_id,
            run.id,
            AgentRunPurposeV1::Ordinary,
            &target,
            &AgentCapabilitySetV1::ordinary(),
            &now,
        )
        .await
        .unwrap();
        if resolved {
            mark_spawned_in_tx(&txn, project_id, run.id, &now)
                .await
                .unwrap();
            if status != AgentRunStatus::Running {
                mark_terminal_in_tx(&txn, project_id, run.id, &now)
                    .await
                    .unwrap();
            }
        }
        txn.commit().await.unwrap();

        let mut headers = HeaderMap::new();
        headers.insert(
            crate::backend::request_attribution::AGENT_ID_HEADER,
            HeaderValue::from_str(&agent_ids::dispatch_run_agent_id(run.id)).unwrap(),
        );
        headers.insert(
            crate::backend::request_attribution::AGENT_RUN_ID_HEADER,
            HeaderValue::from_str(&run.id.to_string()).unwrap(),
        );
        headers
    }

    async fn decode<T: DeserializeOwned>(response: Response<Body>) -> T {
        let status = response.status();
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        if status != StatusCode::OK {
            eprintln!(
                "unexpected API response: {}",
                String::from_utf8_lossy(&body)
            );
        }
        assert_that!(&status).is_equal_to(StatusCode::OK);
        serde_json::from_slice(&body).unwrap()
    }

    async fn decode_error(response: Response<Body>) -> String {
        assert_that!(&(response.status())).is_equal_to(StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice::<ApiError>(&body).unwrap().error
    }

    #[tokio::test]
    async fn operator_rule_endpoints_reject_invalid_policy_without_writing() {
        use dispatch_types::AutomationTriggerView;

        let (_temp, state, _) = test_state().await;
        let mut input: AutomationRuleInput = serde_json::from_value(serde_json::json!({
            "name": "Rule policy", "enabled": true, "activation": "work_item",
            "effect": "consume_work", "schedule": "15s", "produced_work": {},
        }))
        .unwrap();
        let error = decode_error(
            operator_create_rule(
                Extension(state.clone()),
                Path("demo".to_owned()),
                Json(input.clone()),
            )
            .await,
        )
        .await;
        assert_that!(&error).contains("only valid for produce_work");
        input.produced_work = None;
        let created: AutomationTriggerView = decode(
            operator_create_rule(
                Extension(state.clone()),
                Path("demo".to_owned()),
                Json(input.clone()),
            )
            .await,
        )
        .await;
        assert_that!(&created.work_item_selector.is_some()).is_true();
        assert_that!(&created.personality_id.is_some()).is_true();

        input.execution.model = Some("unknown-model".to_owned());
        let error = decode_error(
            operator_update_rule(
                Extension(state.clone()),
                Path(("demo".to_owned(), created.id)),
                Json(input.clone()),
            )
            .await,
        )
        .await;
        assert_that!(&error).contains("automation model override must be one of");
        let unchanged: AutomationTriggerView = decode(
            operator_get_rule(
                Extension(state.clone()),
                Path(("demo".to_owned(), created.id.to_string())),
            )
            .await,
        )
        .await;
        assert_that!(&serde_json::to_value(unchanged).unwrap())
            .is_equal_to(serde_json::to_value(&created).unwrap());

        input.execution.model = None;
        input.name = "Updated rule policy".to_owned();
        let updated: AutomationTriggerView = decode(
            operator_update_rule(
                Extension(state),
                Path(("demo".to_owned(), created.id)),
                Json(input),
            )
            .await,
        )
        .await;
        assert_that!(&updated.name).is_equal_to("Updated rule policy");
        assert_that!(&updated.current_revision_id).is_not_equal_to(created.current_revision_id);
    }

    #[tokio::test]
    async fn knowledge_queries_read_plain_files_and_retire_signed_routes() {
        use dispatch_types::knowledge::{KnowledgeQuery, KnowledgeView};
        let (temp, state, _) = test_state().await;
        std::fs::create_dir(temp.path().join("knowledge")).unwrap();
        std::fs::write(
            temp.path().join("knowledge/README.md"),
            "---\nid: project\n---\n# Project\n\nPlain Markdown overview.\n",
        )
        .unwrap();
        let headers = ordinary_run_headers(
            &state,
            AutomationRunMutability::Mutating,
            AgentRunStatus::Running,
            true,
        )
        .await;
        let view: KnowledgeView = decode(
            query_knowledge(
                Extension(state.clone()),
                Path(("demo".into(), "root".into())),
                headers.clone(),
                Query(KnowledgeQuery::default()),
            )
            .await,
        )
        .await;
        assert_that!(&view.document.unwrap().markdown).contains("Plain Markdown overview");
        let response = query_knowledge(
            Extension(state),
            Path(("demo".into(), "integrity".into())),
            headers,
            Query(KnowledgeQuery::default()),
        )
        .await;
        assert_that!(&response.status()).is_equal_to(StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn project_list_endpoint_returns_available_projects() {
        let (_temp, state, _item_id) = test_state().await;

        let projects: Vec<ProjectView> = decode(list_projects(Extension(state)).await).await;

        assert_that!(&(projects.len())).is_equal_to(1);
        assert_that!(&(projects[0].name.as_str())).is_equal_to("demo");
        assert_that!(&(projects[0].display_name.as_str())).is_equal_to("demo");
    }

    #[tokio::test]
    async fn workflow_endpoints_claim_progress_release_and_finish() {
        let (_temp, state, item_id) = test_state().await;
        let agent_id = "dispatch-run-1".to_owned();

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(ClaimWorkItemRequest {
                    agent_id: agent_id.clone(),
                    state: "open".to_owned(),
                }),
            )
            .await,
        )
        .await;
        let claimed_item = claimed.item.unwrap();
        assert_that!(&(claimed_item.id)).is_equal_to(item_id);
        assert_that!(&(claimed_item.claimed_by.as_deref())).is_equal_to(Some(agent_id.as_str()));

        let progress: CommentView = decode(
            progress_item(
                Extension(state.clone()),
                Path(("demo".to_owned(), item_id)),
                HeaderMap::new(),
                Json(ProgressWorkItemRequest {
                    agent_id: agent_id.clone(),
                    body: "Working".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(progress.body)).is_equal_to("Working");

        let released: WorkItemView = decode(
            release_item(
                Extension(state.clone()),
                Path(("demo".to_owned(), item_id)),
                HeaderMap::new(),
                Json(ReleaseWorkItemRequest {
                    agent_id: agent_id.clone(),
                    comment: Some("Paused".to_owned()),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(released.state.as_deref())).is_equal_to(Some("open"));
        assert_that!(&(released.claimed_by)).is_equal_to(None);
        assert_that!(
            &(released
                .labels
                .iter()
                .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY))
        )
        .is_true();

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(ClaimWorkItemRequest {
                    agent_id: agent_id.clone(),
                    state: "open".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(!claimed.claimed())).is_true();

        let feedback_item_id = items::create_item(
            &state.store,
            "demo",
            CreateWorkItem {
                title: "Endpoint feedback".to_owned(),
                description: "Exercise feedback request endpoint".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap()
        .id;

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(ClaimWorkItemRequest {
                    agent_id: agent_id.clone(),
                    state: "open".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(claimed.item.unwrap().id)).is_equal_to(feedback_item_id);

        let feedback_requested: WorkItemView = decode(
            request_item_feedback(
                Extension(state.clone()),
                Path(("demo".to_owned(), feedback_item_id)),
                HeaderMap::new(),
                Json(RequestFeedbackWorkItemRequest {
                    agent_id: agent_id.clone(),
                    body: "Need a user decision".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(feedback_requested.state.as_deref())).is_equal_to(Some("open"));
        assert_that!(&(feedback_requested.claimed_by)).is_equal_to(None);
        assert_that!(
            &(feedback_requested
                .labels
                .iter()
                .any(|label| label.key == FEEDBACK_REQUESTED_LABEL_KEY))
        )
        .is_true();

        let finish_item_id = items::create_item(
            &state.store,
            "demo",
            CreateWorkItem {
                title: "Endpoint finish".to_owned(),
                description: "Exercise finish endpoint".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap()
        .id;

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(ClaimWorkItemRequest {
                    agent_id: agent_id.clone(),
                    state: "open".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(claimed.item.unwrap().id)).is_equal_to(finish_item_id);

        let finished: WorkItemView = decode(
            finish_item(
                Extension(state),
                Path(("demo".to_owned(), finish_item_id)),
                HeaderMap::new(),
                Json(FinishWorkItemRequest {
                    agent_id,
                    report: "Done".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(finished.state.as_deref())).is_equal_to(Some("done"));
        assert_that!(&(finished.claimed_by)).is_equal_to(None);
    }

    #[tokio::test]
    async fn update_endpoint_applies_fields_and_state_as_one_patch() {
        let (_temp, state, item_id) = test_state().await;
        let original = items::get_item(&state.store, "demo", item_id)
            .await
            .unwrap();

        let updated: WorkItemView = decode(
            update_item(
                Extension(state),
                Path(("demo".to_owned(), item_id)),
                HeaderMap::new(),
                Json(UpdateWorkItemRequest {
                    title: Some("Endpoint update".to_owned()),
                    description: None,
                    state: Some("review".to_owned()),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    expect_version: Some(original.version),
                }),
            )
            .await,
        )
        .await;

        assert_that!(&(updated.title)).is_equal_to("Endpoint update");
        assert_that!(&(updated.state.as_deref())).is_equal_to(Some("review"));
        assert_that!(&(updated.version)).is_equal_to(original.version + 1);
    }

    #[tokio::test]
    async fn create_endpoint_defaults_missing_labels_and_accepts_initial_labels() {
        let (_temp, state, _item_id) = test_state().await;
        let backwards_compatible: CreateWorkItemRequest =
            serde_json::from_value(serde_json::json!({
                "title": "No labels request",
                "description": "Older client payload",
                "state": "open",
                "agent_model_override": null,
                "agent_reasoning_effort_override": null
            }))
            .unwrap();

        let created_without_labels: WorkItemView = decode(
            create_item(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(backwards_compatible),
            )
            .await,
        )
        .await;
        assert_that!(&(created_without_labels.state.as_deref())).is_equal_to(Some("open"));
        assert_that!(
            &(created_without_labels
                .labels
                .iter()
                .filter(|label| label.key != dispatch_types::STATE_LABEL_KEY)
                .count())
        )
        .is_equal_to(0);

        let created_with_labels: WorkItemView = decode(
            create_item(
                Extension(state),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(CreateWorkItemRequest {
                    title: "Initial labels request".to_owned(),
                    description: "New client payload".to_owned(),
                    state: Some("review".to_owned()),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: vec![
                        CreateWorkItemLabelRequest {
                            key: "type".to_owned(),
                            value: Some("feature".to_owned()),
                        },
                        CreateWorkItemLabelRequest {
                            key: "needs-verification".to_owned(),
                            value: None,
                        },
                    ],
                }),
            )
            .await,
        )
        .await;

        assert_that!(&(created_with_labels.state.as_deref())).is_equal_to(Some("review"));
        assert_that!(
            &(created_with_labels
                .labels
                .iter()
                .any(|label| { label.key == "type" && label.value.as_deref() == Some("feature") }))
        )
        .is_true();
        assert_that!(
            &(created_with_labels
                .labels
                .iter()
                .any(|label| label.key == "needs-verification" && label.value.is_none()))
        )
        .is_true();
    }

    #[tokio::test]
    async fn label_endpoints_add_update_delete_and_suggest() {
        let (_temp, state, item_id) = test_state().await;

        let labeled: WorkItemView = decode(
            add_item_label(
                Extension(state.clone()),
                Path(("demo".to_owned(), item_id)),
                HeaderMap::new(),
                Query(LabelMutationQuery {
                    expect_version: None,
                }),
                Json(CreateWorkItemLabelRequest {
                    key: "severity".to_owned(),
                    value: Some("high".to_owned()),
                }),
            )
            .await,
        )
        .await;
        let label = labeled
            .labels
            .iter()
            .find(|label| label.key == "severity")
            .cloned()
            .unwrap();

        let labels: Vec<WorkItemLabelView> = decode(
            list_item_labels(Extension(state.clone()), Path(("demo".to_owned(), item_id))).await,
        )
        .await;
        assert_that!(&(labels.iter().any(|label| label.key == "severity"))).is_true();

        let updated: WorkItemView = decode(
            update_item_label(
                Extension(state.clone()),
                Path(("demo".to_owned(), item_id, label.id)),
                HeaderMap::new(),
                Json(UpdateWorkItemLabelRequest {
                    key: Some("priority".to_owned()),
                    value: Some(Some("p1".to_owned())),
                    expect_version: None,
                }),
            )
            .await,
        )
        .await;
        assert_that!(
            &(updated
                .labels
                .iter()
                .any(|label| { label.key == "priority" && label.value.as_deref() == Some("p1") }))
        )
        .is_true();

        let suggestions: Vec<ProjectLabelView> =
            decode(list_project_labels(Extension(state.clone()), Path("demo".to_owned())).await)
                .await;
        assert_that!(
            &(suggestions
                .iter()
                .any(|label| { label.key == "priority" && label.value.as_deref() == Some("p1") }))
        )
        .is_true();

        let deleted = delete_item_label(
            Extension(state),
            Path(("demo".to_owned(), item_id, label.id)),
            HeaderMap::new(),
            Query(LabelMutationQuery {
                expect_version: None,
            }),
        )
        .await;
        assert_that!(&(deleted.status())).is_equal_to(StatusCode::OK);
    }

    #[tokio::test]
    async fn relationship_endpoints_create_list_update_delete_and_validate() {
        let (_temp, state, source_id) = test_state().await;
        let target_id = items::create_item(
            &state.store,
            "demo",
            CreateWorkItem {
                title: "Relationship target".to_owned(),
                description: "Receives a relationship".to_owned(),
                state: "open".to_owned(),
                agent_model_override: None,
                agent_reasoning_effort_override: None,
                initial_labels: Vec::new(),
            },
        )
        .await
        .unwrap()
        .id;

        let created: WorkItemRelationshipListEntry = decode(
            create_item_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), source_id)),
                HeaderMap::new(),
                Json(CreateWorkItemRelationshipRequest {
                    target_work_item_id: target_id,
                    kind: " is follow-up of ".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(created.direction)).is_equal_to(WorkItemRelationshipDirection::Outgoing);
        assert_that!(&(created.relationship.kind)).is_equal_to("is follow-up of");

        let outgoing: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(
                Extension(state.clone()),
                Path(("demo".to_owned(), source_id)),
            )
            .await,
        )
        .await;
        assert_that!(&(outgoing.len())).is_equal_to(1);
        assert_that!(&(outgoing[0].direction)).is_equal_to(WorkItemRelationshipDirection::Outgoing);

        let incoming: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(
                Extension(state.clone()),
                Path(("demo".to_owned(), target_id)),
            )
            .await,
        )
        .await;
        assert_that!(&(incoming.len())).is_equal_to(1);
        assert_that!(&(incoming[0].direction)).is_equal_to(WorkItemRelationshipDirection::Incoming);

        let duplicate = decode_error(
            create_item_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), source_id)),
                HeaderMap::new(),
                Json(CreateWorkItemRelationshipRequest {
                    target_work_item_id: target_id,
                    kind: "is follow-up of".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(duplicate.contains("duplicate relationship"))).is_true();

        let self_link = decode_error(
            create_item_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), source_id)),
                HeaderMap::new(),
                Json(CreateWorkItemRelationshipRequest {
                    target_work_item_id: source_id,
                    kind: "relates".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(self_link.contains("must differ"))).is_true();

        let empty_kind = decode_error(
            create_item_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), source_id)),
                HeaderMap::new(),
                Json(CreateWorkItemRelationshipRequest {
                    target_work_item_id: target_id,
                    kind: " ".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(empty_kind.contains("kind cannot be empty"))).is_true();

        let updated: WorkItemRelationshipView = decode(
            update_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), created.relationship.id)),
                HeaderMap::new(),
                Json(UpdateWorkItemRelationshipRequest {
                    kind: "unblocks".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_that!(&(updated.kind)).is_equal_to("unblocks");

        let deleted: DeleteWorkItemRelationshipResponse = decode(
            delete_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), created.relationship.id)),
                HeaderMap::new(),
            )
            .await,
        )
        .await;
        assert_that!(&(deleted.deleted)).is_true();

        let outgoing: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(Extension(state), Path(("demo".to_owned(), source_id))).await,
        )
        .await;
        assert_that!(&(outgoing.is_empty())).is_true();
    }
}
