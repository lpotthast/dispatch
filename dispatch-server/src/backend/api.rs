use axum::{
    Json, Router,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use dispatch_types::ApiError;
use rootcause::Result;
use serde::Serialize;

pub(crate) fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .merge(super::runs::transport::api::routes())
        .merge(super::automation::routing::transport::routes())
        .merge(super::automation::bundles::transport::routes())
        .merge(super::automation::revisions::transport::routes())
        .merge(super::automation::rules::transport::api::routes())
        .merge(crate::backend::relationships::transport::api::routes())
        .merge(crate::backend::comments::transport::api::routes())
        .merge(crate::backend::items::transport::api::routes())
        .merge(crate::backend::items::labels::transport::routes())
        .merge(crate::backend::items::groups::transport::routes())
        .merge(crate::backend::knowledge::jobs::api::routes())
        .merge(crate::backend::knowledge::transport::routes())
        .merge(crate::backend::projects::transport::api::routes())
        .merge(crate::backend::items::claims::transport::routes())
        .merge(super::events::transport::routes())
        .merge(super::automation::personalities::transport::api::routes())
}

pub(crate) fn json_result<T>(result: Result<T>) -> Response
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

#[cfg(test)]
mod tests {
    use crate::backend::app_state::AppState;
    use crate::backend::items::claims::transport::{
        claim_item, finish_item, progress_item, release_item, request_item_feedback,
    };
    use crate::backend::items::labels::transport::{
        LabelMutationQuery, add_item_label, delete_item_label, list_item_labels,
        list_project_labels, update_item_label,
    };
    use crate::backend::items::{
        CreateWorkItem,
        transport::api::{create_item, update_item},
    };
    use crate::backend::knowledge::transport::query_knowledge;
    use crate::backend::relationships::transport::api::*;
    use assertr::prelude::*;
    use axum::Extension;
    use axum::{
        extract::{Path, Query},
        http::HeaderMap,
    };
    use dispatch_types::{
        ClaimWorkItemRequest, FinishWorkItemRequest, ProgressWorkItemRequest,
        ReleaseWorkItemRequest, RequestFeedbackWorkItemRequest,
    };
    use dispatch_types::{CreateWorkItemLabelRequest, UpdateWorkItemLabelRequest};
    use dispatch_types::{CreateWorkItemRelationshipRequest, UpdateWorkItemRelationshipRequest};
    use dispatch_types::{CreateWorkItemRequest, UpdateWorkItemRequest};

    use axum::body::{Body, to_bytes};
    use axum::http::HeaderValue;
    use dispatch_types::{
        AUTOMATION_BLOCKED_LABEL_KEY, AgentRunKind, AgentRunPurposeV1, AgentRunStatus,
        AutomationRunMutability, ClaimWorkItemResponse, CommentView,
        DeleteWorkItemRelationshipResponse, FEEDBACK_REQUESTED_LABEL_KEY, ProjectLabelView,
        ProjectView, WorkItemLabelView, WorkItemRelationshipDirection,
        WorkItemRelationshipListEntry, WorkItemRelationshipView, WorkItemView,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set, TransactionTrait};
    use serde::de::DeserializeOwned;
    use tempfile::{TempDir, tempdir};

    use super::*;
    use crate::backend::{
        entities::agent_run::AgentRunActiveModel,
        execution::identity as agent_ids,
        projects::CreateProject,
        runs::launch::model::{AgentCapabilitySetV1, AgentLaunchTargetV1},
        runs::launch::repository::{
            insert_contract_in_tx, mark_spawned_in_tx, mark_terminal_in_tx,
        },
        storage::{Store, utc_now},
    };

    async fn test_state() -> (TempDir, AppState, i64) {
        let event_bus = crate::backend::events::UiEventBus::new();

        let temp = tempdir().unwrap();
        let store = Store::open(temp.path().join("dispatch.sqlite3"))
            .await
            .unwrap();
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
        let item = crate::backend::items::creation::tests::service(&store, event_bus.clone())
            .create(
                crate::backend::projects::ProjectReference::Name("demo"),
                CreateWorkItem {
                    title: "Endpoint work".to_owned(),
                    description: "Exercise workflow API endpoints".to_owned(),
                    state: "open".to_owned(),
                    agent_model_override: None,
                    agent_reasoning_effort_override: None,
                    initial_labels: Vec::new(),
                },
                Default::default(),
            )
            .await
            .unwrap();
        let application = crate::backend::application::Application::from_store(
            store.clone(),
            "http://127.0.0.1:4000".into(),
        );
        let state = application.state.clone();

        (temp, state, item.id)
    }

    async fn ordinary_run_headers(
        state: &AppState,
        mutability: AutomationRunMutability,
        status: AgentRunStatus,
        resolved: bool,
    ) -> HeaderMap {
        let project = state.projects.get("demo").await.unwrap();
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
            crate::backend::attribution::transport::AGENT_ID_HEADER,
            HeaderValue::from_str(&agent_ids::dispatch_run_agent_id(run.id)).unwrap(),
        );
        headers.insert(
            crate::backend::attribution::transport::AGENT_RUN_ID_HEADER,
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
        use crate::backend::automation::rules::transport::api::{
            operator_create_rule, operator_get_rule, operator_update_rule,
        };
        use dispatch_types::AutomationRuleInput;
        use dispatch_types::AutomationTriggerView;

        let (_temp, state, _) = test_state().await;
        let mut input: AutomationRuleInput = serde_json::from_value(serde_json::json!({
            "name": "Rule policy", "enabled": true, "activation": "work_item",
            "effect": "consume_work", "schedule": "15s", "produced_work": {},
        }))
        .unwrap();
        let error = decode_error(
            operator_create_rule(
                Extension((state.clone()).rule_controller.clone()),
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
                Extension((state.clone()).rule_controller.clone()),
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
                Extension((state.clone()).rule_controller.clone()),
                Path(("demo".to_owned(), created.id)),
                Json(input.clone()),
            )
            .await,
        )
        .await;
        assert_that!(&error).contains("automation model override must be one of");
        let unchanged: AutomationTriggerView = decode(
            operator_get_rule(
                Extension((state.clone()).rule_controller.clone()),
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
                Extension((state).rule_controller.clone()),
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
                Extension((state.clone()).knowledge_controller.clone()),
                Path(("demo".into(), "root".into())),
                headers.clone(),
                Query(KnowledgeQuery::default()),
            )
            .await,
        )
        .await;
        assert_that!(&view.document.unwrap().markdown).contains("Plain Markdown overview");
        let response = query_knowledge(
            Extension((state).knowledge_controller.clone()),
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

        let projects: Vec<ProjectView> = decode(
            crate::backend::projects::transport::api::list_projects(Extension(
                (state).project_controller.clone(),
            ))
            .await,
        )
        .await;

        assert_that!(&(projects.len())).is_equal_to(1);
        assert_that!(&(projects[0].name.as_str())).is_equal_to("demo");
        assert_that!(&(projects[0].display_name.as_str())).is_equal_to("demo");
    }

    #[tokio::test]
    async fn workflow_endpoints_claim_progress_release_and_finish() {
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, state, item_id) = test_state().await;
        let agent_id = "dispatch-run-1".to_owned();

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension((state.clone()).claim_controller.clone()),
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
                Extension((state.clone()).claim_controller.clone()),
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
                Extension((state.clone()).claim_controller.clone()),
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
                Extension((state.clone()).claim_controller.clone()),
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

        let feedback_item_id =
            crate::backend::items::creation::tests::service(&state.store, event_bus.clone())
                .create(
                    crate::backend::projects::ProjectReference::Name("demo"),
                    CreateWorkItem {
                        title: "Endpoint feedback".to_owned(),
                        description: "Exercise feedback request endpoint".to_owned(),
                        state: "open".to_owned(),
                        agent_model_override: None,
                        agent_reasoning_effort_override: None,
                        initial_labels: Vec::new(),
                    },
                    Default::default(),
                )
                .await
                .unwrap()
                .id;

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension((state.clone()).claim_controller.clone()),
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
                Extension((state.clone()).claim_controller.clone()),
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

        let finish_item_id =
            crate::backend::items::creation::tests::service(&state.store, event_bus.clone())
                .create(
                    crate::backend::projects::ProjectReference::Name("demo"),
                    CreateWorkItem {
                        title: "Endpoint finish".to_owned(),
                        description: "Exercise finish endpoint".to_owned(),
                        state: "open".to_owned(),
                        agent_model_override: None,
                        agent_reasoning_effort_override: None,
                        initial_labels: Vec::new(),
                    },
                    Default::default(),
                )
                .await
                .unwrap()
                .id;

        let claimed: ClaimWorkItemResponse = decode(
            claim_item(
                Extension((state.clone()).claim_controller.clone()),
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
                Extension((state).claim_controller.clone()),
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
        let original = state.items.get("demo", item_id).await.unwrap();

        let updated: WorkItemView = decode(
            update_item(
                Extension((state).item_controller.clone()),
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
                Extension((state.clone()).item_controller.clone()),
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
                Extension((state).item_controller.clone()),
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
                Extension((state.clone()).label_controller.clone()),
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
            list_item_labels(
                Extension((state.clone()).label_controller.clone()),
                Path(("demo".to_owned(), item_id)),
            )
            .await,
        )
        .await;
        assert_that!(&(labels.iter().any(|label| label.key == "severity"))).is_true();

        let updated: WorkItemView = decode(
            update_item_label(
                Extension((state.clone()).label_controller.clone()),
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

        let suggestions: Vec<ProjectLabelView> = decode(
            list_project_labels(
                Extension((state.clone()).label_controller.clone()),
                Path("demo".to_owned()),
            )
            .await,
        )
        .await;
        assert_that!(
            &(suggestions
                .iter()
                .any(|label| { label.key == "priority" && label.value.as_deref() == Some("p1") }))
        )
        .is_true();

        let deleted = delete_item_label(
            Extension((state).label_controller.clone()),
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
        let event_bus = crate::backend::events::UiEventBus::new();

        let (_temp, state, source_id) = test_state().await;
        let target_id =
            crate::backend::items::creation::tests::service(&state.store, event_bus.clone())
                .create(
                    crate::backend::projects::ProjectReference::Name("demo"),
                    CreateWorkItem {
                        title: "Relationship target".to_owned(),
                        description: "Receives a relationship".to_owned(),
                        state: "open".to_owned(),
                        agent_model_override: None,
                        agent_reasoning_effort_override: None,
                        initial_labels: Vec::new(),
                    },
                    Default::default(),
                )
                .await
                .unwrap()
                .id;

        let created: WorkItemRelationshipListEntry = decode(
            create_item_relationship(
                Extension((state.clone()).relationship_controller.clone()),
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
                Extension((state.clone()).relationship_controller.clone()),
                Path(("demo".to_owned(), source_id)),
            )
            .await,
        )
        .await;
        assert_that!(&(outgoing.len())).is_equal_to(1);
        assert_that!(&(outgoing[0].direction)).is_equal_to(WorkItemRelationshipDirection::Outgoing);

        let incoming: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(
                Extension((state.clone()).relationship_controller.clone()),
                Path(("demo".to_owned(), target_id)),
            )
            .await,
        )
        .await;
        assert_that!(&(incoming.len())).is_equal_to(1);
        assert_that!(&(incoming[0].direction)).is_equal_to(WorkItemRelationshipDirection::Incoming);

        let duplicate = decode_error(
            create_item_relationship(
                Extension((state.clone()).relationship_controller.clone()),
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
                Extension((state.clone()).relationship_controller.clone()),
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
                Extension((state.clone()).relationship_controller.clone()),
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
                Extension((state.clone()).relationship_controller.clone()),
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
                Extension((state.clone()).relationship_controller.clone()),
                Path(("demo".to_owned(), created.relationship.id)),
                HeaderMap::new(),
            )
            .await,
        )
        .await;
        assert_that!(&(deleted.deleted)).is_true();

        let outgoing: Vec<WorkItemRelationshipListEntry> = decode(
            list_item_relationships(
                Extension((state).relationship_controller.clone()),
                Path(("demo".to_owned(), source_id)),
            )
            .await,
        )
        .await;
        assert_that!(&(outgoing.is_empty())).is_true();
    }
}
