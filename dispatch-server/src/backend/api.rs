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
    RoutingExplainRequest, UpdateProjectMemoryRequest, UpdateWorkItemLabelRequest,
    UpdateWorkItemRelationshipRequest, UpdateWorkItemRequest, WorkItemSearchRequest,
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
        .route("/api/projects/{project}", get(get_project))
        .route(
            "/api/projects/{project}/settings",
            get(get_project_settings),
        )
        .route(
            "/api/projects/{project}/memory",
            get(get_project_memory).put(set_project_memory),
        )
        .route(
            "/api/projects/{project}/memory/append",
            post(append_project_memory),
        )
        .route(
            "/api/projects/{project}/memory/events",
            get(list_project_memory_events),
        )
        .route(
            "/api/projects/{project}/memory/events/compact",
            post(compact_project_memory_events),
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

async fn get_project_memory(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(projects::get_memory(&state.store, &project).await)
}

async fn list_project_memory_events(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(projects::list_memory_events(&state.store, &project).await)
}

async fn set_project_memory(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<UpdateProjectMemoryRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.cross_check_agent_run_id(request.agent_run_id) {
        return json_result::<()>(Err(err));
    }
    json_result(
        projects::update_memory_with_source(
            &state.store,
            &project,
            request.body,
            projects::ProjectChangeSource::Agent {
                agent_id: request.agent_id,
                agent_run_id: request.agent_run_id,
            },
        )
        .await,
    )
}

async fn append_project_memory(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
    headers: HeaderMap,
    Json(request): Json<UpdateProjectMemoryRequest>,
) -> Response {
    let attribution = match RequestAttribution::from_headers(&state.store, &project, &headers).await
    {
        Ok(attribution) => attribution,
        Err(err) => return json_result::<()>(Err(err)),
    };
    if let Err(err) = attribution.cross_check_agent_id(&request.agent_id) {
        return json_result::<()>(Err(err));
    }
    if let Err(err) = attribution.cross_check_agent_run_id(request.agent_run_id) {
        return json_result::<()>(Err(err));
    }
    json_result(
        projects::append_memory_with_source(
            &state.store,
            &project,
            request.body,
            projects::ProjectChangeSource::Agent {
                agent_id: request.agent_id,
                agent_run_id: request.agent_run_id,
            },
        )
        .await,
    )
}

async fn compact_project_memory_events(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Response {
    json_result(projects::compact_memory_events(&state.store, &project).await)
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
    json_result(
        automation::read_run_log_with_active_session(
            &state.store,
            &state.sessions,
            &project,
            run_id,
        )
        .await,
    )
}

async fn active_sessions(
    Extension(state): Extension<AppState>,
    Path(project): Path<String>,
) -> Json<Vec<ProcessSessionView>> {
    Json(state.sessions.list_for_project(&project).await)
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use dispatch_types::{
        AUTOMATION_BLOCKED_LABEL_KEY, ClaimWorkItemResponse, CommentView,
        CreateWorkItemRelationshipRequest, DeleteWorkItemRelationshipResponse,
        FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView, ProjectMemoryCompactionView,
        ProjectMemoryEventView, ProjectMemoryUpdateView, ProjectMemoryView,
        UpdateWorkItemRelationshipRequest, WorkItemLabelView, WorkItemRelationshipDirection,
        WorkItemRelationshipListEntry, WorkItemRelationshipView, WorkItemView,
    };
    use serde::de::DeserializeOwned;
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::backend::{
        automation_controller::AutomationController,
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
        let state = AppState {
            store,
            sessions: ProcessSessionRegistry::new(),
            automation_controller: AutomationController::new(),
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

    async fn decode<T: DeserializeOwned>(response: Response<Body>) -> T {
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    async fn decode_error(response: Response<Body>) -> String {
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice::<ApiError>(&body).unwrap().error
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
        assert_eq!(claimed_item.id, item_id);
        assert_eq!(claimed_item.claimed_by.as_deref(), Some(agent_id.as_str()));

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
        assert_eq!(progress.body, "Working");

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
        assert_eq!(released.state.as_deref(), Some("open"));
        assert_eq!(released.claimed_by, None);
        assert!(
            released
                .labels
                .iter()
                .any(|label| label.key == AUTOMATION_BLOCKED_LABEL_KEY)
        );

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
        assert!(!claimed.claimed());

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
        assert_eq!(claimed.item.unwrap().id, feedback_item_id);

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
        assert_eq!(feedback_requested.state.as_deref(), Some("open"));
        assert_eq!(feedback_requested.claimed_by, None);
        assert!(
            feedback_requested
                .labels
                .iter()
                .any(|label| label.key == FEEDBACK_REQUESTED_LABEL_KEY)
        );

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
        assert_eq!(claimed.item.unwrap().id, finish_item_id);

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
        assert_eq!(finished.state.as_deref(), Some("done"));
        assert_eq!(finished.claimed_by, None);
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

        assert_eq!(updated.title, "Endpoint update");
        assert_eq!(updated.state.as_deref(), Some("review"));
        assert_eq!(updated.version, original.version + 1);
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
        assert_eq!(created_without_labels.state.as_deref(), Some("open"));
        assert_eq!(
            created_without_labels
                .labels
                .iter()
                .filter(|label| label.key != dispatch_types::STATE_LABEL_KEY)
                .count(),
            0
        );

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

        assert_eq!(created_with_labels.state.as_deref(), Some("review"));
        assert!(
            created_with_labels
                .labels
                .iter()
                .any(|label| { label.key == "type" && label.value.as_deref() == Some("feature") })
        );
        assert!(
            created_with_labels
                .labels
                .iter()
                .any(|label| label.key == "needs-verification" && label.value.is_none())
        );
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
        assert!(labels.iter().any(|label| label.key == "severity"));

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
        assert!(
            updated
                .labels
                .iter()
                .any(|label| { label.key == "priority" && label.value.as_deref() == Some("p1") })
        );

        let suggestions: Vec<ProjectLabelView> =
            decode(list_project_labels(Extension(state.clone()), Path("demo".to_owned())).await)
                .await;
        assert!(
            suggestions
                .iter()
                .any(|label| { label.key == "priority" && label.value.as_deref() == Some("p1") })
        );

        let deleted = delete_item_label(
            Extension(state),
            Path(("demo".to_owned(), item_id, label.id)),
            HeaderMap::new(),
            Query(LabelMutationQuery {
                expect_version: None,
            }),
        )
        .await;
        assert_eq!(deleted.status(), StatusCode::OK);
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
        assert_eq!(created.direction, WorkItemRelationshipDirection::Outgoing);
        assert_eq!(created.relationship.kind, "is follow-up of");

        let outgoing: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(
                Extension(state.clone()),
                Path(("demo".to_owned(), source_id)),
            )
            .await,
        )
        .await;
        assert_eq!(outgoing.len(), 1);
        assert_eq!(
            outgoing[0].direction,
            WorkItemRelationshipDirection::Outgoing
        );

        let incoming: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(
                Extension(state.clone()),
                Path(("demo".to_owned(), target_id)),
            )
            .await,
        )
        .await;
        assert_eq!(incoming.len(), 1);
        assert_eq!(
            incoming[0].direction,
            WorkItemRelationshipDirection::Incoming
        );

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
        assert!(duplicate.contains("duplicate relationship"));

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
        assert!(self_link.contains("must differ"));

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
        assert!(empty_kind.contains("kind cannot be empty"));

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
        assert_eq!(updated.kind, "unblocks");

        let deleted: DeleteWorkItemRelationshipResponse = decode(
            delete_relationship(
                Extension(state.clone()),
                Path(("demo".to_owned(), created.relationship.id)),
                HeaderMap::new(),
            )
            .await,
        )
        .await;
        assert!(deleted.deleted);

        let outgoing: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(Extension(state), Path(("demo".to_owned(), source_id))).await,
        )
        .await;
        assert!(outgoing.is_empty());
    }

    #[tokio::test]
    async fn memory_endpoints_snapshot_agent_changes_and_compact_history() {
        let (_temp, state, _item_id) = test_state().await;

        let set: ProjectMemoryUpdateView = decode(
            set_project_memory(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(UpdateProjectMemoryRequest {
                    agent_id: "dispatch-run-7".to_owned(),
                    agent_run_id: None,
                    body: "Remember the relay CLI.".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_eq!(set.project.memory, "Remember the relay CLI.");
        assert_eq!(set.event.operation, "set");
        assert_eq!(set.event.memory, "Remember the relay CLI.");
        assert_eq!(set.event.actor_type.as_deref(), Some("agent"));
        assert_eq!(set.event.actor_id.as_deref(), Some("dispatch-run-7"));
        assert_eq!(set.event.agent_run_id, Some(7));

        let appended: ProjectMemoryUpdateView = decode(
            append_project_memory(
                Extension(state.clone()),
                Path("demo".to_owned()),
                HeaderMap::new(),
                Json(UpdateProjectMemoryRequest {
                    agent_id: "dispatch-run-7".to_owned(),
                    agent_run_id: None,
                    body: "Use Dispatch memory commands.".to_owned(),
                }),
            )
            .await,
        )
        .await;
        assert_eq!(
            appended.project.memory,
            "Remember the relay CLI.\n\nUse Dispatch memory commands."
        );
        assert_eq!(appended.event.operation, "append");

        let current: ProjectMemoryView =
            decode(get_project_memory(Extension(state.clone()), Path("demo".to_owned())).await)
                .await;
        assert_eq!(current.last_event.unwrap().id, appended.event.id);

        let events: Vec<ProjectMemoryEventView> = decode(
            list_project_memory_events(Extension(state.clone()), Path("demo".to_owned())).await,
        )
        .await;
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, appended.event.id);

        let compacted: ProjectMemoryCompactionView = decode(
            compact_project_memory_events(Extension(state.clone()), Path("demo".to_owned())).await,
        )
        .await;
        assert_eq!(compacted.deleted_events, 2);

        let events: Vec<ProjectMemoryEventView> = decode(
            list_project_memory_events(Extension(state.clone()), Path("demo".to_owned())).await,
        )
        .await;
        assert!(events.is_empty());

        let current: ProjectMemoryView =
            decode(get_project_memory(Extension(state), Path("demo".to_owned())).await).await;
        assert_eq!(
            current.memory,
            "Remember the relay CLI.\n\nUse Dispatch memory commands."
        );
        assert!(current.last_event.is_none());
    }
}
