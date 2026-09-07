use super::types::*;
use crate::frontend::automation::service::AutomationService;
use crate::frontend::queries::cache::QueryCache;
use dispatch_types::*;
use leptos::prelude::*;

#[derive(Clone)]
pub(crate) struct AutomationStore {
    rule_inspector: QueryCache<(String, i64), AutomationRuleInspectorView>,
    personality_inspector: QueryCache<(String, i64), AutomationPersonalityInspectorView>,
    installed_bundles: QueryCache<String, Vec<InstalledAutomationBundleView>>,
    service: AutomationService,
    page: QueryCache<Option<String>, AutomationConfiguration>,
    trigger_runs: QueryCache<(String, i64), Vec<RunSummaryView>>,
}

impl AutomationStore {
    pub(crate) fn new(service: AutomationService) -> Self {
        Self {
            rule_inspector: QueryCache::in_memory(),
            personality_inspector: QueryCache::in_memory(),
            installed_bundles: QueryCache::in_memory(),
            service,
            page: QueryCache::persistent("dispatch.store.automation.v1"),
            trigger_runs: QueryCache::persistent("dispatch.store.trigger-runs.v1"),
        }
    }

    pub(crate) fn cached_page(
        &self,
        selected_project: &Option<String>,
    ) -> Option<AutomationConfiguration> {
        self.page.get(&selected_project.clone())
    }

    pub(crate) fn cached_page_untracked(
        &self,
        selected_project: &Option<String>,
    ) -> Option<AutomationConfiguration> {
        self.page.get_untracked(&selected_project.clone())
    }

    pub(crate) async fn load_page(
        &self,
        selected_project: Option<String>,
    ) -> Result<AutomationConfiguration, ServerFnError> {
        let service = self.service.clone();
        self.page
            .load(selected_project.clone(), move || async move {
                service.load_page(selected_project).await.map(Into::into)
            })
            .await
    }

    pub(crate) fn seed_page(
        &self,
        selected_project: Option<String>,
        value: AutomationConfiguration,
    ) {
        self.page.seed(selected_project.clone(), value);
    }

    pub(crate) fn cached_trigger_runs(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Option<Vec<RunSummaryView>> {
        self.trigger_runs.get(&(project.to_owned(), trigger_id))
    }

    pub(crate) fn cached_trigger_runs_untracked(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Option<Vec<RunSummaryView>> {
        self.trigger_runs
            .get_untracked(&(project.to_owned(), trigger_id))
    }

    pub(crate) async fn load_trigger_runs(
        &self,
        project: String,
        trigger_id: i64,
    ) -> Result<Vec<RunSummaryView>, ServerFnError> {
        let service = self.service.clone();
        self.trigger_runs
            .load((project.clone(), trigger_id), move || async move {
                service.load_trigger_runs(project, trigger_id).await
            })
            .await
    }

    pub(crate) fn seed_trigger_runs(
        &self,
        project: String,
        trigger_id: i64,
        value: Vec<RunSummaryView>,
    ) {
        self.trigger_runs.seed((project.clone(), trigger_id), value);
    }

    pub(crate) async fn load_rule_inspector(
        &self,
        project: String,
        id: i64,
    ) -> Result<AutomationRuleInspectorView, ServerFnError> {
        let service = self.service.clone();
        self.rule_inspector
            .load((project.clone(), id), move || async move {
                service.load_rule_inspector(project, id).await
            })
            .await
    }

    pub(crate) async fn load_personality_inspector(
        &self,
        project: String,
        id: i64,
    ) -> Result<AutomationPersonalityInspectorView, ServerFnError> {
        let service = self.service.clone();
        self.personality_inspector
            .load((project.clone(), id), move || async move {
                service.load_personality_inspector(project, id).await
            })
            .await
    }

    pub(crate) async fn list_installed_bundles(
        &self,
        project: String,
    ) -> Result<Vec<InstalledAutomationBundleView>, ServerFnError> {
        let service = self.service.clone();
        self.installed_bundles
            .load(project.clone(), move || async move {
                service.list_installed_bundles(project).await
            })
            .await
    }

    pub(crate) fn invalidate_page(&self, selected_project: Option<String>) {
        self.page.invalidate_key(&selected_project);
    }

    pub(crate) fn invalidate_trigger_runs(&self, project: String, trigger_id: i64) {
        self.trigger_runs.invalidate_key(&(project, trigger_id));
    }

    pub(crate) fn clear_cache(&self) {
        self.installed_bundles.clear();
        self.personality_inspector.clear();
        self.rule_inspector.clear();
        self.page.clear();
        self.trigger_runs.clear();
    }
}

pub(crate) fn automation_store() -> AutomationStore {
    leptos::prelude::expect_context()
}
