pub(crate) mod encoding;
use super::model::{CreateRunConfig, RunChange};
use crate::backend::{
    entities::agent_run::{self, AgentRun, AgentRunActiveModel, AgentRunModel},
    runs::launch::{
        model::AgentCapabilitySetV1,
        repository::{insert_contract_in_tx, mark_spawned_in_tx, mark_terminal_in_tx},
    },
    storage::{Transaction, utc_now},
};
use dispatch_types::*;
use rootcause::{Result, prelude::*};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityTrait, QueryFilter, QueryOrder,
};
pub(crate) struct RunRepository;
impl RunRepository {
    pub(crate) async fn item_exists_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        item_id: i64,
    ) -> Result<bool> {
        use crate::backend::entities::work_item;
        Ok(work_item::Entity::find_by_id(item_id)
            .filter(work_item::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await?
            .is_some())
    }
    pub(crate) async fn update_metadata_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
        metadata: &super::model::RunMetadata,
    ) -> Result<()> {
        let mut active: AgentRunActiveModel =
            load_model_in(transaction, project_id, run_id).await?.into();
        active.work_item_id = Set(metadata.work_item_id);
        active.tool_name = Set(metadata.tool.as_storage().into());
        active.updated_at = Set(utc_now());
        active.update(transaction.connection()).await?;
        Ok(())
    }
    pub(crate) async fn delete_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<u64> {
        Ok(AgentRun::delete_many()
            .filter(agent_run::Column::Id.eq(run_id))
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .exec(transaction.connection())
            .await?
            .rows_affected)
    }

    pub(crate) async fn create_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        config: CreateRunConfig<'_>,
    ) -> Result<AgentRunView> {
        let now = utc_now();
        let run = AgentRunActiveModel {
            project_id: Set(project_id),
            work_item_id: Set(None),
            knowledge_job_id: Set(config.knowledge_job_id),
            run_kind: Set(config.run_kind.as_storage().to_owned()),
            purpose: Set(Some(config.purpose.as_storage().to_owned())),
            knowledge_revision: Set(None),
            source_baseline_id: Set(None),
            source_snapshot_id: Set(None),
            knowledge_view_sha256: Set(None),
            input_overlay_sha256: Set(None),
            source_authority_kind: Set(None),
            source_ref_name: Set(None),
            source_raw_head: Set(None),
            memory_event_id: Set(None),
            trigger_id: Set(config.trigger.map(|trigger| trigger.trigger_id)),
            trigger_name: Set(config.trigger.map(|trigger| trigger.trigger_name.clone())),
            trigger_revision_id: Set(config
                .trigger
                .and_then(|trigger| trigger.trigger_revision_id)),
            personality_revision_id: Set(config.personality_revision_id),
            system_prompt_event_id: Set(None),
            tool_name: Set(config.tool.as_storage().to_owned()),
            mutability: Set(config.mutability.as_storage().to_owned()),
            status: Set(AgentRunStatus::Running.as_storage().to_owned()),
            command: Set(String::new()),
            working_dir: Set(String::new()),
            worktree_path: Set(None),
            branch_name: Set(None),
            process_id: Set(None),
            exit_code: Set(None),
            log_path: Set(None),
            developer_instructions_path: Set(None),
            user_prompt_path: Set(None),
            agent_model: Set(None),
            agent_reasoning_effort: Set(None),
            effective_input_sha256: Set(None),
            effective_timeout_seconds: Set(Some(config.effective_timeout_seconds as i64)),
            effective_concurrency_group: Set(config
                .effective_concurrency_group
                .map(ToOwned::to_owned)),
            input_tokens: Set(None),
            cached_input_tokens: Set(None),
            output_tokens: Set(None),
            commit_required: Set(false),
            commit_outcome: Set(AgentCommitOutcome::NotEvaluated.as_storage().to_owned()),
            commit_shas: Set("[]".to_owned()),
            pr_requested: Set(false),
            pr_url: Set(None),
            cleanup_status: Set(AgentRunCleanupStatus::NotApplicable.as_storage().to_owned()),
            worktree_cleaned_at: Set(None),
            result_summary: Set(String::new()),
            semantic_postcondition_status: Set(SemanticPostconditionStatus::NotConfigured
                .as_storage()
                .to_owned()),
            semantic_postcondition_failures: Set("[]".to_owned()),
            started_at: Set(Some(now.clone())),
            finished_at: Set(None),
            created_at: Set(now.clone()),
            updated_at: Set(now.clone()),
            ..Default::default()
        }
        .insert(transaction.connection())
        .await
        .context("failed to create agent run")?;
        insert_contract_in_tx(
            transaction.connection(),
            project_id,
            run.id,
            config.purpose,
            config.launch_target,
            &AgentCapabilitySetV1::ordinary(),
            &now,
        )
        .await?;
        encoding::decode_in(transaction, run).await
    }
    pub(crate) async fn get_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<AgentRunView> {
        encoding::decode_in(
            transaction,
            load_model_in(transaction, project_id, run_id).await?,
        )
        .await
    }
    #[cfg(test)]
    pub(crate) async fn find_in(
        &self,
        transaction: &Transaction,
        run_id: i64,
    ) -> Result<Option<AgentRunView>> {
        match AgentRun::find_by_id(run_id)
            .one(transaction.connection())
            .await
            .context("failed to load agent run")?
        {
            Some(record) => Ok(Some(encoding::decode_in(transaction, record).await?)),
            None => Ok(None),
        }
    }
    pub(crate) async fn find_scoped_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
    ) -> Result<Option<AgentRunView>> {
        match AgentRun::find_by_id(run_id)
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .one(transaction.connection())
            .await
            .context("failed to load agent run")?
        {
            Some(record) => Ok(Some(encoding::decode_in(transaction, record).await?)),
            None => Ok(None),
        }
    }
    pub(crate) async fn list_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        running_only: bool,
        run_id: Option<i64>,
    ) -> Result<Vec<AgentRunView>> {
        let mut query = AgentRun::find()
            .filter(agent_run::Column::ProjectId.eq(project_id))
            .order_by_desc(agent_run::Column::CreatedAt)
            .order_by_desc(agent_run::Column::Id);
        if running_only {
            query =
                query.filter(agent_run::Column::Status.eq(AgentRunStatus::Running.as_storage()));
        }
        if let Some(run_id) = run_id {
            query = query.filter(agent_run::Column::Id.eq(run_id));
        }
        let records = query
            .all(transaction.connection())
            .await
            .context("failed to load agent runs")?;
        let mut views = Vec::with_capacity(records.len());
        for record in records {
            views.push(encoding::decode_in(transaction, record).await?);
        }
        Ok(views)
    }
    pub(crate) async fn change_in(
        &self,
        transaction: &Transaction,
        project_id: i64,
        run_id: i64,
        change: RunChange<'_>,
    ) -> Result<AgentRunView> {
        let run = load_model_in(transaction, project_id, run_id).await?;
        let mut active: AgentRunActiveModel = run.into();
        let mut spawned = false;
        let mut terminal = false;
        match change {
            RunChange::Launch(details) => {
                let details = *details;
                active.work_item_id = Set(details.work_item_id);
                active.memory_event_id = Set(None);
                active.command = Set(details.command);
                active.working_dir =
                    Set(details.workspace.working_dir.to_string_lossy().into_owned());
                let has_worktree = details.workspace.worktree_path.is_some();
                active.worktree_path = Set(details
                    .workspace
                    .worktree_path
                    .map(|path| path.to_string_lossy().into_owned()));
                active.branch_name = Set(details.workspace.branch_name);
                active.developer_instructions_path = Set(details.developer_instructions_path);
                active.user_prompt_path = Set(details.user_prompt_path);
                active.log_path = Set(details.log_path);
                active.agent_model = Set(details.agent_model);
                active.agent_reasoning_effort = Set(details
                    .agent_reasoning_effort
                    .map(|effort| effort.as_storage().to_owned()));
                active.system_prompt_event_id = Set(details.system_prompt_event_id);
                active.effective_input_sha256 = Set(Some(details.effective_input_sha256));
                active.effective_timeout_seconds =
                    Set(Some(details.effective_timeout_seconds as i64));
                active.commit_required = Set(details.commit_required);
                active.pr_requested = Set(details.pr_requested);
                active.cleanup_status = Set(if has_worktree {
                    AgentRunCleanupStatus::Pending.as_storage().to_owned()
                } else {
                    AgentRunCleanupStatus::NotApplicable.as_storage().to_owned()
                });
                active.updated_at = Set(utc_now());
                spawned = true;
            }
            RunChange::Finish {
                status,
                exit_code,
                result_summary,
            } => {
                let now = utc_now();

                active.status = Set(status.as_storage().to_owned());
                active.exit_code = Set(exit_code);
                active.result_summary = Set(result_summary);
                active.finished_at = Set(Some(now.clone()));
                active.updated_at = Set(now);
                terminal = true;
            }
            RunChange::ProcessId(process_id) => {
                active.process_id = Set(process_id);
                active.updated_at = Set(utc_now());
            }
            RunChange::TokenUsage(usage) => {
                active.input_tokens = Set(Some(usage.input_tokens));
                active.cached_input_tokens = Set(Some(usage.cached_input_tokens));
                active.output_tokens = Set(Some(usage.output_tokens));
                active.updated_at = Set(utc_now());
            }
            RunChange::Commit(evaluation) => {
                active.commit_outcome = Set(evaluation.outcome.as_storage().to_owned());
                active.commit_shas = Set(serde_json::to_string(&evaluation.shas)
                    .context("failed to encode automation commit SHAs")?);
                active.updated_at = Set(utc_now());
            }
            RunChange::Semantics(evaluation) => {
                active.semantic_postcondition_status =
                    Set(evaluation.status.as_storage().to_owned());
                active.semantic_postcondition_failures =
                    Set(serde_json::to_string(&evaluation.failures)
                        .context("failed to encode semantic postcondition failures")?);
                active.updated_at = Set(utc_now());
            }
            RunChange::PullRequest(pr_url) => {
                active.pr_url = Set(pr_url);
                active.updated_at = Set(utc_now());
            }
            RunChange::Cleanup {
                cleanup_status,
                worktree_cleaned_at,
            } => {
                active.cleanup_status = Set(cleanup_status.as_storage().to_owned());
                active.worktree_cleaned_at = Set(worktree_cleaned_at);
                active.updated_at = Set(utc_now());
            }
        }
        let updated = active
            .update(transaction.connection())
            .await
            .context("failed to update agent run")?;
        if spawned {
            mark_spawned_in_tx(transaction.connection(), project_id, run_id, &utc_now()).await?;
        }
        if terminal {
            mark_terminal_in_tx(transaction.connection(), project_id, run_id, &utc_now()).await?;
        }
        encoding::decode_in(transaction, updated).await
    }
}
async fn load_model_in(
    transaction: &Transaction,
    project_id: i64,
    run_id: i64,
) -> Result<AgentRunModel> {
    AgentRun::find_by_id(run_id)
        .filter(agent_run::Column::ProjectId.eq(project_id))
        .one(transaction.connection())
        .await
        .context("failed to load agent run")?
        .ok_or_else(|| report!("agent run {run_id} does not exist in this project"))
}
