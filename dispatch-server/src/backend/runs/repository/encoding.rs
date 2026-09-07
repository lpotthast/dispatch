use crate::backend::{
    entities::agent_run::AgentRunModel, projects, runs::launch::model::PersistedLaunchContract,
};
use dispatch_types::*;
use rootcause::{Result, prelude::*};
use std::str::FromStr;
pub(crate) fn model_to_view_with_contract(
    run: AgentRunModel,
    contract: Option<&PersistedLaunchContract>,
) -> Result<AgentRunView> {
    Ok(AgentRunView {
        knowledge_job_id: run.knowledge_job_id,
        id: run.id,
        project_id: run.project_id,
        work_item_id: run.work_item_id,
        run_kind: AgentRunKind::from_str(&run.run_kind)?,
        purpose: run
            .purpose
            .as_deref()
            .map(AgentRunPurposeV1::from_str)
            .transpose()?,
        launch_target: contract.map(|contract| contract.target.view()),
        launch_resolution: contract.map(|contract| contract.resolution.view()),
        knowledge_revision: run.knowledge_revision,
        source_baseline_id: run.source_baseline_id,
        source_snapshot_id: run.source_snapshot_id,
        knowledge_view_sha256: run.knowledge_view_sha256,
        input_overlay_sha256: run.input_overlay_sha256,
        source_authority_kind: run
            .source_authority_kind
            .as_deref()
            .map(AgentRunSourceAuthority::from_str)
            .transpose()?,
        source_ref_name: run.source_ref_name,
        source_raw_head: run.source_raw_head,
        trigger_id: run.trigger_id,
        trigger_name: projects::normalize_optional(run.trigger_name),
        trigger_revision_id: run.trigger_revision_id,
        personality_revision_id: run.personality_revision_id,
        system_prompt_event_id: run.system_prompt_event_id,
        tool_name: AgentToolName::from_str(&run.tool_name)?,
        mutability: AutomationRunMutability::from_str(&run.mutability)?,
        status: AgentRunStatus::from_str(&run.status)?,
        command: run.command,
        working_dir: run.working_dir,
        worktree_path: run.worktree_path,
        branch_name: run.branch_name,
        process_id: run.process_id,
        exit_code: run.exit_code,
        log_path: run.log_path,
        developer_instructions_path: run.developer_instructions_path,
        user_prompt_path: run.user_prompt_path,
        agent_model: projects::normalize_optional(run.agent_model),
        agent_reasoning_effort: run
            .agent_reasoning_effort
            .as_deref()
            .map(str::parse::<AgentReasoningEffort>)
            .transpose()?,
        effective_input_sha256: run.effective_input_sha256,
        effective_timeout_seconds: run
            .effective_timeout_seconds
            .map(|value| u64::try_from(value).context("invalid effective automation timeout"))
            .transpose()?,
        effective_concurrency_group: projects::normalize_optional(run.effective_concurrency_group),
        token_usage: token_usage_from_columns(
            run.input_tokens,
            run.cached_input_tokens,
            run.output_tokens,
        ),
        commit_required: run.commit_required,
        commit_outcome: AgentCommitOutcome::from_str(&run.commit_outcome)?,
        commit_shas: parse_commit_shas(&run.commit_shas)?,
        pr_requested: run.pr_requested,
        pr_url: run.pr_url,
        cleanup_status: AgentRunCleanupStatus::from_str(&run.cleanup_status)?,
        worktree_cleaned_at: run.worktree_cleaned_at,
        result_summary: run.result_summary,
        semantic_postcondition_status: SemanticPostconditionStatus::from_str(
            &run.semantic_postcondition_status,
        )?,
        semantic_postcondition_failures: serde_json::from_str::<Vec<PostconditionFailureView>>(
            &run.semantic_postcondition_failures,
        )
        .context("invalid semantic postcondition failure details")?,
        started_at: run.started_at,
        finished_at: run.finished_at,
        created_at: run.created_at,
        updated_at: run.updated_at,
    })
}

fn token_usage_from_columns(
    input_tokens: Option<i64>,
    cached_input_tokens: Option<i64>,
    output_tokens: Option<i64>,
) -> Option<AgentRunTokenUsageView> {
    let input_tokens = input_tokens?;
    let cached_input_tokens = cached_input_tokens.unwrap_or_default();
    let output_tokens = output_tokens?;
    Some(AgentRunTokenUsageView {
        input_tokens,
        cached_input_tokens,
        output_tokens,
        total_tokens: input_tokens.saturating_add(output_tokens),
    })
}

fn parse_commit_shas(raw: &str) -> Result<Vec<String>> {
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(raw).context("failed to decode automation commit SHAs")?)
}

pub(crate) async fn decode_in(
    transaction: &crate::backend::storage::Transaction,
    run: AgentRunModel,
) -> Result<AgentRunView> {
    let contract = crate::backend::runs::launch::repository::load_contract(
        transaction.connection(),
        run.project_id,
        run.id,
    )
    .await?;
    if run.purpose.is_some() && contract.is_none() {
        bail!(
            "post-049 agent run {} is missing its launch contract",
            run.id
        );
    }
    model_to_view_with_contract(run, contract.as_ref())
}
