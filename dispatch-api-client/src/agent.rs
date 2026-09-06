use crate::{ClientResult as Result, DispatchClient, ProjectClient};
use dispatch_types::{
    AddCommentRequest, AgentRunView, AssignWorkItemGroupRequest, AutomationTriggerView,
    ClaimWorkItemRequest, ClaimWorkItemResponse, CommentView, CreateWorkItemGroupRequest,
    CreateWorkItemLabelRequest, CreateWorkItemRelationshipRequest, CreateWorkItemRequest,
    DeleteWorkItemLabelResponse, DeleteWorkItemRelationshipResponse, FinishWorkItemRequest,
    ProgressWorkItemRequest, ProjectLabelView, ProjectSettingsView, ProjectView,
    ReleaseWorkItemRequest, RequestFeedbackWorkItemRequest, RoutingExplainRequest,
    RoutingExplanationView, RunLogView, UpdateWorkItemLabelRequest,
    UpdateWorkItemRelationshipRequest, UpdateWorkItemRequest, WorkItemGroupView, WorkItemLabelView,
    WorkItemPage, WorkItemRelationshipListEntry, WorkItemRelationshipView, WorkItemSearchRequest,
    WorkItemView,
};
impl DispatchClient {
    pub async fn list_projects(&self) -> Result<Vec<ProjectView>> {
        self.get("/api/projects").await
    }
}

impl ProjectClient<'_> {
    pub async fn get_project(&self) -> Result<ProjectView> {
        self.get(self.endpoint("")).await
    }

    pub async fn get_project_settings(&self) -> Result<ProjectSettingsView> {
        self.get(self.endpoint("settings")).await
    }

    pub async fn list_items(&self, state: Option<&str>) -> Result<Vec<WorkItemView>> {
        let mut endpoint = self.endpoint("items");
        if let Some(state) = state {
            endpoint = endpoint.query("state", state);
        }
        self.get(endpoint).await
    }

    pub async fn search_items(&self, request: &WorkItemSearchRequest) -> Result<WorkItemPage> {
        self.post(self.endpoint("items/search"), request).await
    }

    pub async fn list_work_item_groups(&self) -> Result<Vec<WorkItemGroupView>> {
        self.get(self.endpoint("work-groups")).await
    }

    pub async fn create_work_item_group(
        &self,
        request: &CreateWorkItemGroupRequest,
    ) -> Result<WorkItemGroupView> {
        self.post(self.endpoint("work-groups"), request).await
    }

    pub async fn assign_work_item_group_items(
        &self,
        group_key: &str,
        request: &AssignWorkItemGroupRequest,
    ) -> Result<WorkItemGroupView> {
        self.post(
            self.endpoint("work-groups")
                .segment(group_key)
                .extend("items"),
            request,
        )
        .await
    }

    pub async fn list_automation_triggers(&self) -> Result<Vec<AutomationTriggerView>> {
        self.get(self.endpoint("automation/triggers")).await
    }

    pub async fn get_automation_trigger(&self, id_or_key: &str) -> Result<AutomationTriggerView> {
        self.get(self.endpoint("automation/triggers").segment(id_or_key))
            .await
    }

    pub async fn explain_automation_routing(
        &self,
        request: &RoutingExplainRequest,
    ) -> Result<RoutingExplanationView> {
        self.post(self.endpoint("automation/routing/explain"), request)
            .await
    }

    pub async fn list_project_labels(&self) -> Result<Vec<ProjectLabelView>> {
        self.get(self.endpoint("labels")).await
    }

    pub async fn create_item(&self, request: &CreateWorkItemRequest) -> Result<WorkItemView> {
        self.post(self.endpoint("items"), request).await
    }

    pub async fn get_item(&self, item_id: i64) -> Result<WorkItemView> {
        self.get(self.endpoint("items").segment(item_id)).await
    }

    pub async fn update_item(
        &self,
        item_id: i64,
        request: &UpdateWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.patch(self.endpoint("items").segment(item_id), request)
            .await
    }

    pub async fn list_item_labels(&self, item_id: i64) -> Result<Vec<WorkItemLabelView>> {
        self.get(self.endpoint("items").segment(item_id).extend("labels"))
            .await
    }

    pub async fn add_item_label(
        &self,
        item_id: i64,
        request: &CreateWorkItemLabelRequest,
        expect_version: Option<i64>,
    ) -> Result<WorkItemView> {
        let mut endpoint = self.endpoint("items").segment(item_id).extend("labels");
        if let Some(expect_version) = expect_version {
            endpoint = endpoint.query("expect_version", expect_version);
        }
        self.post(endpoint, request).await
    }

    pub async fn update_item_label(
        &self,
        item_id: i64,
        label_id: i64,
        request: &UpdateWorkItemLabelRequest,
    ) -> Result<WorkItemView> {
        self.patch(
            self.endpoint("items")
                .segment(item_id)
                .extend("labels")
                .segment(label_id),
            request,
        )
        .await
    }

    pub async fn delete_item_label(
        &self,
        item_id: i64,
        label_id: i64,
        expect_version: Option<i64>,
    ) -> Result<DeleteWorkItemLabelResponse> {
        let mut endpoint = self
            .endpoint("items")
            .segment(item_id)
            .extend("labels")
            .segment(label_id);
        if let Some(expect_version) = expect_version {
            endpoint = endpoint.query("expect_version", expect_version);
        }
        self.delete(endpoint).await
    }

    pub async fn list_item_relationships(
        &self,
        item_id: i64,
    ) -> Result<Vec<WorkItemRelationshipListEntry>> {
        self.get(
            self.endpoint("items")
                .segment(item_id)
                .extend("relationships"),
        )
        .await
    }

    pub async fn create_item_relationship(
        &self,
        source_item_id: i64,
        request: &CreateWorkItemRelationshipRequest,
    ) -> Result<WorkItemRelationshipListEntry> {
        self.post(
            self.endpoint("items")
                .segment(source_item_id)
                .extend("relationships"),
            request,
        )
        .await
    }

    pub async fn update_relationship(
        &self,
        relationship_id: i64,
        request: &UpdateWorkItemRelationshipRequest,
    ) -> Result<WorkItemRelationshipView> {
        self.patch(
            self.endpoint("relationships").segment(relationship_id),
            request,
        )
        .await
    }

    pub async fn delete_relationship(
        &self,
        relationship_id: i64,
    ) -> Result<DeleteWorkItemRelationshipResponse> {
        self.delete(self.endpoint("relationships").segment(relationship_id))
            .await
    }

    pub async fn claim_item(
        &self,
        request: &ClaimWorkItemRequest,
    ) -> Result<ClaimWorkItemResponse> {
        self.post(self.endpoint("items/claim"), request).await
    }

    pub async fn progress_item(
        &self,
        item_id: i64,
        request: &ProgressWorkItemRequest,
    ) -> Result<CommentView> {
        self.post(
            self.endpoint("items").segment(item_id).extend("progress"),
            request,
        )
        .await
    }

    pub async fn finish_item(
        &self,
        item_id: i64,
        request: &FinishWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(
            self.endpoint("items").segment(item_id).extend("finish"),
            request,
        )
        .await
    }

    pub async fn release_item(
        &self,
        item_id: i64,
        request: &ReleaseWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(
            self.endpoint("items").segment(item_id).extend("release"),
            request,
        )
        .await
    }

    pub async fn request_item_feedback(
        &self,
        item_id: i64,
        request: &RequestFeedbackWorkItemRequest,
    ) -> Result<WorkItemView> {
        self.post(
            self.endpoint("items")
                .segment(item_id)
                .extend("request-feedback"),
            request,
        )
        .await
    }

    pub async fn list_comments(&self, item_id: i64) -> Result<Vec<CommentView>> {
        self.get(self.endpoint("items").segment(item_id).extend("comments"))
            .await
    }

    pub async fn add_comment(
        &self,
        item_id: i64,
        request: &AddCommentRequest,
    ) -> Result<CommentView> {
        self.post(
            self.endpoint("items").segment(item_id).extend("comments"),
            request,
        )
        .await
    }

    pub async fn list_runs(&self, limit: Option<u64>) -> Result<Vec<AgentRunView>> {
        let mut endpoint = self.endpoint("automation/runs");
        if let Some(limit) = limit {
            endpoint = endpoint.query("limit", limit);
        }
        self.get(endpoint).await
    }

    pub async fn read_run_log(&self, run_id: i64) -> Result<RunLogView> {
        self.get(
            self.endpoint("automation/runs")
                .segment(run_id)
                .extend("log"),
        )
        .await
    }
}
