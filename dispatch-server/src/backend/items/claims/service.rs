use super::{
    model::*,
    policy::{self, ClaimReturnMode},
    repository::ClaimRepository,
};
use crate::backend::{
    attribution::{model::AttributionInput, service::AttributionService},
    events::UiEventBus,
    execution::identity as agent_ids,
    items::{labels::workflow as workflow_labels, repository::ItemRepository},
    projects::repository::ProjectRepository,
    runs::launch::{
        model::{AgentLaunchResolutionV1, AgentLaunchTargetV1, AgentRunLaunchState},
        repository::AgentRunLaunchRepository,
    },
    storage::{Transaction, TransactionManager, utc_now},
};
use crudkit_core::condition::Condition;
use dispatch_types::{
    AuthorType, CommentView, RecoveredClaimView, WorkItemEventType, WorkItemView,
};
use rootcause::{Result, prelude::*};
use std::sync::Arc;
pub(crate) struct ClaimService {
    transactions: Arc<TransactionManager>,
    projects: Arc<ProjectRepository>,
    repository: Arc<ClaimRepository>,
    items: Arc<ItemRepository>,
    launches: Arc<AgentRunLaunchRepository>,
    attribution: Arc<AttributionService>,
    events: UiEventBus,
}
impl ClaimService {
    pub(crate) fn new(
        transactions: Arc<TransactionManager>,
        projects: Arc<ProjectRepository>,
        repository: Arc<ClaimRepository>,
        items: Arc<ItemRepository>,
        launches: Arc<AgentRunLaunchRepository>,
        attribution: Arc<AttributionService>,
        events: UiEventBus,
    ) -> Self {
        Self {
            transactions,
            projects,
            repository,
            items,
            launches,
            attribution,
            events,
        }
    }
    pub(crate) async fn claim_item(
        &self,
        project_name: &str,
        agent_id: &str,
        state_filter: &str,
        input: AttributionInput,
    ) -> Result<Option<WorkItemView>> {
        agent_ids::validate_agent_id(agent_id)?;
        let selector = ClaimSelector::state(state_filter)?;
        let transaction = self
            .transactions
            .begin()
            .await
            .context("failed to start item claim")?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let attribution = self
            .attribution
            .validate_in(&transaction, project_name, input, false)
            .await?;
        attribution.cross_check_agent_id(agent_id)?;
        attribution.ensure_generic_claim()?;
        let item = self
            .claim_first_in(&transaction, project_id, agent_id, &selector)
            .await?;
        self.commit_claim(
            transaction,
            project_name,
            item,
            "failed to commit item claim",
        )
        .await
    }
    #[cfg(test)]
    pub(crate) async fn claim_item_matching_condition(
        &self,
        project_name: &str,
        agent_id: &str,
        condition: &Condition,
    ) -> Result<Option<WorkItemView>> {
        agent_ids::validate_agent_id(agent_id)?;
        let selector = ClaimSelector::automation_condition(condition)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let item = self
            .claim_first_in(&transaction, project_id, agent_id, &selector)
            .await?;
        self.commit_claim(
            transaction,
            project_name,
            item,
            "failed to commit item claim",
        )
        .await
    }
    pub(crate) async fn matching_item_ids_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        condition: &Condition,
    ) -> Result<Vec<i64>> {
        let selector = ClaimSelector::automation_condition(condition)?;
        self.repository
            .matching_ids_in(transaction, project_id, &selector)
            .await
    }
    pub(crate) async fn has_claimable_item_matching_condition(
        &self,
        project_name: &str,
        condition: &Condition,
    ) -> Result<bool> {
        let selector = ClaimSelector::automation_condition(condition)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let available = self
            .repository
            .next_in(&transaction, project_id, &selector, None)
            .await?
            .is_some();
        transaction.commit().await?;
        Ok(available)
    }
    pub(crate) async fn has_claimable_specific_item_matching_condition(
        &self,
        project_name: &str,
        item_id: i64,
        condition: &Condition,
    ) -> Result<bool> {
        let selector = ClaimSelector::automation_condition(condition)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let available = self
            .repository
            .specific_in(&transaction, project_id, item_id, Some(&selector))
            .await?
            .is_some();
        transaction.commit().await?;
        Ok(available)
    }
    #[cfg(test)]
    pub(crate) async fn claim_specific_item_matching_condition(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        condition: &Condition,
    ) -> Result<Option<WorkItemView>> {
        let selector = ClaimSelector::automation_condition(condition)?;
        self.claim_specific(project_name, item_id, agent_id, Some(&selector))
            .await
    }
    #[cfg(test)]
    pub(crate) async fn claim_specific_item(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
    ) -> Result<Option<WorkItemView>> {
        self.claim_specific(project_name, item_id, agent_id, None)
            .await
    }
    #[cfg(test)]
    async fn claim_specific(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        selector: Option<&ClaimSelector>,
    ) -> Result<Option<WorkItemView>> {
        agent_ids::validate_agent_id(agent_id)?;
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let candidate = self
            .repository
            .specific_in(&transaction, project_id, item_id, selector)
            .await?;
        let item = match candidate {
            Some(candidate) => {
                self.claim_candidate_in(&transaction, project_id, agent_id, &candidate, None)
                    .await?
            }
            None => None,
        };
        self.commit_claim(
            transaction,
            project_name,
            item,
            "failed to commit specific item claim",
        )
        .await
    }
    async fn claim_first_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        agent_id: &str,
        selector: &ClaimSelector,
    ) -> Result<Option<WorkItemView>> {
        let mut previous = None;
        while let Some(candidate) = self
            .repository
            .next_in(transaction, project_id, selector, previous.as_ref())
            .await?
        {
            if let Some(item) = self
                .claim_candidate_in(
                    transaction,
                    project_id,
                    agent_id,
                    &candidate,
                    Some(candidate.observed_version),
                )
                .await?
            {
                return Ok(Some(item));
            }
            previous = Some(candidate);
        }
        Ok(None)
    }
    async fn claim_candidate_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        agent_id: &str,
        candidate: &ClaimCandidate,
        expected_version: Option<i64>,
    ) -> Result<Option<WorkItemView>> {
        if !self
            .repository
            .claim_in(
                transaction,
                project_id,
                candidate,
                agent_id,
                expected_version,
            )
            .await?
        {
            return Ok(None);
        }
        let body = format!("Claimed by {agent_id}");
        self.repository
            .record_in(
                transaction,
                project_id,
                candidate.item_id,
                agent_id,
                ClaimRecord {
                    labels: Some(workflow_labels::new_claim_workflow_label_plan(
                        &candidate.source_state,
                    )),
                    comment: Some((AuthorType::System, &body)),
                    event_type: WorkItemEventType::ItemClaimed,
                    body: &body,
                },
            )
            .await?;
        Ok(Some(
            self.items
                .get_in(transaction, project_id, candidate.item_id)
                .await?,
        ))
    }
    async fn commit_claim(
        &self,
        transaction: Transaction,
        project_name: &str,
        item: Option<WorkItemView>,
        context: &'static str,
    ) -> Result<Option<WorkItemView>> {
        transaction.commit().await.context(context)?;
        if let Some(item) = &item {
            self.events.publish_work_item_changed(project_name, item.id);
        }
        Ok(item)
    }
    pub(crate) async fn resolve_agent_run_target(
        &self,
        project_name: &str,
        run_id: i64,
        agent_id: &str,
        supplied_target: &AgentLaunchTargetV1,
        selector_condition: Option<&Condition>,
    ) -> Result<Option<WorkItemView>> {
        agent_ids::validate_agent_id(agent_id)?;
        let transaction = self
            .transactions
            .begin()
            .await
            .context("failed to start agent run target resolution")?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let item = self
            .resolve_agent_run_target_in(
                &transaction,
                project_id,
                run_id,
                agent_id,
                supplied_target,
                selector_condition,
            )
            .await?;
        self.commit_claim(
            transaction,
            project_name,
            item,
            "failed to commit agent run target resolution",
        )
        .await
    }
    pub(crate) async fn resolve_agent_run_target_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
        agent_id: &str,
        supplied_target: &AgentLaunchTargetV1,
        selector_condition: Option<&Condition>,
    ) -> Result<Option<WorkItemView>> {
        agent_ids::validate_agent_id(agent_id)?;
        let contract = self
            .launches
            .load_in(transaction, project_id, run_id)
            .await?
            .ok_or_else(|| report!("agent run {run_id} has no persisted launch contract"))?;
        if &contract.target != supplied_target {
            bail!("supplied automation target disagrees with persisted launch contract");
        }
        if matches!(supplied_target, AgentLaunchTargetV1::None { .. }) {
            if contract.state != AgentRunLaunchState::TargetResolved
                || contract.resolution != AgentLaunchResolutionV1::none()
            {
                bail!("none launch target is not resolved as none");
            }
            return Ok(None);
        }
        if contract.state != AgentRunLaunchState::Prepared
            || contract.resolution != AgentLaunchResolutionV1::pending()
        {
            bail!("agent run launch target has already been resolved");
        }
        let item = match supplied_target {
            AgentLaunchTargetV1::None { .. } => unreachable!("none target returned before claim"),
            AgentLaunchTargetV1::NextOpen { state, .. } => {
                if selector_condition.is_some() {
                    bail!("next-open launch target cannot carry an automation selector");
                }
                self.claim_first_in(
                    transaction,
                    project_id,
                    agent_id,
                    &ClaimSelector::state(state)?,
                )
                .await?
            }
            AgentLaunchTargetV1::Selector { .. } => {
                let condition = selector_condition
                    .ok_or_else(|| report!("selector launch target is missing its condition"))?;
                supplied_target.validate_selector(condition)?;
                self.claim_first_in(
                    transaction,
                    project_id,
                    agent_id,
                    &ClaimSelector::automation_condition(condition)?,
                )
                .await?
            }
            AgentLaunchTargetV1::Specific {
                work_item_id,
                expected_version,
                ..
            } => {
                let existing = self
                    .items
                    .get_in(transaction, project_id, *work_item_id)
                    .await?;
                if existing.version != *expected_version {
                    None
                } else {
                    let selector = selector_condition
                        .map(ClaimSelector::automation_condition)
                        .transpose()?;
                    match self
                        .repository
                        .specific_in(transaction, project_id, *work_item_id, selector.as_ref())
                        .await?
                    {
                        Some(candidate) => {
                            self.claim_candidate_in(
                                transaction,
                                project_id,
                                agent_id,
                                &candidate,
                                Some(*expected_version),
                            )
                            .await?
                        }
                        None => None,
                    }
                }
            }
        };
        let resolution = item.as_ref().map_or_else(
            || AgentLaunchResolutionV1::unavailable("target_not_claimable"),
            |item| AgentLaunchResolutionV1::claimed(item.id, item.version),
        );
        self.launches
            .resolve_in(
                transaction,
                project_id,
                run_id,
                &resolution,
                item.as_ref().map(|item| item.id),
                &utc_now(),
            )
            .await?;
        match item {
            Some(item) => Ok(Some(
                self.items.get_in(transaction, project_id, item.id).await?,
            )),
            None => Ok(None),
        }
    }
    pub(crate) async fn progress_item(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        body: &str,
        input: AttributionInput,
    ) -> Result<CommentView> {
        agent_ids::validate_agent_id(agent_id)?;
        if body.trim().is_empty() {
            bail!("progress body cannot be empty");
        }
        let transaction = self
            .transactions
            .begin()
            .await
            .context("failed to start item progress")?;
        let project_id = self
            .scope_in(
                &transaction,
                project_name,
                agent_id,
                input,
                "record item progress",
            )
            .await?;
        let mut item = self.items.get_in(&transaction, project_id, item_id).await?;
        policy::ensure_active_claim(&item, agent_id)?;
        policy::touch_claim(&mut item, &utc_now());
        self.repository
            .save_in(&transaction, &item)
            .await
            .context("failed to update item after progress")?;
        let comment = self
            .repository
            .record_in(
                &transaction,
                project_id,
                item_id,
                agent_id,
                ClaimRecord {
                    labels: None,
                    comment: Some((AuthorType::Agent, body)),
                    event_type: WorkItemEventType::ProgressAdded,
                    body,
                },
            )
            .await?
            .ok_or_else(|| report!("progress comment was not persisted"))?;
        transaction
            .commit()
            .await
            .context("failed to commit item progress")?;
        self.events.publish_comment_changed(project_name, item_id);
        Ok(comment)
    }
    pub(crate) async fn finish_item(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        report: &str,
        input: AttributionInput,
    ) -> Result<WorkItemView> {
        agent_ids::validate_agent_id(agent_id)?;
        if report.trim().is_empty() {
            bail!("finish report cannot be empty");
        }
        let transaction = self
            .transactions
            .begin()
            .await
            .context("failed to start item finish")?;
        let project_id = self
            .scope_in(
                &transaction,
                project_name,
                agent_id,
                input,
                "finish work items",
            )
            .await?;
        let mut item = self.items.get_in(&transaction, project_id, item_id).await?;
        policy::ensure_active_claim(&item, agent_id)?;
        let now = utc_now();
        policy::clear_claim(&mut item, &now);
        item.finished_at = Some(now);
        self.repository
            .save_in(&transaction, &item)
            .await
            .context("failed to finish work item")?;
        self.repository
            .record_in(
                &transaction,
                project_id,
                item_id,
                agent_id,
                ClaimRecord {
                    labels: Some(workflow_labels::finish_workflow_label_plan()),
                    comment: Some((AuthorType::Agent, report)),
                    event_type: WorkItemEventType::ItemFinished,
                    body: report,
                },
            )
            .await?;
        let item = self.items.get_in(&transaction, project_id, item_id).await?;
        transaction
            .commit()
            .await
            .context("failed to commit item finish")?;
        self.events.publish_work_item_changed(project_name, item_id);
        Ok(item)
    }
    async fn scope_in(
        &self,
        transaction: &Transaction,
        project_name: &str,
        agent_id: &str,
        input: AttributionInput,
        operation: &str,
    ) -> Result<i64> {
        let project_id = self.projects.id_in(transaction, project_name).await?;
        let attribution = self
            .attribution
            .validate_in(transaction, project_name, input, false)
            .await?;
        attribution.cross_check_agent_id(agent_id)?;
        attribution.ensure_item_mutation(operation)?;
        Ok(project_id)
    }
    pub(crate) async fn release_item(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        comment: Option<String>,
        automation_disposition: ReleaseAutomationDisposition,
        input: AttributionInput,
    ) -> Result<WorkItemView> {
        agent_ids::validate_agent_id(agent_id)?;
        self.return_claim(
            project_name,
            item_id,
            agent_id,
            ClaimReturnMode::Release {
                comment: comment.as_deref(),
                automation_disposition,
            },
            input,
            "release work items",
        )
        .await
    }
    pub(crate) async fn request_feedback(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        body: &str,
        input: AttributionInput,
    ) -> Result<WorkItemView> {
        agent_ids::validate_agent_id(agent_id)?;
        if body.trim().is_empty() {
            bail!("feedback request body cannot be empty");
        }
        self.return_claim(
            project_name,
            item_id,
            agent_id,
            ClaimReturnMode::FeedbackRequest { body },
            input,
            "request item feedback",
        )
        .await
    }
    async fn return_claim(
        &self,
        project_name: &str,
        item_id: i64,
        agent_id: &str,
        mode: ClaimReturnMode<'_>,
        input: AttributionInput,
        operation: &str,
    ) -> Result<WorkItemView> {
        let transaction = self
            .transactions
            .begin()
            .await
            .context(mode.start_context())?;
        let project_id = self
            .scope_in(&transaction, project_name, agent_id, input, operation)
            .await?;
        let item = self.items.get_in(&transaction, project_id, item_id).await?;
        let item = self
            .return_in(&transaction, project_id, item, agent_id, mode)
            .await?;
        transaction.commit().await.context(mode.commit_context())?;
        self.events.publish_work_item_changed(project_name, item_id);
        Ok(item)
    }
    async fn return_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        mut item: WorkItemView,
        agent_id: &str,
        mode: ClaimReturnMode<'_>,
    ) -> Result<WorkItemView> {
        policy::ensure_active_claim(&item, agent_id)?;
        let release_state = workflow_labels::release_state_from_claim_labels(&item.labels);
        policy::clear_claim(&mut item, &utc_now());
        self.repository
            .save_in(transaction, &item)
            .await
            .context(mode.update_context())?;
        let event_body = mode.event_body(agent_id, &release_state);
        self.repository
            .record_in(
                transaction,
                project_id,
                item.id,
                agent_id,
                ClaimRecord {
                    labels: Some(workflow_labels::claim_return_workflow_label_plan(
                        &release_state,
                        mode.label_disposition(),
                    )),
                    comment: mode
                        .agent_comment_body()
                        .filter(|body| !body.trim().is_empty())
                        .map(|body| (AuthorType::Agent, body)),
                    event_type: mode.event_type(),
                    body: &event_body,
                },
            )
            .await?;
        self.items.get_in(transaction, project_id, item.id).await
    }
    pub(crate) async fn finalize_automation_claim(
        &self,
        context: AutomationClaimFinalization<'_>,
    ) -> Result<()> {
        let project = context.project_name;
        let transaction = self.transactions.begin().await?;
        let changed = self.finalize_automation_in(&transaction, context).await?;
        transaction.commit().await?;
        if let Some(id) = changed {
            self.events.publish_work_item_changed(project, id);
        }
        Ok(())
    }
    pub(crate) async fn finalize_automation_in(
        &self,
        transaction: &Transaction,
        context: AutomationClaimFinalization<'_>,
    ) -> Result<Option<i64>> {
        let AutomationClaimFinalization {
            project_id,
            project_name,
            run_id,
            claimed_item_id,
            agent_id,
            outcome,
            detail,
        } = context;
        let Some(item_id) = claimed_item_id else {
            return Ok(None);
        };
        let resolved = self.projects.id_in(transaction, project_name).await?;
        if resolved != project_id {
            bail!("automation claim project does not match run scope");
        }
        let item = self.items.get_in(transaction, project_id, item_id).await?;
        if item.claimed_by.as_deref() != Some(agent_id) || item.finished_at.is_some() {
            return Ok(None);
        }
        agent_ids::validate_agent_id(agent_id)?;
        let comment = policy::automation_claim_release_comment(outcome, run_id, detail);
        self.return_in(
            transaction,
            project_id,
            item,
            agent_id,
            ClaimReturnMode::Release {
                comment: Some(&comment),
                automation_disposition: outcome.release_disposition(),
            },
        )
        .await?;
        Ok(Some(item_id))
    }
    pub(crate) async fn recover_stale_claims(
        &self,
        project_name: &str,
        stale_after_minutes: i64,
    ) -> Result<Vec<RecoveredClaimView>> {
        self.recover_configured(project_name, Some(stale_after_minutes))
            .await
    }
    pub(crate) async fn recover_all_configured(&self) -> Result<Vec<RecoveredClaimView>> {
        let projects = self.projects.list().await?;
        let mut recovered = Vec::new();
        for project in projects {
            recovered.extend(self.recover_configured(&project.name, None).await?);
        }
        Ok(recovered)
    }
    pub(crate) async fn recover_configured(
        &self,
        project_name: &str,
        stale_after_minutes: Option<i64>,
    ) -> Result<Vec<RecoveredClaimView>> {
        let transaction = self.transactions.begin().await?;
        let project_id = self.projects.id_in(&transaction, project_name).await?;
        let stale_after_minutes = match stale_after_minutes {
            Some(minutes) => minutes,
            None => {
                self.projects
                    .settings_in(&transaction, project_name)
                    .await?
                    .stale_claim_minutes
            }
        };
        if stale_after_minutes <= 0 {
            transaction.commit().await?;
            return Ok(Vec::new());
        }

        let items = self.repository.claimed_in(&transaction, project_id).await?;
        let now = time::OffsetDateTime::now_utc();
        let mut recovered = Vec::new();
        for item in items {
            if !policy::is_stale(&item, stale_after_minutes, now) {
                continue;
            }
            let Some(agent_id) = item.claimed_by.clone() else {
                continue;
            };
            agent_ids::validate_agent_id(&agent_id)?;
            let claim = RecoveredClaimView {
                item_id: item.id,
                agent_id: agent_id.clone(),
                claimed_at: item.claimed_at.clone(),
            };
            let comment = format!("Recovered stale claim after {stale_after_minutes} minute(s).");
            self.return_in(
                &transaction,
                project_id,
                item,
                &agent_id,
                ClaimReturnMode::Release {
                    comment: Some(&comment),
                    automation_disposition: ReleaseAutomationDisposition::Claimable,
                },
            )
            .await?;
            recovered.push(claim);
        }
        transaction.commit().await?;
        for claim in &recovered {
            self.events
                .publish_work_item_changed(project_name, claim.item_id);
        }
        Ok(recovered)
    }
}
