use crate::frontend::http::request::ServiceRequest;
use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct AutomationConfiguration {
    pub(crate) selected_project: Option<String>,
    pub(crate) selected_project_view: Option<ProjectView>,
    pub(crate) settings: Option<ProjectSettingsView>,
    pub(crate) personalities: Vec<PersonalityView>,
}
impl From<TriggersPage> for AutomationConfiguration {
    fn from(value: TriggersPage) -> Self {
        Self {
            selected_project: value.selected_project,
            selected_project_view: value.selected_project_view,
            settings: value.settings,
            personalities: value.personalities,
        }
    }
}

pub(super) struct AutomationRequests {
    pub(super) load_page: ServiceRequest<Option<String>, TriggersPage>,
    pub(super) load_trigger_runs: ServiceRequest<(String, i64), Vec<RunSummaryView>>,
    pub(super) set_running: ServiceRequest<(String, bool), ()>,
    pub(super) schedule_trigger_evaluation: ServiceRequest<(String, i64), ()>,
    pub(super) validate_bundle_yaml: ServiceRequest<String, AutomationBundleValidationView>,
    pub(super) diff_bundle_yaml: ServiceRequest<(String, String), AutomationBundleDiffView>,
    pub(super) apply_bundle_yaml: ServiceRequest<(String, String, bool), AutomationBundleApplyView>,
    pub(super) export_bundle_yaml: ServiceRequest<(String, String), AutomationBundleExportView>,
    pub(super) list_installed_bundles: ServiceRequest<String, Vec<InstalledAutomationBundleView>>,
    pub(super) remove_installed_bundle:
        ServiceRequest<(String, String, String), AutomationBundleApplyView>,
    pub(super) load_rule_inspector: ServiceRequest<(String, i64), AutomationRuleInspectorView>,
    pub(super) restore_rule_revision: ServiceRequest<(String, i64, i64), AutomationTriggerView>,
    pub(super) detach_rule: ServiceRequest<(String, i64), AutomationTriggerView>,
    pub(super) load_personality_inspector:
        ServiceRequest<(String, i64), AutomationPersonalityInspectorView>,
    pub(super) restore_personality_revision:
        ServiceRequest<(String, i64, i64), dispatch_types::PersonalityView>,
    pub(super) detach_personality: ServiceRequest<(String, i64), dispatch_types::PersonalityView>,
    pub(super) explain_route: ServiceRequest<(String, i64), RoutingExplanationView>,
}
