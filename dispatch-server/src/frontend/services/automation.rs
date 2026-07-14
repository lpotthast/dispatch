#[cfg(feature = "ssr")]
use crate::backend::{
    app_state, automation_bundles, automation_revisions, automation_routing, automation_triggers,
    page_data, personalities, projects,
};
use crate::frontend::{
    pages::{AutomationRuleInspectorView, BoardRunSessionView, TriggersPage},
    services::{
        cache::LocalStorageCache,
        origin::api_base_url,
        request::{ServiceFuture, ServiceRequest},
    },
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
    page_cache: Option<LocalStorageCache<TriggersPage>>,
    trigger_runs_cache: Option<LocalStorageCache<Vec<BoardRunSessionView>>>,
}

impl AutomationService {
    pub(crate) fn new(
        load_page: impl Fn(Option<String>) -> ServiceFuture<TriggersPage> + Send + Sync + 'static,
        load_trigger_runs: impl Fn((String, i64)) -> ServiceFuture<Vec<BoardRunSessionView>>
        + Send
        + Sync
        + 'static,
    ) -> Self {
        Self {
            load_page: ServiceRequest::new(load_page),
            load_trigger_runs: ServiceRequest::new(load_trigger_runs),
            page_cache: None,
            trigger_runs_cache: None,
        }
    }

    pub(super) fn production() -> Self {
        let mut service = Self::new(
            |selected_project| Box::pin(load_triggers_page(selected_project, api_base_url())),
            |(project, trigger_id)| Box::pin(load_trigger_run_sessions(project, trigger_id)),
        );
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
        let key = selected_project.clone();
        let page = self.load_page.execute(selected_project).await?;
        if let Some(cache) = self.page_cache {
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
        let key = project.clone();
        let runs = self
            .load_trigger_runs
            .execute((project, trigger_id))
            .await?;
        if let Some(cache) = self.trigger_runs_cache {
            cache.store(&(key, trigger_id), &runs);
        }
        Ok(runs)
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
pub(crate) async fn validate_bundle_yaml(
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
pub(crate) async fn diff_bundle_yaml(
    project: String,
    yaml: String,
) -> Result<AutomationBundleDiffView, ServerFnError> {
    let state = app_state::app_state();
    automation_bundles::diff_yaml(&state.store, &project, &yaml)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
pub(crate) async fn apply_bundle_yaml(
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
pub(crate) async fn export_bundle_yaml(
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
pub(crate) async fn list_installed_bundles(
    project: String,
) -> Result<Vec<InstalledAutomationBundleView>, ServerFnError> {
    let state = app_state::app_state();
    automation_bundles::list_installed(&state.store, &project)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
pub(crate) async fn remove_installed_bundle(
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
pub(crate) async fn load_automation_rule_inspector(
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
pub(crate) async fn restore_automation_rule_revision(
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
pub(crate) async fn detach_automation_rule(
    project: String,
    trigger_id: i64,
) -> Result<AutomationTriggerView, ServerFnError> {
    let state = app_state::app_state();
    automation_triggers::detach_trigger(&state.store, &project, trigger_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
pub(crate) async fn load_automation_personality_inspector(
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
pub(crate) async fn restore_automation_personality_revision(
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
pub(crate) async fn detach_automation_personality(
    project: String,
    personality_id: i64,
) -> Result<dispatch_types::PersonalityView, ServerFnError> {
    let state = app_state::app_state();
    personalities::detach_personality(&state.store, &project, personality_id)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))
}

#[server(prefix = "/leptos")]
pub(crate) async fn explain_automation_route(
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
