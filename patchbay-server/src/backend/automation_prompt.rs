use crate::shared::view_models::{
    AUTOMATION_BLOCKED_LABEL_KEY, AutomationRunMutability, CLAIMED_FROM_STATE_LABEL_KEY,
    FEEDBACK_REQUESTED_LABEL_KEY, RevertStrategy, WorkItemView, WorkspaceMode,
};

const PATCHBAY_AGENT_INSTRUCTIONS: &str = include_str!("../../../AGENT_INSTRUCTIONS.md");

pub(crate) struct PromptContext<'a> {
    pub(crate) project_name: &'a str,
    pub(crate) system_prompt: &'a str,
    pub(crate) memory: &'a str,
    pub(crate) memory_event_id: Option<i64>,
    pub(crate) item: Option<&'a WorkItemView>,
    pub(crate) agent_id: &'a str,
    pub(crate) personality_description: Option<&'a str>,
    pub(crate) extra_prompt: Option<&'a str>,
    pub(crate) mutability: AutomationRunMutability,
    pub(crate) workspace_mode: WorkspaceMode,
    pub(crate) auto_commit: bool,
    pub(crate) commit_standard: &'a str,
    pub(crate) revert_strategy: RevertStrategy,
    pub(crate) create_pr: bool,
    pub(crate) git_command_policy: patchbay_types::AgentGitCommandPolicy,
    pub(crate) git_policy_workspace_mode: WorkspaceMode,
}

pub(crate) fn build_prompt(context: PromptContext<'_>) -> String {
    let mut prompt = format!(
        "# Patchbay Automation\n\nProject: {}\nAgent id: {}\n\n",
        context.project_name, context.agent_id
    );
    prompt.push_str("## Patchbay Agent Instructions\n\n");
    prompt.push_str(patchbay_agent_instructions_body());
    prompt.push_str("\n\n");
    if context.item.is_none() {
        prompt.push_str(
            "This run has no claimed item, so commands that require an item id must be given one explicitly.\n\n",
        );
    }
    if !context.system_prompt.trim().is_empty() {
        prompt.push_str("## Project System Prompt\n\n");
        prompt.push_str(context.system_prompt);
        prompt.push_str("\n\n");
    }
    prompt.push_str("## Project Memory\n\n");
    if let Some(memory_event_id) = context.memory_event_id {
        prompt.push_str(&format!("MemoryChanged event: #{memory_event_id}\n\n"));
    }
    if context.memory.trim().is_empty() {
        prompt.push_str("(empty)\n\n");
    } else {
        prompt.push_str(context.memory);
        prompt.push_str("\n\n");
    }
    if let Some(item) = context.item {
        prompt.push_str("## Claimed Work Item\n\n");
        let state = item.state.as_deref().unwrap_or("(none)");
        let claimed_from_state = claimed_from_state_label(item).unwrap_or(state);
        let labels = item
            .labels
            .iter()
            .map(|label| match label.value.as_deref() {
                Some(value) => format!("{}={value}", label.key),
                None => label.key.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        prompt.push_str(&format!(
            "Item: #{}\nTitle: {}\nState label: {}\nClaimed from state label: {}\nRelease behavior: `patchbay item release` restores the claimed-from state and adds `{}` so automation will not pick the item again until that label is removed.\nFeedback behavior: `patchbay item request-feedback --body ...` restores the claimed-from state and adds `{}` plus `{}` so automation waits for a user response.\nLabels: {}\nVersion: {}\n\n{}\n\n",
            item.id,
            item.title,
            state,
            claimed_from_state,
            AUTOMATION_BLOCKED_LABEL_KEY,
            FEEDBACK_REQUESTED_LABEL_KEY,
            AUTOMATION_BLOCKED_LABEL_KEY,
            if labels.is_empty() { "(none)" } else { &labels },
            item.version,
            item.description
        ));
    }
    prompt.push_str("## Git Commit And Revert Policy\n\n");
    prompt.push_str(&format!("Run mutability: {}\n", context.mutability));
    if context.mutability == AutomationRunMutability::ReadOnly {
        prompt.push_str("Workspace mode: read_only project checkout\n");
        prompt.push_str("Commit required: no\n");
        prompt.push_str("Pull request required: no\n\n");
        prompt.push_str(
            "- This run is read-only with respect to the project checkout. Do not edit project files, create or remove files under the workspace, change Git index or refs, create commits, push, reset, create branches/worktrees, or open pull requests.\n",
        );
        prompt.push_str(
            "- Patchbay metadata writes requested by the trigger are still allowed through the `patchbay` CLI/API, including item updates, labels, comments, progress, release state, and project memory.\n",
        );
        prompt.push_str(
            "- No commit is required. Report sandbox or Git blockers instead of working around read-only restrictions.\n",
        );
    } else {
        prompt.push_str(&format!("Workspace mode: {}\n", context.workspace_mode));
        match context.workspace_mode {
            WorkspaceMode::CurrentBranch => {
                prompt.push_str(&format!(
                    "Auto-commit: {}\n",
                    if context.auto_commit { "on" } else { "off" }
                ));
                prompt.push_str(&format!(
                    "Failure revert strategy: {}\n\n",
                    context.revert_strategy
                ));
                prompt.push_str(
                    "- At the start of work, inspect `git status --short` so you can distinguish pre-existing changes from your own changes.\n",
                );
                if context.auto_commit {
                    prompt.push_str(
                        "- After completed work and verification, inspect the diff, stage only the changes for this work item, and create a git commit before calling `patchbay item finish` or otherwise ending a successful prompt-directed run.\n",
                    );
                    prompt.push_str(
                        "- Generate the commit message from the completed diff and requested behavior. Follow the commit standard below and the repository's existing history.\n",
                    );
                    prompt.push_str(
                        "- If the project is not a git repository or there are no file changes to commit, say that in the finish report or final response instead of inventing a commit.\n",
                    );
                } else {
                    prompt.push_str(
                        "- Do not create a git commit solely for Patchbay after completed work; leave completed changes in the current branch and describe them in the finish report or final response.\n",
                    );
                }
                prompt.push_str(&format!(
                    "- If the work cannot be completed, revert all changes you made using the `{}` strategy before calling `patchbay item release --comment ...`.\n",
                    context.revert_strategy
                ));
                prompt.push_str(current_branch_revert_instruction(context.revert_strategy));
            }
            WorkspaceMode::GitBranch | WorkspaceMode::GitWorktree => {
                prompt.push_str(
                    "Auto-commit: always on for this workspace mode\nFailure revert strategy: not applicable\n\n",
                );
                prompt.push_str(
                    "- After completed work and verification, inspect the diff, stage the changes for this work item, and create a git commit before calling `patchbay item finish` or otherwise ending a successful prompt-directed run.\n",
                );
                prompt.push_str(
                    "- If the work cannot be completed, do not revert partial changes solely because the work is incomplete. Commit the useful partial work and then call `patchbay item release --comment ...` with what you tried and what remains.\n",
                );
                prompt.push_str(
                    "- If there are no file changes to commit, explain that in the finish or release report.\n",
                );
                prompt.push_str(
                    "- Generate commit messages from the diff and requested behavior. Follow the commit standard below and the repository's existing history.\n",
                );
            }
        }
    }
    prompt.push('\n');
    prompt.push_str("## Available Git Commands\n\n");
    prompt.push_str(&git_command_policy_prompt(
        &context.git_command_policy,
        context.git_policy_workspace_mode,
    ));
    prompt.push('\n');
    prompt.push_str("Commit standard:\n");
    if context.commit_standard.trim().is_empty() {
        prompt.push_str(
            "(not configured; infer the repository's existing commit message style from recent history)\n\n",
        );
    } else {
        prompt.push_str(context.commit_standard.trim());
        prompt.push_str("\n\n");
    }
    if let Some(personality_description) = context
        .personality_description
        .filter(|value| !value.trim().is_empty())
    {
        prompt.push_str("## Personality\n\n");
        prompt.push_str(personality_description);
        prompt.push_str("\n\n");
    }
    if let Some(extra_prompt) = context
        .extra_prompt
        .filter(|value| !value.trim().is_empty())
    {
        prompt.push_str("## Trigger Prompt\n\n");
        prompt.push_str(extra_prompt);
        prompt.push_str("\n\n");
    }
    if context.create_pr && context.mutability == AutomationRunMutability::Mutating {
        prompt.push_str(
            "## Pull Request\n\nCreate a pull request after the requested work is committed. \
             Patchbay will also attempt `gh pr create --fill` after your process exits.\n\n",
        );
    }
    prompt
}

fn patchbay_agent_instructions_body() -> &'static str {
    PATCHBAY_AGENT_INSTRUCTIONS
        .strip_prefix("# Patchbay Agent Instructions\n\n")
        .unwrap_or(PATCHBAY_AGENT_INSTRUCTIONS)
        .trim()
}

fn git_command_policy_prompt(
    policy: &patchbay_types::AgentGitCommandPolicy,
    workspace_mode: WorkspaceMode,
) -> String {
    let mut lines = Vec::new();
    if policy.add {
        lines.push("- `git add ...` is allowed; stage only changes for this work item.");
    }
    if policy.commit {
        lines.push("- `git commit ...` is allowed. Use `--no-verify`; Patchbay also enforces it.");
    }
    if policy.push {
        lines.push(
            "- `git push ...` is allowed for normal pushes. Force, mirror, prune, delete, empty-source delete-refspec, and `+ref` pushes are blocked.",
        );
    }
    if policy.reset {
        lines.push("- `git reset ...` is allowed within this project's configured limits.");
        if policy.allows_hard_reset(workspace_mode) {
            lines.push(
                "- `git reset --hard` is allowed because this run uses an isolated branch/worktree mode.",
            );
        } else {
            lines.push(
                "- `git reset --hard` is blocked for this workspace mode; preserve unrelated current-branch work.",
            );
        }
    }
    if lines.is_empty() {
        return "No mutable Git commands are available for this run. If a Git command is blocked, report that blocker in your progress or final response.\n".to_owned();
    }
    lines.push(
        "- Other mutable Git commands may be blocked by Codex rules or the Patchbay git wrapper. If blocked, report the exact command and reason.",
    );
    let mut text = lines.join("\n");
    text.push('\n');
    text
}

fn current_branch_revert_instruction(revert_strategy: RevertStrategy) -> &'static str {
    match revert_strategy {
        RevertStrategy::Manual => {
            "- Manual revert means reviewing the diff, restoring edited files by hand, and removing generated files you created while preserving unrelated pre-existing user changes.\n"
        }
        RevertStrategy::GitReset => {
            "- Git reset revert means using git reset/clean commands to return the workspace to the run's starting point. Check for unrelated pre-existing changes first and do not discard them silently.\n"
        }
    }
}

fn claimed_from_state_label(item: &WorkItemView) -> Option<&str> {
    item.labels
        .iter()
        .find(|label| label.key == CLAIMED_FROM_STATE_LABEL_KEY)
        .and_then(|label| label.value.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::view_models::{
        AgentGitCommandPolicy, AgentGitHardResetPolicy, WorkItemLabelView,
    };

    fn read_only_policy() -> AgentGitCommandPolicy {
        AgentGitCommandPolicy {
            add: false,
            commit: false,
            push: false,
            reset: false,
            hard_reset: AgentGitHardResetPolicy::Never,
        }
    }

    fn base_context<'a>() -> PromptContext<'a> {
        PromptContext {
            project_name: "demo",
            system_prompt: "",
            memory: "",
            memory_event_id: None,
            item: None,
            agent_id: "patchbay-run-1",
            personality_description: None,
            extra_prompt: None,
            mutability: AutomationRunMutability::Mutating,
            workspace_mode: WorkspaceMode::CurrentBranch,
            auto_commit: true,
            commit_standard: "",
            revert_strategy: RevertStrategy::Manual,
            create_pr: false,
            git_command_policy: AgentGitCommandPolicy::default(),
            git_policy_workspace_mode: WorkspaceMode::CurrentBranch,
        }
    }

    fn item() -> WorkItemView {
        WorkItemView {
            id: 42,
            project_id: 1,
            title: "Implement API relay".to_owned(),
            description: "Switch agent-facing CLI calls through HTTP.".to_owned(),
            state: Some("in_progress".to_owned()),
            labels: vec![
                WorkItemLabelView {
                    id: 1,
                    project_id: 1,
                    work_item_id: 42,
                    key: "state".to_owned(),
                    value: Some("in_progress".to_owned()),
                    created_at: "2026-06-14T00:00:00Z".to_owned(),
                    updated_at: "2026-06-14T00:00:00Z".to_owned(),
                },
                WorkItemLabelView {
                    id: 2,
                    project_id: 1,
                    work_item_id: 42,
                    key: CLAIMED_FROM_STATE_LABEL_KEY.to_owned(),
                    value: Some("ready".to_owned()),
                    created_at: "2026-06-14T00:00:00Z".to_owned(),
                    updated_at: "2026-06-14T00:00:00Z".to_owned(),
                },
            ],
            version: 3,
            claimed_by: Some("patchbay-run-1".to_owned()),
            claimed_at: None,
            claim_expires_at: None,
            claim_source: None,
            finished_at: None,
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            created_at: "2026-06-14T00:00:00Z".to_owned(),
            updated_at: "2026-06-14T00:00:00Z".to_owned(),
            comment_count: 0,
        }
    }

    #[test]
    fn prompt_includes_cli_context_without_agent_model_settings() {
        let item = item();
        let prompt = build_prompt(PromptContext {
            item: Some(&item),
            memory_event_id: Some(7),
            commit_standard: "Use short imperative subjects.",
            ..base_context()
        });

        assert!(prompt.contains("## Patchbay Agent Instructions"));
        assert!(
            prompt.contains("is the source of truth for work state, labels, and project memory")
        );
        assert!(prompt.contains("Patchbay-launched agents run through the Codex SDK"));
        assert!(
            prompt.contains("extra writable root or sandbox mode change would likely be needed")
        );
        assert!(prompt.contains("PATCHBAY_API_URL=<api-url>"));
        assert!(prompt.contains("PATCHBAY_CLAIMED_ITEM_ID=<item-id>"));
        assert!(prompt.contains("When `PATCHBAY_CLAIMED_ITEM_ID` is set"));
        assert!(
            prompt.contains(
                "`item list`, `item create`, and `item claim` do not use the claimed item"
            )
        );
        assert!(prompt.contains("patchbay item show [item-id] [--json]"));
        assert!(prompt.contains("patchbay item update [item-id]"));
        assert!(prompt.contains("--state <state-label>"));
        assert!(prompt.contains("patchbay label add [item-id]"));
        assert!(prompt.contains("State label: in_progress"));
        assert!(prompt.contains("Claimed from state label: ready"));
        assert!(prompt.contains("Release behavior: `patchbay item release` restores"));
        assert!(prompt.contains("Feedback behavior: `patchbay item request-feedback --body ...`"));
        assert!(prompt.contains(AUTOMATION_BLOCKED_LABEL_KEY));
        assert!(prompt.contains(FEEDBACK_REQUESTED_LABEL_KEY));
        assert!(prompt.contains("Labels: state=in_progress, patchbay:claimed-from-state=ready"));
        assert!(prompt.contains("--clear-agent-reasoning-effort"));
        assert!(prompt.contains("patchbay comment add [item-id]"));
        assert!(prompt.contains("patchbay automation runs [--limit N]"));
        assert!(prompt.contains("Project memory is tracked through Patchbay"));
        assert!(prompt.contains("not through Codex internal memory"));
        assert!(prompt.contains("full project memory snapshot"));
        assert!(prompt.contains("patchbay memory append --body"));
        assert!(prompt.contains("MemoryChanged event: #7"));
        assert!(!prompt.contains("Mode:"));
        assert!(!prompt.contains("PATCHBAY_DATABASE"));
        assert!(!prompt.contains("--project demo"));
        assert!(!prompt.contains("PATCHBAY_URL"));
        assert!(!prompt.contains("## Agent Model Settings"));
        assert!(!prompt.contains("Model: gpt-5-codex"));
        assert!(!prompt.contains("Reasoning effort: medium"));
        assert!(!prompt.contains("Use the Patchbay CLI for progress and final status"));
        assert!(prompt.contains("## Git Commit And Revert Policy"));
        assert!(prompt.contains("Workspace mode: current_branch"));
        assert!(prompt.contains("Auto-commit: on"));
        assert!(prompt.contains("Failure revert strategy: manual"));
        assert!(prompt.contains("create a git commit before calling `patchbay item finish`"));
        assert!(prompt.contains("revert all changes you made using the `manual` strategy"));
        assert!(prompt.contains("## Available Git Commands"));
        assert!(prompt.contains("`git add ...` is allowed"));
        assert!(prompt.contains("Patchbay also enforces it"));
        assert!(prompt.contains("Force, mirror, prune, delete"));
        assert!(prompt.contains("`git reset --hard` is blocked for this workspace mode"));
        assert!(prompt.contains("Use short imperative subjects."));
    }

    #[test]
    fn worktree_prompt_commits_incomplete_work_instead_of_reverting() {
        let prompt = build_prompt(PromptContext {
            mutability: AutomationRunMutability::Mutating,
            workspace_mode: WorkspaceMode::GitWorktree,
            auto_commit: false,
            revert_strategy: RevertStrategy::GitReset,
            git_policy_workspace_mode: WorkspaceMode::GitWorktree,
            ..base_context()
        });

        assert!(prompt.contains("Workspace mode: git_worktree"));
        assert!(prompt.contains("Auto-commit: always on for this workspace mode"));
        assert!(
            prompt.contains("do not revert partial changes solely because the work is incomplete")
        );
        assert!(prompt.contains("Commit the useful partial work"));
        assert!(prompt.contains("`git reset --hard` is allowed because this run uses an isolated"));
        assert!(
            prompt.contains("not configured; infer the repository's existing commit message style")
        );
    }

    #[test]
    fn read_only_prompt_disables_file_edits_commits_and_pull_requests() {
        let prompt = build_prompt(PromptContext {
            extra_prompt: Some("Inspect the item and update labels."),
            mutability: AutomationRunMutability::ReadOnly,
            workspace_mode: WorkspaceMode::GitWorktree,
            auto_commit: true,
            commit_standard: "Use short subjects.",
            revert_strategy: RevertStrategy::GitReset,
            create_pr: true,
            git_command_policy: read_only_policy(),
            git_policy_workspace_mode: WorkspaceMode::CurrentBranch,
            ..base_context()
        });

        assert!(prompt.contains("Run mutability: read_only"));
        assert!(prompt.contains("Do not edit project files"));
        assert!(
            prompt.contains("Patchbay metadata writes requested by the trigger are still allowed")
        );
        assert!(prompt.contains("No commit is required"));
        assert!(prompt.contains("No mutable Git commands are available for this run"));
        assert!(!prompt.contains("create a git commit before calling"));
        assert!(!prompt.contains("Patchbay will also attempt `gh pr create --fill`"));
        assert!(prompt.contains("Inspect the item and update labels."));
    }

    #[test]
    fn prompt_includes_non_empty_personality_before_trigger_prompt() {
        let prompt = build_prompt(PromptContext {
            personality_description: Some("Be concise and skeptical."),
            extra_prompt: Some("Inspect the item and update labels."),
            mutability: AutomationRunMutability::ReadOnly,
            auto_commit: false,
            git_command_policy: read_only_policy(),
            ..base_context()
        });

        assert!(prompt.contains("## Personality\n\nBe concise and skeptical.\n\n"));
        assert!(prompt.find("## Personality").unwrap() < prompt.find("## Trigger Prompt").unwrap());
    }

    #[test]
    fn empty_personality_description_is_behavior_neutral() {
        let prompt = build_prompt(PromptContext {
            personality_description: Some("   "),
            extra_prompt: Some("Inspect the item and update labels."),
            mutability: AutomationRunMutability::ReadOnly,
            auto_commit: false,
            git_command_policy: read_only_policy(),
            ..base_context()
        });

        assert!(!prompt.contains("## Personality"));
        assert!(prompt.contains("## Trigger Prompt"));
    }
}
