use super::types::AutomationRequests;
#[cfg(feature = "ssr")]
use crate::backend::app_state;
use crate::frontend::http::{origin::api_base_url, request::ServiceRequest};
use dispatch_types::AutomationPersonalityInspectorView;
use dispatch_types::AutomationRuleInspectorView;
use dispatch_types::{
    AutomationBundleApplyView, AutomationBundleDiffView, AutomationBundleExportView,
    AutomationBundleValidationView, AutomationTriggerView, InstalledAutomationBundleView,
    RoutingExplanationView,
};
use dispatch_types::{RunSummaryView, TriggersPage};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct AutomationService {
    load_page: ServiceRequest<Option<String>, TriggersPage>,
    load_trigger_runs: ServiceRequest<(String, i64), Vec<RunSummaryView>>,
    set_running: ServiceRequest<(String, bool), ()>,
    schedule_trigger_evaluation: ServiceRequest<(String, i64), ()>,
    validate_bundle_yaml: ServiceRequest<String, AutomationBundleValidationView>,
    diff_bundle_yaml: ServiceRequest<(String, String), AutomationBundleDiffView>,
    apply_bundle_yaml: ServiceRequest<(String, String, bool), AutomationBundleApplyView>,
    export_bundle_yaml: ServiceRequest<(String, String), AutomationBundleExportView>,
    list_installed_bundles: ServiceRequest<String, Vec<InstalledAutomationBundleView>>,
    remove_installed_bundle: ServiceRequest<(String, String, String), AutomationBundleApplyView>,
    load_rule_inspector: ServiceRequest<(String, i64), AutomationRuleInspectorView>,
    restore_rule_revision: ServiceRequest<(String, i64, i64), AutomationTriggerView>,
    detach_rule: ServiceRequest<(String, i64), AutomationTriggerView>,
    load_personality_inspector: ServiceRequest<(String, i64), AutomationPersonalityInspectorView>,
    restore_personality_revision:
        ServiceRequest<(String, i64, i64), dispatch_types::PersonalityView>,
    detach_personality: ServiceRequest<(String, i64), dispatch_types::PersonalityView>,
    explain_route: ServiceRequest<(String, i64), RoutingExplanationView>,
}

impl AutomationService {
    fn new(requests: AutomationRequests) -> Self {
        Self {
            load_page: requests.load_page,
            load_trigger_runs: requests.load_trigger_runs,
            set_running: requests.set_running,
            schedule_trigger_evaluation: requests.schedule_trigger_evaluation,
            validate_bundle_yaml: requests.validate_bundle_yaml,
            diff_bundle_yaml: requests.diff_bundle_yaml,
            apply_bundle_yaml: requests.apply_bundle_yaml,
            export_bundle_yaml: requests.export_bundle_yaml,
            list_installed_bundles: requests.list_installed_bundles,
            remove_installed_bundle: requests.remove_installed_bundle,
            load_rule_inspector: requests.load_rule_inspector,
            restore_rule_revision: requests.restore_rule_revision,
            detach_rule: requests.detach_rule,
            load_personality_inspector: requests.load_personality_inspector,
            restore_personality_revision: requests.restore_personality_revision,
            detach_personality: requests.detach_personality,
            explain_route: requests.explain_route,
        }
    }

    pub(crate) fn production(http: crate::frontend::http::HttpService) -> Self {
        Self::new(AutomationRequests {
            load_page: http.request(|selected_project| {
                Box::pin(load_triggers_page(selected_project, api_base_url()))
            }),
            load_trigger_runs: http.request(|(project, trigger_id)| {
                Box::pin(load_trigger_run_summaries(project, trigger_id))
            }),
            set_running: http
                .request(|(project, running)| Box::pin(set_automation_running(project, running))),
            schedule_trigger_evaluation: http.request(|(project, trigger_id)| {
                Box::pin(schedule_trigger_evaluation(project, trigger_id))
            }),
            validate_bundle_yaml: http.request(|yaml| Box::pin(validate_bundle_yaml(yaml))),
            diff_bundle_yaml: http
                .request(|(project, yaml)| Box::pin(diff_bundle_yaml(project, yaml))),
            apply_bundle_yaml: http.request(|(project, yaml, allow_deletions)| {
                Box::pin(apply_bundle_yaml(project, yaml, allow_deletions))
            }),
            export_bundle_yaml: http
                .request(|(project, bundle_key)| Box::pin(export_bundle_yaml(project, bundle_key))),
            list_installed_bundles: http
                .request(|project| Box::pin(list_installed_bundles(project))),
            remove_installed_bundle: http.request(|(project, bundle_key, expected_hash)| {
                Box::pin(remove_installed_bundle(project, bundle_key, expected_hash))
            }),
            load_rule_inspector: http.request(|(project, trigger_id)| {
                Box::pin(load_automation_rule_inspector(project, trigger_id))
            }),
            restore_rule_revision: http.request(|(project, trigger_id, revision_id)| {
                Box::pin(restore_automation_rule_revision(
                    project,
                    trigger_id,
                    revision_id,
                ))
            }),
            detach_rule: http.request(|(project, trigger_id)| {
                Box::pin(detach_automation_rule(project, trigger_id))
            }),
            load_personality_inspector: http.request(|(project, personality_id)| {
                Box::pin(load_automation_personality_inspector(
                    project,
                    personality_id,
                ))
            }),
            restore_personality_revision: http.request(|(project, personality_id, revision_id)| {
                Box::pin(restore_automation_personality_revision(
                    project,
                    personality_id,
                    revision_id,
                ))
            }),
            detach_personality: http.request(|(project, personality_id)| {
                Box::pin(detach_automation_personality(project, personality_id))
            }),
            explain_route: http
                .request(|(project, item_id)| Box::pin(explain_automation_route(project, item_id))),
        })
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<TriggersPage, ServerFnError> {
        self.load_page.execute(selected_project).await
    }

    pub(crate) async fn load_trigger_runs(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<Vec<RunSummaryView>, ServerFnError> {
        self.load_trigger_runs.execute((project, trigger_id)).await
    }

    pub(crate) async fn set_running(
        &self,
        project: String,
        running: bool,
    ) -> Result<(), ServerFnError> {
        self.set_running.execute((project, running)).await
    }

    pub(crate) async fn schedule_trigger_evaluation(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<(), ServerFnError> {
        self.schedule_trigger_evaluation
            .execute((project, trigger_id))
            .await
    }

    pub(crate) async fn validate_bundle_yaml(
        &self,
        yaml: String,
    ) -> Result<AutomationBundleValidationView, ServerFnError> {
        self.validate_bundle_yaml.execute(yaml).await
    }

    pub(crate) async fn diff_bundle_yaml(
        &self,
        project: String,
        yaml: String,
    ) -> Result<AutomationBundleDiffView, ServerFnError> {
        self.diff_bundle_yaml.execute((project, yaml)).await
    }

    pub(crate) async fn apply_bundle_yaml(
        &self,
        project: String,
        yaml: String,
        allow_deletions: bool,
    ) -> Result<AutomationBundleApplyView, ServerFnError> {
        self.apply_bundle_yaml
            .execute((project, yaml, allow_deletions))
            .await
    }

    pub(crate) async fn export_bundle_yaml(
        &self,
        project: String,
        bundle_key: String,
    ) -> Result<AutomationBundleExportView, ServerFnError> {
        self.export_bundle_yaml.execute((project, bundle_key)).await
    }

    pub(crate) async fn list_installed_bundles(
        &self,
        project: String,
    ) -> Result<Vec<InstalledAutomationBundleView>, ServerFnError> {
        self.list_installed_bundles.execute(project).await
    }

    pub(crate) async fn remove_installed_bundle(
        &self,
        project: String,
        bundle_key: String,
        expected_hash: String,
    ) -> Result<AutomationBundleApplyView, ServerFnError> {
        self.remove_installed_bundle
            .execute((project, bundle_key, expected_hash))
            .await
    }

    pub(crate) async fn load_rule_inspector(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<AutomationRuleInspectorView, ServerFnError> {
        self.load_rule_inspector
            .execute((project, trigger_id))
            .await
    }

    pub(crate) async fn restore_rule_revision(
        &self,
        project: String,
        trigger_id: i64,
        revision_id: i64,
    ) -> Result<AutomationTriggerView, ServerFnError> {
        self.restore_rule_revision
            .execute((project, trigger_id, revision_id))
            .await
    }

    pub(crate) async fn detach_rule(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<AutomationTriggerView, ServerFnError> {
        self.detach_rule.execute((project, trigger_id)).await
    }

    pub(crate) async fn load_personality_inspector(
        &self,
        project: String,
        personality_id: i64,
    ) -> Result<AutomationPersonalityInspectorView, ServerFnError> {
        self.load_personality_inspector
            .execute((project, personality_id))
            .await
    }

    pub(crate) async fn restore_personality_revision(
        &self,
        project: String,
        personality_id: i64,
        revision_id: i64,
    ) -> Result<dispatch_types::PersonalityView, ServerFnError> {
        self.restore_personality_revision
            .execute((project, personality_id, revision_id))
            .await
    }

    pub(crate) async fn detach_personality(
        &self,
        project: String,
        personality_id: i64,
    ) -> Result<dispatch_types::PersonalityView, ServerFnError> {
        self.detach_personality
            .execute((project, personality_id))
            .await
    }

    pub(crate) async fn explain_route(
        &self,
        project: String,
        item_id: i64,
    ) -> Result<RoutingExplanationView, ServerFnError> {
        self.explain_route.execute((project, item_id)).await
    }
}

#[server(prefix = "/leptos")]
async fn load_triggers_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<TriggersPage, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .automation_page(selected_project.as_deref(), api_base_url)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_trigger_run_summaries(
    project: String,
    trigger_id: i64,
) -> Result<Vec<RunSummaryView>, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .operator_queries
        .rule_runs(&project, trigger_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn set_automation_running(project: String, running: bool) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    let result = if running {
        state.automation_supervisor.start_project(project).await
    } else {
        state.automation_supervisor.stop_project(&project).await
    };
    result.map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn schedule_trigger_evaluation(
    project: String,
    trigger_id: i64,
) -> Result<(), ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .rules
        .schedule(&project, trigger_id)
        .await
        .map(|_| ())
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn validate_bundle_yaml(
    yaml: String,
) -> Result<AutomationBundleValidationView, ServerFnError> {
    let bundle = crate::backend::automation::bundles::policy::validate_yaml(&yaml)
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(AutomationBundleValidationView {
        manifest: bundle.manifest,
        manifest_hash: bundle.manifest_hash,
    })
}

#[server(prefix = "/leptos")]
async fn diff_bundle_yaml(
    project: String,
    yaml: String,
) -> Result<AutomationBundleDiffView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .bundles
        .diff(&project, &yaml)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn apply_bundle_yaml(
    project: String,
    yaml: String,
    allow_deletions: bool,
) -> Result<AutomationBundleApplyView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .bundles
        .apply_current(&project, &yaml, allow_deletions)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn export_bundle_yaml(
    project: String,
    bundle_key: String,
) -> Result<AutomationBundleExportView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .bundles
        .export(&project, &bundle_key)
        .await
        .map(|yaml| AutomationBundleExportView { yaml })
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn list_installed_bundles(
    project: String,
) -> Result<Vec<InstalledAutomationBundleView>, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .bundles
        .list_installed(&project)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn remove_installed_bundle(
    project: String,
    bundle_key: String,
    expected_current_hash: String,
) -> Result<AutomationBundleApplyView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .bundles
        .remove(&project, &bundle_key, Some(&expected_current_hash))
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_automation_rule_inspector(
    project: String,
    trigger_id: i64,
) -> Result<AutomationRuleInspectorView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .revision_queries
        .inspect_rule(&project, trigger_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn restore_automation_rule_revision(
    project: String,
    trigger_id: i64,
    revision_id: i64,
) -> Result<AutomationTriggerView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .rules
        .restore(&project, trigger_id, revision_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn detach_automation_rule(
    project: String,
    trigger_id: i64,
) -> Result<AutomationTriggerView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .rules
        .detach(&project, trigger_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_automation_personality_inspector(
    project: String,
    personality_id: i64,
) -> Result<AutomationPersonalityInspectorView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .personalities
        .inspect(&project, personality_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn restore_automation_personality_revision(
    project: String,
    personality_id: i64,
    revision_id: i64,
) -> Result<dispatch_types::PersonalityView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .personalities
        .restore(&project, personality_id, revision_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn detach_automation_personality(
    project: String,
    personality_id: i64,
) -> Result<dispatch_types::PersonalityView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .personalities
        .detach(&project, personality_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn explain_automation_route(
    project: String,
    item_id: i64,
) -> Result<RoutingExplanationView, ServerFnError> {
    let state = leptos::prelude::expect_context::<app_state::AppState>();
    state
        .routing
        .explain(
            &project,
            dispatch_types::RoutingExplainRequest {
                item_id: Some(item_id),
                rule: None,
            },
        )
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

pub(crate) fn automation_service() -> AutomationService {
    leptos::prelude::expect_context()
}

#[cfg(all(test, feature = "ssr"))]
mod automation_adapter_tests {
    use super::*;
    use crate::backend::projects::ProjectReference;
    use assertr::prelude::*;
    use leptos::prelude::{Owner, ScopedFuture, provide_context};

    #[tokio::test]
    async fn personality_restore_uses_request_service_and_retains_revision_history() {
        let (_temp, first, _, _) = crate::backend::comments::tests::application().await;
        let (_other_temp, second, _, _) = crate::backend::comments::tests::application().await;
        let mut records = Vec::new();
        for app in [&first, &second] {
            let record = app
                .state
                .personalities
                .create(
                    ProjectReference::Name("demo"),
                    dispatch_types::AutomationPersonalityInput {
                        key: String::new(),
                        name: "Review".into(),
                        description: "first".into(),
                    },
                )
                .await
                .unwrap();
            app.state
                .personalities
                .update(
                    ProjectReference::Name("demo"),
                    record.id,
                    dispatch_types::AutomationPersonalityInput {
                        key: String::new(),
                        name: "Review".into(),
                        description: "second".into(),
                    },
                )
                .await
                .unwrap();
            records.push(record);
        }
        let owner = Owner::new();
        owner.with(|| provide_context(first.state.clone()));
        let mut events = first.state.events.subscribe();
        let mut other_events = second.state.events.subscribe();
        let restored = owner
            .with(|| {
                ScopedFuture::new(restore_automation_personality_revision(
                    "demo".into(),
                    records[0].id,
                    records[0].current_revision_id.unwrap(),
                ))
            })
            .await
            .unwrap();
        assert_that!(&restored.personality_description).is_equal_to("first");
        let inspector = owner
            .with(|| {
                ScopedFuture::new(load_automation_personality_inspector(
                    "demo".into(),
                    records[0].id,
                ))
            })
            .await
            .unwrap();
        assert_that!(&inspector.revisions.len()).is_equal_to(3);
        assert_that!(&inspector.revisions[0].operation)
            .is_equal_to(dispatch_types::RevisionChangeOperation::Restore);
        assert_that!(
            &second
                .state
                .personalities
                .get("demo", "Review")
                .await
                .unwrap()
                .personality_description
        )
        .is_equal_to("second");
        assert_that!(&matches!(
            events.try_recv().unwrap(),
            dispatch_types::UiEvent::AutomationChanged { .. }
        ))
        .is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(&other_events.try_recv().is_err()).is_true();
    }
    #[tokio::test]
    async fn rule_restore_and_inspector_use_the_request_service_and_share_committed_history() {
        let (_temp, first, _, _) = crate::backend::comments::tests::application().await;
        let (_other_temp, second, _, _) = crate::backend::comments::tests::application().await;
        let mut records = Vec::new();
        for app in [&first, &second] {
            let input: dispatch_types::AutomationRuleInput = serde_json::from_value(serde_json::json!({"name":"Rule","enabled":true,"activation":"work_item","effect":"consume_work","schedule":"15s","prompt_markdown":"first"})).unwrap();
            let record = app
                .state
                .rules
                .create_from_input("demo", input.clone())
                .await
                .unwrap();
            let mut changed = input;
            changed.prompt_markdown = "second".into();
            app.state
                .rules
                .update_from_input("demo", record.id, changed)
                .await
                .unwrap();
            records.push(record);
        }
        let owner = Owner::new();
        owner.with(|| provide_context(first.state.clone()));
        let mut events = first.state.events.subscribe();
        let mut other_events = second.state.events.subscribe();
        let restored = owner
            .with(|| {
                ScopedFuture::new(restore_automation_rule_revision(
                    "demo".into(),
                    records[0].id,
                    records[0].current_revision_id.unwrap(),
                ))
            })
            .await
            .unwrap();
        assert_that!(&restored.prompt).is_equal_to(records[0].prompt.clone());
        let inspector = owner
            .with(|| {
                ScopedFuture::new(load_automation_rule_inspector("demo".into(), records[0].id))
            })
            .await
            .unwrap();
        assert_that!(&inspector.trigger.current_revision_id)
            .is_equal_to(restored.current_revision_id);
        assert_that!(&inspector.revisions.len()).is_equal_to(3);
        assert_that!(&inspector.revisions[0].operation)
            .is_equal_to(dispatch_types::RevisionChangeOperation::Restore);
        assert_that!(&inspector.current_revision_analytics.unwrap().revision_id)
            .is_equal_to(restored.current_revision_id.unwrap());
        assert_that!(&second.state.rules.get("demo", "Rule").await.unwrap().prompt)
            .contains("second");
        assert_that!(&matches!(
            events.try_recv().unwrap(),
            dispatch_types::UiEvent::AutomationChanged { .. }
        ))
        .is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(&other_events.try_recv().is_err()).is_true();
    }
    #[tokio::test]
    async fn bundles_share_service_history_through_json_and_leptos_without_cross_instance_events() {
        use axum::{Extension, http::StatusCode};
        let (_temp, first, _, _) = crate::backend::comments::tests::application().await;
        let (_other, second, _, _) = crate::backend::comments::tests::application().await;
        let yaml = include_str!("../../../../examples/automation/engineering-review.yaml");
        let mut events = first.state.events.subscribe();
        let mut other_events = second.state.events.subscribe();
        let router = crate::backend::automation::bundles::transport::routes::<()>()
            .layer(Extension(first.state.clone()));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
        let client = reqwest::Client::new();
        let response = client
            .post(format!(
                "{url}/operator/api/projects/demo/automation/bundles/apply"
            ))
            .json(&dispatch_types::BundleYamlRequest {
                yaml: yaml.into(),
                expected_current_hash: None,
            })
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(StatusCode::OK);
        let applied: AutomationBundleApplyView = response.json().await.unwrap();
        assert_that!(&applied.status).is_equal_to("applied");
        assert_that!(&events.try_recv().is_ok()).is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(&other_events.try_recv().is_err()).is_true();
        assert_that!(&second.state.bundles.list_installed("demo").await.unwrap()).is_empty();
        let owner = Owner::new();
        owner.with(|| provide_context(first.state.clone()));
        let exported = owner
            .with(|| {
                ScopedFuture::new(export_bundle_yaml(
                    "demo".into(),
                    "engineering-review".into(),
                ))
            })
            .await
            .unwrap();
        assert_that!(
            &crate::backend::automation::bundles::policy::validate_yaml(&exported.yaml)
                .unwrap()
                .manifest_hash
        )
        .is_equal_to(applied.diff.manifest_hash);
        let empty = "schema_version: 1\nbundle_key: engineering-review\ndisplay_name: Empty\npersonalities: []\nautomations: []\n";
        assert_that!(
            &owner
                .with(|| ScopedFuture::new(apply_bundle_yaml("demo".into(), empty.into(), false)))
                .await
                .is_err()
        )
        .is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        let changed = owner
            .with(|| ScopedFuture::new(apply_bundle_yaml("demo".into(), empty.into(), true)))
            .await
            .unwrap();
        assert_that!(&changed.diff.has_deletions).is_true();
        assert_that!(&events.try_recv().is_ok()).is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        let installed = first.state.bundles.list_installed("demo").await.unwrap();
        assert_that!(&installed[0].manifest_hash).is_equal_to(changed.diff.manifest_hash.clone());
        let response = client
            .delete(format!(
                "{url}/operator/api/projects/demo/automation/bundles/engineering-review"
            ))
            .json(&dispatch_types::RemoveAutomationBundleRequest {
                expected_current_hash: Some(changed.diff.manifest_hash),
            })
            .send()
            .await
            .unwrap();
        assert_that!(&response.status()).is_equal_to(StatusCode::OK);
        assert_that!(&first.state.bundles.list_installed("demo").await.unwrap()).is_empty();
        assert_that!(&events.try_recv().is_ok()).is_true();
        assert_that!(&events.try_recv().is_err()).is_true();
        assert_that!(&other_events.try_recv().is_err()).is_true();
        server.abort();
    }
}
