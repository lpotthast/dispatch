#[cfg(feature = "ssr")]
use crate::backend::{
    app_state, automation, automation_bundles, automation_revisions, automation_routing,
    automation_triggers, page_data, personalities, projects,
};
use crate::frontend::{
    pages::{AutomationRuleInspectorView, BoardRunSessionView, TriggersPage},
    services::{cache::LocalStorageCache, origin::api_base_url, request::ServiceRequest},
    types::AutomationPersonalityInspectorView,
};
use dispatch_types::{
    AutomationBundleApplyView, AutomationBundleDiffView, AutomationBundleExportView,
    AutomationBundleValidationView, AutomationTriggerView, InstalledAutomationBundleView,
    RoutingExplanationView,
};
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct AutomationService {
    load_page: ServiceRequest<Option<String>, TriggersPage>,
    load_trigger_runs: ServiceRequest<(String, i64), Vec<BoardRunSessionView>>,
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
    page_cache: Option<LocalStorageCache<TriggersPage>>,
    trigger_runs_cache: Option<LocalStorageCache<Vec<BoardRunSessionView>>>,
}

struct AutomationRequests {
    load_page: ServiceRequest<Option<String>, TriggersPage>,
    load_trigger_runs: ServiceRequest<(String, i64), Vec<BoardRunSessionView>>,
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
            page_cache: None,
            trigger_runs_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(AutomationRequests {
            load_page: ServiceRequest::new(|selected_project| {
                Box::pin(load_triggers_page(selected_project, api_base_url()))
            }),
            load_trigger_runs: ServiceRequest::new(|(project, trigger_id)| {
                Box::pin(load_trigger_run_sessions(project, trigger_id))
            }),
            set_running: ServiceRequest::new(|(project, running)| {
                Box::pin(set_automation_running(project, running))
            }),
            schedule_trigger_evaluation: ServiceRequest::new(|(project, trigger_id)| {
                Box::pin(schedule_trigger_evaluation(project, trigger_id))
            }),
            validate_bundle_yaml: ServiceRequest::new(|yaml| Box::pin(validate_bundle_yaml(yaml))),
            diff_bundle_yaml: ServiceRequest::new(|(project, yaml)| {
                Box::pin(diff_bundle_yaml(project, yaml))
            }),
            apply_bundle_yaml: ServiceRequest::new(|(project, yaml, allow_deletions)| {
                Box::pin(apply_bundle_yaml(project, yaml, allow_deletions))
            }),
            export_bundle_yaml: ServiceRequest::new(|(project, bundle_key)| {
                Box::pin(export_bundle_yaml(project, bundle_key))
            }),
            list_installed_bundles: ServiceRequest::new(|project| {
                Box::pin(list_installed_bundles(project))
            }),
            remove_installed_bundle: ServiceRequest::new(|(project, bundle_key, expected_hash)| {
                Box::pin(remove_installed_bundle(project, bundle_key, expected_hash))
            }),
            load_rule_inspector: ServiceRequest::new(|(project, trigger_id)| {
                Box::pin(load_automation_rule_inspector(project, trigger_id))
            }),
            restore_rule_revision: ServiceRequest::new(|(project, trigger_id, revision_id)| {
                Box::pin(restore_automation_rule_revision(
                    project,
                    trigger_id,
                    revision_id,
                ))
            }),
            detach_rule: ServiceRequest::new(|(project, trigger_id)| {
                Box::pin(detach_automation_rule(project, trigger_id))
            }),
            load_personality_inspector: ServiceRequest::new(|(project, personality_id)| {
                Box::pin(load_automation_personality_inspector(
                    project,
                    personality_id,
                ))
            }),
            restore_personality_revision: ServiceRequest::new(
                |(project, personality_id, revision_id)| {
                    Box::pin(restore_automation_personality_revision(
                        project,
                        personality_id,
                        revision_id,
                    ))
                },
            ),
            detach_personality: ServiceRequest::new(|(project, personality_id)| {
                Box::pin(detach_automation_personality(project, personality_id))
            }),
            explain_route: ServiceRequest::new(|(project, item_id)| {
                Box::pin(explain_automation_route(project, item_id))
            }),
        });
        service.page_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.automation.v1",
        ));
        service.trigger_runs_cache = Some(LocalStorageCache::persistent(
            "dispatch.query.automation-trigger-runs.v1",
        ));
        service
    }

    pub(crate) fn cached_page(&self, selected_project: &Option<String>) -> Option<TriggersPage> {
        self.page_cache?.get(selected_project)
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<TriggersPage> {
        self.page_cache?.get_untracked(selected_project)
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<TriggersPage, ServerFnError> {
        let lifecycle_epoch = self.page_cache.map(|cache| cache.capture_lifecycle_epoch());
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.page_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.page_cache
        {
            cache.store(&key, &page);
        }
        Ok(page)
    }

    pub(crate) fn cached_trigger_runs(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Option<Vec<BoardRunSessionView>> {
        self.trigger_runs_cache?.get(&(project, trigger_id))
    }

    pub(crate) fn cached_trigger_runs_untracked(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Option<Vec<BoardRunSessionView>> {
        self.trigger_runs_cache?
            .get_untracked(&(project, trigger_id))
    }

    pub(crate) async fn load_trigger_runs(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<Vec<BoardRunSessionView>, ServerFnError> {
        let lifecycle_epoch = self
            .trigger_runs_cache
            .map(|cache| cache.capture_lifecycle_epoch());
        let key = project.clone();
        let runs = self
            .load_trigger_runs
            .execute((project, trigger_id))
            .await?;
        if lifecycle_epoch.is_some_and(|epoch| {
            self.trigger_runs_cache
                .is_some_and(|cache| cache.lifecycle_epoch_is(epoch))
        }) && let Some(cache) = self.trigger_runs_cache
        {
            cache.store(&(key, trigger_id), &runs);
        }
        Ok(runs)
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

    #[cfg(not(feature = "ssr"))]
    pub(crate) fn clear_cache(&self) {
        if let Some(cache) = self.page_cache {
            cache.clear();
        }
        if let Some(cache) = self.trigger_runs_cache {
            cache.clear();
        }
    }
}

#[server(prefix = "/leptos")]
async fn load_triggers_page(
    selected_project: Option<String>,
    api_base_url: String,
) -> Result<TriggersPage, ServerFnError> {
    let state = app_state::app_state();
    let codex_status = state.codex_status.read().await.clone();
    page_data::triggers_page_data(
        &state.store,
        &state.automation_controller,
        codex_status,
        selected_project.as_deref(),
        api_base_url,
    )
    .await
    .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_trigger_run_sessions(
    project: String,
    trigger_id: i64,
) -> Result<Vec<BoardRunSessionView>, ServerFnError> {
    let state = app_state::app_state();
    page_data::trigger_run_sessions(&state.store, &state.sessions, &project, trigger_id)
        .await
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn set_automation_running(project: String, running: bool) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    let result = if running {
        state
            .automation_controller
            .start_project(&state.store, project)
            .await
    } else {
        let project_id = projects::project_id(&state.store, &project)
            .await
            .map_err(|err| ServerFnError::new(err.to_string()))?;
        state
            .automation_controller
            .stop_project(project_id, &project, &state.sessions)
            .await
            .map_err(|err| ServerFnError::new(err.to_string()))?;
        automation::stop_automation(&state.store, &project)
            .await
            .map(|_| ())
    };
    result.map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn schedule_trigger_evaluation(
    project: String,
    trigger_id: i64,
) -> Result<(), ServerFnError> {
    let state = app_state::app_state();
    automation_triggers::schedule_trigger_evaluation(&state.store, &project, trigger_id)
        .await
        .map(|_| ())
        .map_err(|err| ServerFnError::new(err.to_string()))
}

#[server(prefix = "/leptos")]
async fn validate_bundle_yaml(
    yaml: String,
) -> Result<AutomationBundleValidationView, ServerFnError> {
    let bundle = automation_bundles::validate_yaml(&yaml)
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
    let state = app_state::app_state();
    automation_bundles::diff_yaml(&state.store, &project, &yaml)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn apply_bundle_yaml(
    project: String,
    yaml: String,
    allow_deletions: bool,
) -> Result<AutomationBundleApplyView, ServerFnError> {
    let state = app_state::app_state();
    let diff = automation_bundles::diff_yaml(&state.store, &project, &yaml)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    if diff.has_deletions && !allow_deletions {
        return Err(ServerFnError::new(
            "bundle diff deletes managed objects; confirm deletions before applying".to_owned(),
        ));
    }
    automation_bundles::apply_yaml(&state.store, &project, &yaml, diff.current_hash.as_deref())
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn export_bundle_yaml(
    project: String,
    bundle_key: String,
) -> Result<AutomationBundleExportView, ServerFnError> {
    let state = app_state::app_state();
    automation_bundles::export_yaml(&state.store, &project, &bundle_key)
        .await
        .map(|yaml| AutomationBundleExportView { yaml })
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn list_installed_bundles(
    project: String,
) -> Result<Vec<InstalledAutomationBundleView>, ServerFnError> {
    let state = app_state::app_state();
    automation_bundles::list_installed(&state.store, &project)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn remove_installed_bundle(
    project: String,
    bundle_key: String,
    expected_current_hash: String,
) -> Result<AutomationBundleApplyView, ServerFnError> {
    let state = app_state::app_state();
    automation_bundles::remove_bundle(
        &state.store,
        &project,
        &bundle_key,
        Some(&expected_current_hash),
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_automation_rule_inspector(
    project: String,
    trigger_id: i64,
) -> Result<AutomationRuleInspectorView, ServerFnError> {
    let state = app_state::app_state();
    let project_id = projects::project_id(&state.store, &project)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let trigger = automation_triggers::get_trigger(&state.store, &project, &trigger_id.to_string())
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let revisions =
        automation_revisions::list_trigger_revisions(&state.store, project_id, trigger_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    let evaluations =
        automation_revisions::list_evaluations(&state.store, &project, Some(trigger_id), 100)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    let current_revision_analytics = match trigger.current_revision_id {
        Some(revision_id) => Some(
            automation_revisions::trigger_revision_analytics(&state.store, &project, revision_id)
                .await
                .map_err(|error| ServerFnError::new(error.to_string()))?,
        ),
        None => None,
    };
    Ok(AutomationRuleInspectorView {
        trigger,
        revisions,
        evaluations,
        current_revision_analytics,
    })
}

#[server(prefix = "/leptos")]
async fn restore_automation_rule_revision(
    project: String,
    trigger_id: i64,
    revision_id: i64,
) -> Result<AutomationTriggerView, ServerFnError> {
    let state = app_state::app_state();
    automation_revisions::restore_trigger_revision(&state.store, &project, trigger_id, revision_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    automation_triggers::get_trigger(&state.store, &project, &trigger_id.to_string())
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn detach_automation_rule(
    project: String,
    trigger_id: i64,
) -> Result<AutomationTriggerView, ServerFnError> {
    let state = app_state::app_state();
    automation_triggers::detach_trigger(&state.store, &project, trigger_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn load_automation_personality_inspector(
    project: String,
    personality_id: i64,
) -> Result<AutomationPersonalityInspectorView, ServerFnError> {
    let state = app_state::app_state();
    let project_id = projects::project_id(&state.store, &project)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    let personality =
        personalities::get_personality(&state.store, &project, &personality_id.to_string())
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    let revisions =
        automation_revisions::list_personality_revisions(&state.store, project_id, personality_id)
            .await
            .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(AutomationPersonalityInspectorView {
        personality,
        revisions,
    })
}

#[server(prefix = "/leptos")]
async fn restore_automation_personality_revision(
    project: String,
    personality_id: i64,
    revision_id: i64,
) -> Result<dispatch_types::PersonalityView, ServerFnError> {
    let state = app_state::app_state();
    automation_revisions::restore_personality_revision(
        &state.store,
        &project,
        personality_id,
        revision_id,
    )
    .await
    .map(dispatch_types::PersonalityView::from)
    .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn detach_automation_personality(
    project: String,
    personality_id: i64,
) -> Result<dispatch_types::PersonalityView, ServerFnError> {
    let state = app_state::app_state();
    personalities::detach_personality(&state.store, &project, personality_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
async fn explain_automation_route(
    project: String,
    item_id: i64,
) -> Result<RoutingExplanationView, ServerFnError> {
    let state = app_state::app_state();
    automation_routing::explain(
        &state.store,
        &project,
        dispatch_types::RoutingExplainRequest {
            item_id: Some(item_id),
            rule: None,
        },
    )
    .await
    .map_err(|error| ServerFnError::new(error.to_string()))
}
