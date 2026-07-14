use dispatch_types::{
    AddCommentRequest, AgentRunView, ApiError, AssignWorkItemGroupRequest,
    AutomationBundleApplyView, AutomationBundleDiffView, AutomationBundleExportView,
    AutomationBundleValidationView, AutomationEvaluationView, AutomationPersonalityInput,
    AutomationRevisionView, AutomationRuleInput, AutomationTriggerView, BundleYamlRequest,
    ClaimWorkItemRequest, ClaimWorkItemResponse, CommentView, CreateWorkItemGroupRequest,
    CreateWorkItemLabelRequest, CreateWorkItemRelationshipRequest, CreateWorkItemRequest,
    DeleteWorkItemLabelResponse, DeleteWorkItemRelationshipResponse, FinishWorkItemRequest,
    InstalledAutomationBundleView, PersonalityRevisionView, PersonalityView,
    ProgressWorkItemRequest, ProjectLabelView, ProjectMemoryCompactionView, ProjectMemoryEventView,
    ProjectMemoryUpdateView, ProjectMemoryView, ProjectSettingsView, ProjectView,
    ReleaseWorkItemRequest, RemoveAutomationBundleRequest, RequestFeedbackWorkItemRequest,
    RestoreRevisionRequest, RevisionAnalyticsView, RoutingExplainRequest, RoutingExplanationView,
    RunLogView, UpdateProjectMemoryRequest, UpdateWorkItemLabelRequest,
    UpdateWorkItemRelationshipRequest, UpdateWorkItemRequest, WorkItemGroupView, WorkItemLabelView,
    WorkItemPage, WorkItemRelationshipListEntry, WorkItemRelationshipView, WorkItemSearchRequest,
    WorkItemView,
};
use rootcause::{Result, prelude::*};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Clone, Debug)]
pub struct DispatchClient {
    base_url: String,
    http: reqwest::Client,
    agent_id: Option<String>,
    agent_run_id: Option<i64>,
}

impl DispatchClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_owned(),
            http: reqwest::Client::new(),
            agent_id: None,
            agent_run_id: None,
        }
    }

    pub fn with_agent_context(
        mut self,
        agent_id: Option<impl Into<String>>,
        agent_run_id: Option<i64>,
    ) -> Self {
        self.agent_id = agent_id.map(Into::into);
        self.agent_run_id = agent_run_id;
        self
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn get_project(&self, project: &str) -> Result<ProjectView> {
        self.get(&project_path(project, "")).await
    }

    pub async fn get_project_settings(&self, project: &str) -> Result<ProjectSettingsView> {
        self.get(&project_path(project, "/settings")).await
    }

    pub async fn get_project_memory(&self, project: &str) -> Result<ProjectMemoryView> {
        self.get(&project_path(project, "/memory")).await
    }

    pub async fn list_project_memory_events(
        &self,
        project: &str,
    ) -> Result<Vec<ProjectMemoryEventView>> {
        self.get(&project_path(project, "/memory/events")).await
    }

    pub async fn set_project_memory(
        &self,
        project: &str,
        request: &UpdateProjectMemoryRequest,
    ) -> Result<ProjectMemoryUpdateView> {
        self.put(&project_path(project, "/memory"), request).await
    }

    pub async fn append_project_memory(
        &self,
        project: &str,
        request: &UpdateProjectMemoryRequest,
    ) -> Result<ProjectMemoryUpdateView> {
        self.post(&project_path(project, "/memory/append"), request)
            .await
    }

    pub async fn compact_project_memory_events(
        &self,
        project: &str,
    ) -> Result<ProjectMemoryCompactionView> {
        self.post(&project_path(project, "/memory/events/compact"), &())
            .await
    }

    pub async fn list_items(
        &self,
        project: &str,
        state: Option<&str>,
    ) -> Result<Vec<WorkItemView>> {
        let mut path = project_path(project, "/items");
        if let Some(state) = state {
            path.push_str("?state=");
            path.push_str(&urlencoding::encode(state));
        }
        self.get(&path).await
    }

    pub async fn search_items(
        &self,
        project: &str,
        request: &WorkItemSearchRequest,
    ) -> Result<WorkItemPage> {
        self.post(&project_path(project, "/items/search"), request)
            .await
    }

    pub async fn list_work_item_groups(&self, project: &str) -> Result<Vec<WorkItemGroupView>> {
        self.get(&project_path(project, "/work-groups")).await
    }

    pub async fn create_work_item_group(
        &self,
        project: &str,
        request: &CreateWorkItemGroupRequest,
    ) -> Result<WorkItemGroupView> {
        self.post(&project_path(project, "/work-groups"), request)
            .await
    }

    pub async fn assign_work_item_group_items(
        &self,
        project: &str,
        group_key: &str,
        request: &AssignWorkItemGroupRequest,
    ) -> Result<WorkItemGroupView> {
        self.post(
            &project_path(
                project,
                &format!("/work-groups/{}/items", encode_path_segment(group_key)),
            ),
            request,
        )
        .await
    }

    pub async fn list_automation_triggers(
        &self,
        project: &str,
    ) -> Result<Vec<AutomationTriggerView>> {
        self.get(&project_path(project, "/automation/triggers"))
            .await
    }

    pub async fn get_automation_trigger(
        &self,
        project: &str,
        id_or_key: &str,
    ) -> Result<AutomationTriggerView> {
        self.get(&project_path(
            project,
            &format!("/automation/triggers/{}", encode_path_segment(id_or_key)),
        ))
        .await
    }

    pub async fn explain_automation_routing(
        &self,
        project: &str,
        request: &RoutingExplainRequest,
    ) -> Result<RoutingExplanationView> {
        self.post(
            &project_path(project, "/automation/routing/explain"),
            request,
        )
        .await
    }

    pub async fn validate_automation_bundle(
        &self,
        request: &BundleYamlRequest,
    ) -> Result<AutomationBundleValidationView> {
        self.post("/operator/api/automation/bundles/validate", request)
            .await
    }

    pub async fn diff_automation_bundle(
        &self,
        project: &str,
        request: &BundleYamlRequest,
    ) -> Result<AutomationBundleDiffView> {
        self.post(
            &operator_project_path(project, "/automation/bundles/diff"),
            request,
        )
        .await
    }

    pub async fn apply_automation_bundle(
        &self,
        project: &str,
        request: &BundleYamlRequest,
    ) -> Result<AutomationBundleApplyView> {
        self.post(
            &operator_project_path(project, "/automation/bundles/apply"),
            request,
        )
        .await
    }

    pub async fn export_automation_bundle(
        &self,
        project: &str,
        bundle_key: &str,
    ) -> Result<AutomationBundleExportView> {
        self.get(&operator_project_path(
            project,
            &format!(
                "/automation/bundles/{}/export",
                encode_path_segment(bundle_key)
            ),
        ))
        .await
    }

    pub async fn list_installed_automation_bundles(
        &self,
        project: &str,
    ) -> Result<Vec<InstalledAutomationBundleView>> {
        self.get(&operator_project_path(project, "/automation/bundles"))
            .await
    }

    pub async fn remove_automation_bundle(
        &self,
        project: &str,
        bundle_key: &str,
        request: &RemoveAutomationBundleRequest,
    ) -> Result<AutomationBundleApplyView> {
        self.delete_with_body(
            &operator_project_path(
                project,
                &format!("/automation/bundles/{}", encode_path_segment(bundle_key)),
            ),
            request,
        )
        .await
    }

    pub async fn list_automation_revisions(
        &self,
        project: &str,
        trigger_id: i64,
    ) -> Result<Vec<AutomationRevisionView>> {
        self.get(&operator_project_path(
            project,
            &format!("/automation/triggers/{trigger_id}/revisions"),
        ))
        .await
    }

    pub async fn operator_list_rules(&self, project: &str) -> Result<Vec<AutomationTriggerView>> {
        self.get(&operator_project_path(project, "/automation/rules"))
            .await
    }

    pub async fn operator_get_rule(
        &self,
        project: &str,
        id_or_key: &str,
    ) -> Result<AutomationTriggerView> {
        self.get(&operator_project_path(
            project,
            &format!("/automation/rules/{}", encode_path_segment(id_or_key)),
        ))
        .await
    }

    pub async fn operator_create_rule(
        &self,
        project: &str,
        input: &AutomationRuleInput,
    ) -> Result<AutomationTriggerView> {
        self.post(&operator_project_path(project, "/automation/rules"), input)
            .await
    }

    pub async fn operator_update_rule(
        &self,
        project: &str,
        rule_id: i64,
        input: &AutomationRuleInput,
    ) -> Result<AutomationTriggerView> {
        self.put(
            &operator_project_path(project, &format!("/automation/rules/{rule_id}")),
            input,
        )
        .await
    }

    pub async fn operator_delete_rule(&self, project: &str, rule_id: i64) -> Result<()> {
        let _: serde_json::Value = self
            .delete(&operator_project_path(
                project,
                &format!("/automation/rules/{rule_id}"),
            ))
            .await?;
        Ok(())
    }

    pub async fn operator_schedule_rule(
        &self,
        project: &str,
        rule_id: i64,
    ) -> Result<AutomationTriggerView> {
        self.post(
            &operator_project_path(project, &format!("/automation/rules/{rule_id}/schedule")),
            &(),
        )
        .await
    }

    pub async fn operator_restore_rule(
        &self,
        project: &str,
        rule_id: i64,
        revision_id: i64,
    ) -> Result<AutomationTriggerView> {
        self.post(
            &operator_project_path(project, &format!("/automation/rules/{rule_id}/restore")),
            &RestoreRevisionRequest { revision_id },
        )
        .await
    }

    pub async fn operator_detach_rule(
        &self,
        project: &str,
        rule_id: i64,
    ) -> Result<AutomationTriggerView> {
        self.post(
            &operator_project_path(project, &format!("/automation/rules/{rule_id}/detach")),
            &(),
        )
        .await
    }

    pub async fn operator_revision_analytics(
        &self,
        project: &str,
        revision_id: i64,
    ) -> Result<RevisionAnalyticsView> {
        self.get(&operator_project_path(
            project,
            &format!("/automation/revisions/{revision_id}/analytics"),
        ))
        .await
    }

    pub async fn operator_list_evaluations(
        &self,
        project: &str,
        trigger_id: Option<i64>,
        limit: Option<u64>,
    ) -> Result<Vec<AutomationEvaluationView>> {
        let mut path = operator_project_path(project, "/automation/evaluations");
        let mut params = Vec::new();
        if let Some(trigger_id) = trigger_id {
            params.push(format!("trigger_id={trigger_id}"));
        }
        if let Some(limit) = limit {
            params.push(format!("limit={limit}"));
        }
        if !params.is_empty() {
            path.push('?');
            path.push_str(&params.join("&"));
        }
        self.get(&path).await
    }

    pub async fn operator_list_personalities(&self, project: &str) -> Result<Vec<PersonalityView>> {
        self.get(&operator_project_path(project, "/automation/personalities"))
            .await
    }

    pub async fn operator_get_personality(
        &self,
        project: &str,
        id_or_key: &str,
    ) -> Result<PersonalityView> {
        self.get(&operator_project_path(
            project,
            &format!(
                "/automation/personalities/{}",
                encode_path_segment(id_or_key)
            ),
        ))
        .await
    }

    pub async fn operator_create_personality(
        &self,
        project: &str,
        input: &AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        self.post(
            &operator_project_path(project, "/automation/personalities"),
            input,
        )
        .await
    }

    pub async fn operator_update_personality(
        &self,
        project: &str,
        personality_id: i64,
        input: &AutomationPersonalityInput,
    ) -> Result<PersonalityView> {
        self.put(
            &operator_project_path(
                project,
                &format!("/automation/personalities/{personality_id}"),
            ),
            input,
        )
        .await
    }

    pub async fn operator_delete_personality(
        &self,
        project: &str,
        personality_id: i64,
    ) -> Result<()> {
        let _: serde_json::Value = self
            .delete(&operator_project_path(
                project,
                &format!("/automation/personalities/{personality_id}"),
            ))
            .await?;
        Ok(())
    }

    pub async fn operator_list_personality_revisions(
        &self,
        project: &str,
        personality_id: i64,
    ) -> Result<Vec<PersonalityRevisionView>> {
        self.get(&operator_project_path(
            project,
            &format!("/automation/personalities/{personality_id}/revisions"),
        ))
        .await
    }

    pub async fn operator_restore_personality(
        &self,
        project: &str,
        personality_id: i64,
        revision_id: i64,
    ) -> Result<PersonalityView> {
        self.post(
            &operator_project_path(
                project,
                &format!("/automation/personalities/{personality_id}/restore"),
            ),
            &RestoreRevisionRequest { revision_id },
        )
        .await
    }

    pub async fn operator_detach_personality(
        &self,
        project: &str,
        personality_id: i64,
    ) -> Result<PersonalityView> {
        self.post(
            &operator_project_path(
                project,
                &format!("/automation/personalities/{personality_id}/detach"),
            ),
            &(),
        )
        .await
    }

    pub async fn list_project_labels(&self, project: &str) -> Result<Vec<ProjectLabelView>> {
        self.get(&project_path(project, "/labels")).await
    }

    pub async fn create_item(
        &self,
        project: &str,
        request: &CreateWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(&project_path(project, "/items"), request).await
    }

    pub async fn get_item(&self, project: &str, item_id: i64) -> Result<WorkItemView> {
        self.get(&project_path(project, &format!("/items/{item_id}")))
            .await
    }

    pub async fn update_item(
        &self,
        project: &str,
        item_id: i64,
        request: &UpdateWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.patch(
            &project_path(project, &format!("/items/{item_id}")),
            request,
        )
        .await
    }

    pub async fn list_item_labels(
        &self,
        project: &str,
        item_id: i64,
    ) -> Result<Vec<WorkItemLabelView>> {
        self.get(&project_path(project, &format!("/items/{item_id}/labels")))
            .await
    }

    pub async fn add_item_label(
        &self,
        project: &str,
        item_id: i64,
        request: &CreateWorkItemLabelRequest,
        expect_version: Option<i64>,
    ) -> Result<WorkItemView> {
        let mut path = project_path(project, &format!("/items/{item_id}/labels"));
        if let Some(expect_version) = expect_version {
            path.push_str("?expect_version=");
            path.push_str(&expect_version.to_string());
        }
        self.post(&path, request).await
    }

    pub async fn update_item_label(
        &self,
        project: &str,
        item_id: i64,
        label_id: i64,
        request: &UpdateWorkItemLabelRequest,
    ) -> Result<WorkItemView> {
        self.patch(
            &project_path(project, &format!("/items/{item_id}/labels/{label_id}")),
            request,
        )
        .await
    }

    pub async fn delete_item_label(
        &self,
        project: &str,
        item_id: i64,
        label_id: i64,
        expect_version: Option<i64>,
    ) -> Result<DeleteWorkItemLabelResponse> {
        let mut path = project_path(project, &format!("/items/{item_id}/labels/{label_id}"));
        if let Some(expect_version) = expect_version {
            path.push_str("?expect_version=");
            path.push_str(&expect_version.to_string());
        }
        self.delete(&path).await
    }

    pub async fn list_item_relationships(
        &self,
        project: &str,
        item_id: i64,
    ) -> Result<Vec<WorkItemRelationshipListEntry>> {
        self.get(&project_path(
            project,
            &format!("/items/{item_id}/relationships"),
        ))
        .await
    }

    pub async fn create_item_relationship(
        &self,
        project: &str,
        source_item_id: i64,
        request: &CreateWorkItemRelationshipRequest,
    ) -> Result<WorkItemRelationshipListEntry> {
        self.post(
            &project_path(project, &format!("/items/{source_item_id}/relationships")),
            request,
        )
        .await
    }

    pub async fn update_relationship(
        &self,
        project: &str,
        relationship_id: i64,
        request: &UpdateWorkItemRelationshipRequest,
    ) -> Result<WorkItemRelationshipView> {
        self.patch(
            &project_path(project, &format!("/relationships/{relationship_id}")),
            request,
        )
        .await
    }

    pub async fn delete_relationship(
        &self,
        project: &str,
        relationship_id: i64,
    ) -> Result<DeleteWorkItemRelationshipResponse> {
        self.delete(&project_path(
            project,
            &format!("/relationships/{relationship_id}"),
        ))
        .await
    }

    pub async fn claim_item(
        &self,
        project: &str,
        request: &ClaimWorkItemRequest,
    ) -> Result<ClaimWorkItemResponse> {
        self.post(&project_path(project, "/items/claim"), request)
            .await
    }

    pub async fn progress_item(
        &self,
        project: &str,
        item_id: i64,
        request: &ProgressWorkItemRequest,
    ) -> Result<CommentView> {
        self.post(
            &project_path(project, &format!("/items/{item_id}/progress")),
            request,
        )
        .await
    }

    pub async fn finish_item(
        &self,
        project: &str,
        item_id: i64,
        request: &FinishWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(
            &project_path(project, &format!("/items/{item_id}/finish")),
            request,
        )
        .await
    }

    pub async fn release_item(
        &self,
        project: &str,
        item_id: i64,
        request: &ReleaseWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(
            &project_path(project, &format!("/items/{item_id}/release")),
            request,
        )
        .await
    }

    pub async fn request_item_feedback(
        &self,
        project: &str,
        item_id: i64,
        request: &RequestFeedbackWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(
            &project_path(project, &format!("/items/{item_id}/request-feedback")),
            request,
        )
        .await
    }

    pub async fn list_comments(&self, project: &str, item_id: i64) -> Result<Vec<CommentView>> {
        self.get(&project_path(
            project,
            &format!("/items/{item_id}/comments"),
        ))
        .await
    }

    pub async fn add_comment(
        &self,
        project: &str,
        item_id: i64,
        request: &AddCommentRequest,
    ) -> Result<CommentView> {
        self.post(
            &project_path(project, &format!("/items/{item_id}/comments")),
            request,
        )
        .await
    }

    pub async fn list_runs(&self, project: &str, limit: Option<u64>) -> Result<Vec<AgentRunView>> {
        let mut path = project_path(project, "/automation/runs");
        if let Some(limit) = limit {
            path.push_str("?limit=");
            path.push_str(&limit.to_string());
        }
        self.get(&path).await
    }

    pub async fn read_run_log(&self, project: &str, run_id: i64) -> Result<RunLogView> {
        self.get(&project_path(
            project,
            &format!("/automation/runs/{run_id}/log"),
        ))
        .await
    }

    async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.send(self.http.get(self.url(path))).await
    }

    async fn post<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.post(self.url(path)).json(body)).await
    }

    async fn put<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.put(self.url(path)).json(body)).await
    }

    async fn patch<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.patch(self.url(path)).json(body)).await
    }

    async fn delete<T>(&self, path: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        self.send(self.http.delete(self.url(path))).await
    }

    async fn delete_with_body<T, B>(&self, path: &str, body: &B) -> Result<T>
    where
        T: DeserializeOwned,
        B: Serialize + ?Sized,
    {
        self.send(self.http.delete(self.url(path)).json(body)).await
    }

    async fn send<T>(&self, request: reqwest::RequestBuilder) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let mut request = request;
        if let Some(agent_id) = &self.agent_id {
            request = request.header("X-Dispatch-Agent-Id", agent_id);
        }
        if let Some(agent_run_id) = self.agent_run_id {
            request = request.header("X-Dispatch-Agent-Run-Id", agent_run_id);
        }
        let response = request
            .send()
            .await
            .context_with(|| format!("failed to call Dispatch API at {}", self.base_url))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .context("failed to read Dispatch API response")?;

        if !status.is_success() {
            if let Ok(error) = serde_json::from_slice::<ApiError>(&bytes) {
                bail!("{}", error.error);
            }
            let body = String::from_utf8_lossy(&bytes);
            bail!("Dispatch API returned {status}: {body}");
        }

        Ok(serde_json::from_slice(&bytes).context("failed to decode Dispatch API response")?)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/{}", self.base_url, path.trim_start_matches('/'))
    }
}

fn project_path(project: &str, suffix: &str) -> String {
    format!("/api/projects/{}{}", encode_path_segment(project), suffix)
}

fn operator_project_path(project: &str, suffix: &str) -> String {
    format!(
        "/operator/api/projects/{}{}",
        encode_path_segment(project),
        suffix
    )
}

fn encode_path_segment(value: &str) -> String {
    urlencoding::encode(value).into_owned()
}
