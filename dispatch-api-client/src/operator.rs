use crate::{ClientResult as Result, DispatchClient, OperatorProjectClient};
use dispatch_types::{
    AutomationBundleApplyView, AutomationBundleDiffView, AutomationBundleExportView,
    AutomationBundleValidationView, AutomationEvaluationView, AutomationPersonalityInput,
    AutomationRevisionView, AutomationRuleInput, AutomationTriggerView, BundleYamlRequest,
    InstalledAutomationBundleView, PersonalityRevisionView, PersonalityView,
    RemoveAutomationBundleRequest, RestoreRevisionRequest, RevisionAnalyticsView,
};
impl DispatchClient {
    pub async fn validate_automation_bundle(
        &self,
        request: &BundleYamlRequest,
    ) -> Result<AutomationBundleValidationView> {
        self.post("/operator/api/automation/bundles/validate", request)
            .await
    }
}

impl OperatorProjectClient<'_> {
    pub async fn diff_automation_bundle(
        &self,
        request: &BundleYamlRequest,
    ) -> Result<AutomationBundleDiffView> {
        self.post(self.endpoint("automation/bundles/diff"), request)
            .await
    }

    pub async fn apply_automation_bundle(
        &self,
        request: &BundleYamlRequest,
    ) -> Result<AutomationBundleApplyView> {
        self.post(self.endpoint("automation/bundles/apply"), request)
            .await
    }

    pub async fn export_automation_bundle(
        &self,
        bundle_key: &str,
    ) -> Result<AutomationBundleExportView> {
        self.get(
            self.endpoint("automation/bundles")
                .segment(bundle_key)
                .extend("export"),
        )
        .await
    }

    pub async fn list_installed_automation_bundles(
        &self,
    ) -> Result<Vec<InstalledAutomationBundleView>> {
        self.get(self.endpoint("automation/bundles")).await
    }

    pub async fn remove_automation_bundle(
        &self,
        bundle_key: &str,
        request: &RemoveAutomationBundleRequest,
    ) -> Result<AutomationBundleApplyView> {
        self.delete_with_body(
            self.endpoint("automation/bundles").segment(bundle_key),
            request,
        )
        .await
    }

    pub async fn list_automation_revisions(
        &self,
        trigger_id: i64,
    ) -> Result<Vec<AutomationRevisionView>> {
        self.get(
            self.endpoint("automation/triggers")
                .segment(trigger_id)
                .extend("revisions"),
        )
        .await
    }

    pub async fn operator_list_rules(&self) -> Result<Vec<AutomationTriggerView>> {
        self.get(self.endpoint("automation/rules")).await
    }

    pub async fn operator_get_rule(&self, id_or_key: &str) -> Result<AutomationTriggerView> {
        self.get(self.endpoint("automation/rules").segment(id_or_key))
            .await
    }

    pub async fn operator_create_rule(
        &self,
        input: &AutomationRuleInput,
    ) -> Result<AutomationTriggerView> {
        self.post(self.endpoint("automation/rules"), input).await
    }

    pub async fn operator_update_rule(
        &self,
        rule_id: i64,
        input: &AutomationRuleInput,
    ) -> Result<AutomationTriggerView> {
        self.put(self.endpoint("automation/rules").segment(rule_id), input)
            .await
    }

    pub async fn operator_delete_rule(&self, rule_id: i64) -> Result<()> {
        self.delete_without_response(self.endpoint("automation/rules").segment(rule_id))
            .await
    }

    pub async fn operator_schedule_rule(&self, rule_id: i64) -> Result<AutomationTriggerView> {
        self.post(
            self.endpoint("automation/rules")
                .segment(rule_id)
                .extend("schedule"),
            &(),
        )
        .await
    }

    pub async fn operator_restore_rule(
        &self,
        rule_id: i64,
        revision_id: i64,
    ) -> Result<AutomationTriggerView> {
        self.post(
            self.endpoint("automation/rules")
                .segment(rule_id)
                .extend("restore"),
            &RestoreRevisionRequest { revision_id },
        )
        .await
    }

    pub async fn operator_detach_rule(&self, rule_id: i64) -> Result<AutomationTriggerView> {
        self.post(
            self.endpoint("automation/rules")
                .segment(rule_id)
                .extend("detach"),
            &(),
        )
        .await
    }

    pub async fn operator_revision_analytics(
        &self,
        revision_id: i64,
    ) -> Result<RevisionAnalyticsView> {
        self.get(
            self.endpoint("automation/revisions")
                .segment(revision_id)
                .extend("analytics"),
        )
        .await
    }

    pub async fn operator_list_evaluations(
        &self,
        trigger_id: Option<i64>,
        limit: Option<u64>,
    ) -> Result<Vec<AutomationEvaluationView>> {
        let mut endpoint = self.endpoint("automation/evaluations");
        if let Some(trigger_id) = trigger_id {
            endpoint = endpoint.query("trigger_id", trigger_id);
        }
        if let Some(limit) = limit {
            endpoint = endpoint.query("limit", limit);
        }
        self.get(endpoint).await
    }

    pub async fn operator_list_personalities(&self) -> Result<Vec<PersonalityView>> {
        self.get(self.endpoint("automation/personalities")).await
    }

    pub async fn operator_get_personality(&self, id_or_key: &str) -> Result<PersonalityView> {
        self.get(self.endpoint("automation/personalities").segment(id_or_key))
            .await
    }

    pub async fn operator_create_personality(
        &self,
        input: &AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        self.post(self.endpoint("automation/personalities"), input)
            .await
    }

    pub async fn operator_update_personality(
        &self,
        personality_id: i64,
        input: &AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        self.put(
            self.endpoint("automation/personalities")
                .segment(personality_id),
            input,
        )
        .await
    }

    pub async fn operator_delete_personality(&self, personality_id: i64) -> Result<()> {
        self.delete_without_response(
            self.endpoint("automation/personalities")
                .segment(personality_id),
        )
        .await
    }

    pub async fn operator_list_personality_revisions(
        &self,
        personality_id: i64,
    ) -> Result<Vec<PersonalityRevisionView>> {
        self.get(
            self.endpoint("automation/personalities")
                .segment(personality_id)
                .extend("revisions"),
        )
        .await
    }

    pub async fn operator_restore_personality(
        &self,
        personality_id: i64,
        revision_id: i64,
    ) -> Result<PersonalityView> {
        self.post(
            self.endpoint("automation/personalities")
                .segment(personality_id)
                .extend("restore"),
            &RestoreRevisionRequest { revision_id },
        )
        .await
    }

    pub async fn operator_detach_personality(
        &self,
        personality_id: i64,
    ) -> Result<PersonalityView> {
        self.post(
            self.endpoint("automation/personalities")
                .segment(personality_id)
                .extend("detach"),
            &(),
        )
        .await
    }
}
