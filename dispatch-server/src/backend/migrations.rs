use std::{
    fs,
    path::{Path, PathBuf},
};

use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::Statement;
use sha2::{Digest, Sha256};

pub(crate) const REMOVED_REFINEMENT_CONCURRENCY_COLUMN: &str =
    "allow_refinement_agents_during_editing";

#[derive(Iden)]
enum CrudkitValidation {
    #[iden = "CrudkitValidation"]
    Table,
    Id,
    ResourceName,
    EntityId,
    ValidatorName,
    ValidatorVersion,
    ViolationSeverity,
    ViolationMessage,
    CreatedAt,
}

#[derive(DeriveIden)]
enum Projects {
    Table,
    Id,
    Name,
    DisplayName,
    Path,
    PathExists,
    PathCheckedAt,
    SystemPrompt,
    Memory,
    WorkspaceMode,
    MaxCodeEditAgents,
    MaxReadOnlyAgents,
    CreatePr,
    AutoCommit,
    CommitStandard,
    RevertStrategy,
    StaleClaimMinutes,
    WorktreeCleanupPolicy,
    DefaultAgentTool,
    DefaultAgentModel,
    DefaultAgentReasoningEffort,
    AgentSandboxMode,
    AgentExtraWritableRoots,
    AgentGitCommandPolicy,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkItems {
    Table,
    Id,
    ProjectId,
    WorkGroupId,
    Title,
    Description,
    State,
    ClaimedBy,
    ClaimedAt,
    ClaimExpiresAt,
    FinishedAt,
    AgentModelOverride,
    AgentReasoningEffortOverride,
    Version,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkItemGroups {
    Table,
    Id,
    ProjectId,
    GroupKey,
    Name,
    ActorId,
    AgentRunId,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkItemLabels {
    Table,
    Id,
    ProjectId,
    WorkItemId,
    LabelKey,
    LabelValue,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkItemRelationships {
    Table,
    Id,
    ProjectId,
    SourceWorkItemId,
    TargetWorkItemId,
    Kind,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum SwimLanes {
    Table,
    Id,
    ProjectId,
    Identifier,
    Name,
    Position,
    Filter,
    ItemOrder,
    CanCreateItems,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum WorkItemStates {
    Table,
    Id,
    ProjectId,
    Identifier,
    Name,
    Position,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Personalities {
    Table,
    Id,
    ProjectId,
    Name,
    PersonalityDescription,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum Comments {
    Table,
    Id,
    WorkItemId,
    AuthorType,
    AuthorName,
    Body,
    CreatedAt,
}

#[derive(DeriveIden)]
enum WorkItemEvents {
    Table,
    Id,
    ProjectId,
    WorkItemId,
    EventType,
    Body,
    ActorType,
    ActorId,
    AgentRunId,
    CreatedAt,
}

#[derive(DeriveIden)]
enum AgentTools {
    Table,
    Id,
    ToolName,
    ExecutablePath,
    DiscoveredPath,
    LastDiscoveredAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AgentRuns {
    Table,
    Id,
    ProjectId,
    WorkItemId,
    MemoryEventId,
    TriggerId,
    TriggerName,
    Mode,
    ToolName,
    Mutability,
    Status,
    Command,
    WorkingDir,
    WorktreePath,
    BranchName,
    ProcessId,
    ExitCode,
    LogPath,
    DeveloperInstructionsPath,
    UserPromptPath,
    AgentModel,
    AgentReasoningEffort,
    InputTokens,
    CachedInputTokens,
    OutputTokens,
    CommitRequired,
    CommitOutcome,
    CommitShas,
    PrRequested,
    PrUrl,
    CleanupStatus,
    WorktreeCleanedAt,
    ResultSummary,
    StartedAt,
    FinishedAt,
    CreatedAt,
    UpdatedAt,
}

#[derive(DeriveIden)]
enum AutomationTriggers {
    Table,
    Id,
    ProjectId,
    Name,
    Enabled,
    Activation,
    Effect,
    Schedule,
    Mode,
    ToolName,
    Mutability,
    Prompt,
    WorkItemSelector,
    Priority,
    EvaluationCount,
    PendingEvaluationCount,
    LastEvaluationQueuedAt,
    LastEvaluatedAt,
    NextEvaluationAt,
    LastEventId,
    CreatedAt,
    UpdatedAt,
}

pub struct Migrator;

const DEFAULT_AGENT_GIT_COMMAND_POLICY: &str =
    r#"{"add":true,"commit":true,"push":true,"reset":true,"hard_reset":"isolated_workspaces"}"#;
const OLD_DEFAULT_WORK_ITEM_SELECTOR: &str =
    r#"{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}}]}"#;
const PRE_FEEDBACK_DEFAULT_WORK_ITEM_SELECTOR: &str = r#"{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}},{"column_name":"needs-refinement","operator":"=","value":{"Bool":false}},{"column_name":"needs-verification","operator":"=","value":{"Bool":false}}]}"#;
const DEFAULT_WORK_ITEM_SELECTOR: &str = r#"{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}},{"column_name":"needs-refinement","operator":"=","value":{"Bool":false}},{"column_name":"needs-verification","operator":"=","value":{"Bool":false}},{"column_name":"dispatch:feedback-requested","operator":"=","value":{"Bool":false}}]}"#;
const DEFAULT_REFINEMENT_SELECTOR: &str =
    r#"{"All":[{"column_name":"needs-refinement","operator":"=","value":{"Bool":true}}]}"#;
const DEFAULT_VERIFICATION_SELECTOR: &str =
    r#"{"All":[{"column_name":"needs-verification","operator":"=","value":{"Bool":true}}]}"#;
const DEFAULT_REFINEMENT_PROMPT: &str = r#"You are the needs-refinement executor for the claimed Dispatch work item.

Goal: turn a rough or under-specified item into implementation-ready work. Do not implement the work.

Required workflow:
- Re-read the item, comments, labels, and any relevant project memory before editing it.
- Clarify the title and description so a later implementation agent can act without guessing. Prefer concrete scope, non-goals, acceptance criteria, suggested approach, verification expectations, and open questions only when human input is genuinely required.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-refinement` label when refinement is complete. Keep or add `needs-verification` only when the refined item should be checked before implementation.
- Add a concise progress comment summarizing what changed.

Do not call `dispatch item finish` for successful refinement, and do not call `dispatch item release` after successful refinement. Let Dispatch release the temporary claim after your final response. If the item cannot be refined without a human decision, leave `needs-refinement` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user."#;
const PRE_FEEDBACK_REFINEMENT_PROMPT: &str = r#"You are the needs-refinement executor for the claimed Dispatch work item.

Goal: turn a rough or under-specified item into implementation-ready work. Do not implement the work.

Required workflow:
- Re-read the item, comments, labels, and any relevant project memory before editing it.
- Clarify the title and description so a later implementation agent can act without guessing. Prefer concrete scope, non-goals, acceptance criteria, suggested approach, verification expectations, and open questions only when human input is genuinely required.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-refinement` label when refinement is complete. Keep or add `needs-verification` only when the refined item should be checked before implementation.
- Add a concise progress comment summarizing what changed.

Do not call `dispatch item finish` for successful refinement, and do not call `dispatch item release` after successful refinement. Let Dispatch release the temporary claim after your final response. If the item cannot be refined without a human decision, leave `needs-refinement` in place and call `dispatch item release --comment ...` with the blocker."#;
const DEFAULT_VERIFICATION_PROMPT: &str = r#"You are the needs-verification executor for the claimed Dispatch work item.

Goal: verify whether the item is necessary, accurate, and ready for a later implementation agent. Do not implement the work.

Required workflow:
- Re-read the item, comments, labels, and any relevant project memory. Inspect repository files only as needed to verify facts.
- Update the title or description with verification findings, corrected scope, risks, acceptance criteria, and verification notes that future workers need.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-verification` label when verification is complete. Add `needs-refinement` only if the item still needs story-shaping before implementation.
- Add a concise progress comment with the verification result.

If verification shows the work is unnecessary, explain why in the item and a comment. You may move the item to a project-specific terminal state only when that state already exists in the project's visible workflow vocabulary; do not invent or hardcode a state name. Use `dispatch label suggestions --json`, existing item labels, comments, or project docs to infer that vocabulary.

Do not call `dispatch item finish` for successful verification, and do not call `dispatch item release` after successful verification. Let Dispatch release the temporary claim after your final response. If verification needs a user decision, leave `needs-verification` in place and call `dispatch item request-feedback --body ...` with the concrete question for the user. If verification is blocked by a technical or environment issue rather than missing user input, call `dispatch item release --comment ...` with the blocker."#;
const PRE_FEEDBACK_VERIFICATION_PROMPT: &str = r#"You are the needs-verification executor for the claimed Dispatch work item.

Goal: verify whether the item is necessary, accurate, and ready for a later implementation agent. Do not implement the work.

Required workflow:
- Re-read the item, comments, labels, and any relevant project memory. Inspect repository files only as needed to verify facts.
- Update the title or description with verification findings, corrected scope, risks, acceptance criteria, and verification notes that future workers need.
- Update labels when they improve routing, priority, status, environment, or follow-up handling.
- Remove the `needs-verification` label when verification is complete. Add `needs-refinement` only if the item still needs story-shaping before implementation.
- Add a concise progress comment with the verification result.

If verification shows the work is unnecessary, explain why in the item and a comment. You may move the item to a project-specific terminal state only when that state already exists in the project's visible workflow vocabulary; do not invent or hardcode a state name. Use `dispatch label suggestions --json`, existing item labels, comments, or project docs to infer that vocabulary.

Do not call `dispatch item finish` for successful verification, and do not call `dispatch item release` after successful verification. Let Dispatch release the temporary claim after your final response. If verification is blocked, leave `needs-verification` in place and call `dispatch item release --comment ...` with the blocker."#;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(CreatePhaseOneTables),
            Box::new(AddPhaseTwoCoordination),
            Box::new(AddProjectContext),
            Box::new(AddPhaseThreeAutomation),
            Box::new(AddPhaseThreeWorkspacePolicy),
            Box::new(AddPhaseFourHardening),
            Box::new(AddProjectDefaultAgentTool),
            Box::new(MoveRunSettingsIntoProjects),
            Box::new(DropClaudeCodeSupport),
            Box::new(RenameProjectRepoPath),
            Box::new(AddProjectPathStatus),
            Box::new(AddAutomationRunConfiguration),
            Box::new(RemoveAutomationTriggerDryRun),
            Box::new(AddAutomationRunTriggerOrigin),
            Box::new(AddProjectMemoryEvents),
            Box::new(RemoveWorkItemAutomationClaimable),
            Box::new(AddLabelsAndSwimLanes),
            Box::new(AddAutomationWorkItemSelectors),
            Box::new(RenameAutomationActivationAndRequireScheduleTransientName),
            Box::new(AddAutomationWorkItemSelectorsTransientName),
            Box::new(RenameAutomationActivationAndRequireSchedule),
            Box::new(AddWorkItemStateLabelReadView),
            Box::new(AddSwimLaneCreateItemFlag),
            Box::new(AddProjectAgentExtraWritableRoots),
            Box::new(AddProjectAgentSandboxMode),
            Box::new(DecoupleStatesAndSwimLanes),
            Box::new(AddProjectCommitPolicy),
            Box::new(AddProjectAgentGitCommandPolicy),
            Box::new(AddAutomationRunCommitOutcomes),
            Box::new(AddAutomationRunTokenUsage),
            Box::new(AddRefinerVerifierAutomations),
            Box::new(RemoveAutomationModes),
            Box::new(RemoveRefinementConcurrencySetting),
            Box::new(AddFeedbackRequestWorkflow),
            Box::new(AddAutomationRunMutability),
            Box::new(AddWorkItemRelationships),
            Box::new(AddAutomationPersonalities),
            Box::new(SeparateAutomationRunInputs),
            Box::new(AddAutomationWorkflowSupport),
            Box::new(AddWorkItemGroups),
        ]
    }
}

struct CreatePhaseOneTables;

impl MigrationName for CreatePhaseOneTables {
    fn name(&self) -> &str {
        "migrations"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for CreatePhaseOneTables {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_crudkit_validation(manager).await?;
        create_projects(manager).await?;
        create_work_items(manager).await?;
        create_comments(manager).await?;
        create_work_item_events(manager).await?;
        create_read_views(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_views(manager).await?;
        manager
            .drop_table(Table::drop().table(WorkItemEvents::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Comments::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(WorkItems::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(Projects::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(CrudkitValidation::Table).to_owned())
            .await
    }
}

async fn create_crudkit_validation(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(CrudkitValidation::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(CrudkitValidation::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::ResourceName)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::EntityId)
                        .json_binary()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::ValidatorName)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::ValidatorVersion)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::ViolationSeverity)
                        .string_len(16)
                        .not_null(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::ViolationMessage)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(CrudkitValidation::CreatedAt)
                        .string()
                        .not_null(),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_crudkit_validation_resource_name")
                .table(CrudkitValidation::Table)
                .col(CrudkitValidation::ResourceName)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn create_projects(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Projects::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Projects::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Projects::Name)
                        .string()
                        .not_null()
                        .unique_key(),
                )
                .col(ColumnDef::new(Projects::DisplayName).string().not_null())
                .col(ColumnDef::new(Projects::Path).string().null())
                .col(
                    ColumnDef::new(Projects::PathExists)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(ColumnDef::new(Projects::PathCheckedAt).string().null())
                .col(
                    ColumnDef::new(Projects::SystemPrompt)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(Projects::Memory)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(Projects::WorkspaceMode)
                        .string()
                        .not_null()
                        .default("current_branch"),
                )
                .col(
                    ColumnDef::new(Projects::MaxCodeEditAgents)
                        .big_integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    ColumnDef::new(Projects::MaxReadOnlyAgents)
                        .big_integer()
                        .not_null()
                        .default(2),
                )
                .col(
                    ColumnDef::new(Projects::CreatePr)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(Projects::AutoCommit)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(Projects::CommitStandard)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(Projects::RevertStrategy)
                        .string()
                        .not_null()
                        .default("manual"),
                )
                .col(
                    ColumnDef::new(Projects::StaleClaimMinutes)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(Projects::WorktreeCleanupPolicy)
                        .string()
                        .not_null()
                        .default("manual"),
                )
                .col(
                    ColumnDef::new(Projects::DefaultAgentTool)
                        .string()
                        .not_null()
                        .default("codex"),
                )
                .col(ColumnDef::new(Projects::DefaultAgentModel).string().null())
                .col(
                    ColumnDef::new(Projects::DefaultAgentReasoningEffort)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(Projects::AgentSandboxMode)
                        .string()
                        .not_null()
                        .default("workspace_write"),
                )
                .col(
                    ColumnDef::new(Projects::AgentExtraWritableRoots)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(Projects::AgentGitCommandPolicy)
                        .text()
                        .not_null()
                        .default(DEFAULT_AGENT_GIT_COMMAND_POLICY),
                )
                .col(
                    ColumnDef::new(Projects::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(Projects::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .to_owned(),
        )
        .await
}

async fn create_work_items(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkItems::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkItems::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkItems::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(WorkItems::Title).string().not_null())
                .col(ColumnDef::new(WorkItems::Description).text().not_null())
                .col(
                    ColumnDef::new(WorkItems::State)
                        .string()
                        .not_null()
                        .default("open"),
                )
                .col(ColumnDef::new(WorkItems::ClaimedBy).string().null())
                .col(ColumnDef::new(WorkItems::ClaimedAt).string().null())
                .col(ColumnDef::new(WorkItems::ClaimExpiresAt).string().null())
                .col(ColumnDef::new(WorkItems::FinishedAt).string().null())
                .col(
                    ColumnDef::new(WorkItems::AgentModelOverride)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(WorkItems::AgentReasoningEffortOverride)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(WorkItems::Version)
                        .big_integer()
                        .not_null()
                        .default(1),
                )
                .col(
                    ColumnDef::new(WorkItems::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(WorkItems::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_items_project_id")
                        .from(WorkItems::Table, WorkItems::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_work_items_project_state")
                .table(WorkItems::Table)
                .col(WorkItems::ProjectId)
                .col(WorkItems::State)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

struct AddPhaseTwoCoordination;

impl MigrationName for AddPhaseTwoCoordination {
    fn name(&self) -> &str {
        "m20260612_000002_add_phase_two_coordination"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddPhaseTwoCoordination {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column_if_missing(manager, "work_items", "claimed_by", "TEXT").await?;
        add_column_if_missing(manager, "work_items", "claimed_at", "TEXT").await?;
        add_column_if_missing(manager, "work_items", "claim_expires_at", "TEXT").await?;
        add_column_if_missing(manager, "work_items", "finished_at", "TEXT").await?;
        drop_read_view(manager, "work_items_read_view").await?;
        create_read_view(manager, "work_items", "work_items_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        create_read_view(manager, "work_items", "work_items_read_view").await
    }
}

struct AddProjectContext;

impl MigrationName for AddProjectContext {
    fn name(&self) -> &str {
        "m20260612_000003_add_project_context"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectContext {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column_if_missing(
            manager,
            "projects",
            "system_prompt",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        add_column_if_missing(manager, "projects", "memory", "TEXT NOT NULL DEFAULT ''").await?;
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct AddPhaseThreeAutomation;

impl MigrationName for AddPhaseThreeAutomation {
    fn name(&self) -> &str {
        "m20260612_000004_add_phase_three_automation"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddPhaseThreeAutomation {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_agent_tools(manager).await?;
        create_agent_runs(manager).await?;
        create_automation_triggers(manager).await?;
        create_read_view(manager, "agent_tools", "agent_tools_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_read_view(manager, "agent_tools_read_view").await?;
        manager
            .drop_table(Table::drop().table(AutomationTriggers::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AgentRuns::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(AgentTools::Table).to_owned())
            .await
    }
}

async fn create_comments(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Comments::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Comments::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Comments::WorkItemId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(Comments::AuthorType).string().not_null())
                .col(ColumnDef::new(Comments::AuthorName).string().null())
                .col(ColumnDef::new(Comments::Body).text().not_null())
                .col(
                    ColumnDef::new(Comments::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_comments_work_item_id")
                        .from(Comments::Table, Comments::WorkItemId)
                        .to(WorkItems::Table, WorkItems::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_comments_work_item_created")
                .table(Comments::Table)
                .col(Comments::WorkItemId)
                .col(Comments::CreatedAt)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn create_work_item_events(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkItemEvents::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkItemEvents::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkItemEvents::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemEvents::WorkItemId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(WorkItemEvents::EventType)
                        .string()
                        .not_null(),
                )
                .col(ColumnDef::new(WorkItemEvents::Body).text().not_null())
                .col(ColumnDef::new(WorkItemEvents::ActorType).string().null())
                .col(ColumnDef::new(WorkItemEvents::ActorId).string().null())
                .col(
                    ColumnDef::new(WorkItemEvents::AgentRunId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(WorkItemEvents::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_events_project_id")
                        .from(WorkItemEvents::Table, WorkItemEvents::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_events_work_item_id")
                        .from(WorkItemEvents::Table, WorkItemEvents::WorkItemId)
                        .to(WorkItems::Table, WorkItems::Id)
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_work_item_events_project_created")
                .table(WorkItemEvents::Table)
                .col(WorkItemEvents::ProjectId)
                .col(WorkItemEvents::CreatedAt)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn create_agent_tools(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(AgentTools::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(AgentTools::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(AgentTools::ToolName)
                        .string()
                        .not_null()
                        .unique_key(),
                )
                .col(ColumnDef::new(AgentTools::ExecutablePath).string().null())
                .col(ColumnDef::new(AgentTools::DiscoveredPath).string().null())
                .col(ColumnDef::new(AgentTools::LastDiscoveredAt).string().null())
                .col(
                    ColumnDef::new(AgentTools::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(AgentTools::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .to_owned(),
        )
        .await
}

async fn create_agent_runs(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(AgentRuns::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(AgentRuns::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(AgentRuns::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(AgentRuns::WorkItemId).big_integer().null())
                .col(
                    ColumnDef::new(AgentRuns::MemoryEventId)
                        .big_integer()
                        .null(),
                )
                .col(ColumnDef::new(AgentRuns::TriggerId).big_integer().null())
                .col(ColumnDef::new(AgentRuns::TriggerName).string().null())
                .col(ColumnDef::new(AgentRuns::Mode).string().not_null())
                .col(ColumnDef::new(AgentRuns::ToolName).string().not_null())
                .col(
                    ColumnDef::new(AgentRuns::Mutability)
                        .string()
                        .not_null()
                        .default("mutating"),
                )
                .col(ColumnDef::new(AgentRuns::Status).string().not_null())
                .col(ColumnDef::new(AgentRuns::Command).text().not_null())
                .col(ColumnDef::new(AgentRuns::WorkingDir).string().not_null())
                .col(ColumnDef::new(AgentRuns::WorktreePath).string().null())
                .col(ColumnDef::new(AgentRuns::BranchName).string().null())
                .col(ColumnDef::new(AgentRuns::ProcessId).big_integer().null())
                .col(ColumnDef::new(AgentRuns::ExitCode).big_integer().null())
                .col(ColumnDef::new(AgentRuns::LogPath).string().null())
                .col(
                    ColumnDef::new(AgentRuns::DeveloperInstructionsPath)
                        .string()
                        .null(),
                )
                .col(ColumnDef::new(AgentRuns::UserPromptPath).string().null())
                .col(ColumnDef::new(AgentRuns::AgentModel).string().null())
                .col(
                    ColumnDef::new(AgentRuns::AgentReasoningEffort)
                        .string()
                        .null(),
                )
                .col(ColumnDef::new(AgentRuns::InputTokens).big_integer().null())
                .col(
                    ColumnDef::new(AgentRuns::CachedInputTokens)
                        .big_integer()
                        .null(),
                )
                .col(ColumnDef::new(AgentRuns::OutputTokens).big_integer().null())
                .col(
                    ColumnDef::new(AgentRuns::CommitRequired)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(AgentRuns::CommitOutcome)
                        .string()
                        .not_null()
                        .default("not_evaluated"),
                )
                .col(
                    ColumnDef::new(AgentRuns::CommitShas)
                        .text()
                        .not_null()
                        .default("[]"),
                )
                .col(
                    ColumnDef::new(AgentRuns::PrRequested)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(ColumnDef::new(AgentRuns::PrUrl).string().null())
                .col(
                    ColumnDef::new(AgentRuns::CleanupStatus)
                        .string()
                        .not_null()
                        .default("not_applicable"),
                )
                .col(ColumnDef::new(AgentRuns::WorktreeCleanedAt).string().null())
                .col(
                    ColumnDef::new(AgentRuns::ResultSummary)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(ColumnDef::new(AgentRuns::StartedAt).string().null())
                .col(ColumnDef::new(AgentRuns::FinishedAt).string().null())
                .col(
                    ColumnDef::new(AgentRuns::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(AgentRuns::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_agent_runs_project_id")
                        .from(AgentRuns::Table, AgentRuns::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_agent_runs_work_item_id")
                        .from(AgentRuns::Table, AgentRuns::WorkItemId)
                        .to(WorkItems::Table, WorkItems::Id)
                        .on_delete(ForeignKeyAction::SetNull),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_agent_runs_project_status")
                .table(AgentRuns::Table)
                .col(AgentRuns::ProjectId)
                .col(AgentRuns::Status)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn create_automation_triggers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(AutomationTriggers::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(AutomationTriggers::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(AutomationTriggers::Name).string().not_null())
                .col(
                    ColumnDef::new(AutomationTriggers::Enabled)
                        .boolean()
                        .not_null()
                        .default(true),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::Activation)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::Effect)
                        .string()
                        .not_null()
                        .default("consume_work"),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::Schedule)
                        .string()
                        .not_null()
                        .default("@every 15s"),
                )
                .col(ColumnDef::new(AutomationTriggers::Mode).string().not_null())
                .col(
                    ColumnDef::new(AutomationTriggers::ToolName)
                        .string()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::Mutability)
                        .string()
                        .not_null()
                        .default("mutating"),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::Prompt)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::WorkItemSelector)
                        .text()
                        .null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::Priority)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::EvaluationCount)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::PendingEvaluationCount)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::LastEvaluationQueuedAt)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::LastEvaluatedAt)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::NextEvaluationAt)
                        .string()
                        .null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::LastEventId)
                        .big_integer()
                        .null(),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(AutomationTriggers::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_automation_triggers_project_id")
                        .from(AutomationTriggers::Table, AutomationTriggers::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_automation_triggers_project_activation")
                .table(AutomationTriggers::Table)
                .col(AutomationTriggers::ProjectId)
                .col(AutomationTriggers::Activation)
                .if_not_exists()
                .to_owned(),
        )
        .await
}

struct AddPhaseThreeWorkspacePolicy;

impl MigrationName for AddPhaseThreeWorkspacePolicy {
    fn name(&self) -> &str {
        "m20260612_000005_add_phase_three_workspace_policy"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddPhaseThreeWorkspacePolicy {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_project_run_settings_columns(manager).await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "pr_requested",
            "BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(manager, "agent_runs", "pr_url", "TEXT").await?;
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct AddPhaseFourHardening;

impl MigrationName for AddPhaseFourHardening {
    fn name(&self) -> &str {
        "m20260612_000006_add_phase_four_hardening"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddPhaseFourHardening {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_project_run_settings_columns(manager).await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "cleanup_status",
            "TEXT NOT NULL DEFAULT 'not_applicable'",
        )
        .await?;
        add_column_if_missing(manager, "agent_runs", "worktree_cleaned_at", "TEXT").await?;
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct AddProjectDefaultAgentTool;

impl MigrationName for AddProjectDefaultAgentTool {
    fn name(&self) -> &str {
        "m20260612_000007_add_project_default_agent_tool"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectDefaultAgentTool {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_project_run_settings_columns(manager).await?;
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct MoveRunSettingsIntoProjects;

impl MigrationName for MoveRunSettingsIntoProjects {
    fn name(&self) -> &str {
        "m20260613_000008_move_run_settings_into_projects"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for MoveRunSettingsIntoProjects {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_project_run_settings_columns(manager).await?;
        if table_exists(manager, "project_settings").await? {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    r#"
                    UPDATE "projects"
                    SET
                        "workspace_mode" = COALESCE((
                            SELECT "workspace_mode"
                            FROM "project_settings"
                            WHERE "project_settings"."project_id" = "projects"."id"
                        ), "workspace_mode"),
                        "max_code_edit_agents" = COALESCE((
                            SELECT "max_code_edit_agents"
                            FROM "project_settings"
                            WHERE "project_settings"."project_id" = "projects"."id"
                        ), "max_code_edit_agents"),
                        "create_pr" = COALESCE((
                            SELECT "create_pr"
                            FROM "project_settings"
                            WHERE "project_settings"."project_id" = "projects"."id"
                        ), "create_pr"),
                        "stale_claim_minutes" = COALESCE((
                            SELECT "stale_claim_minutes"
                            FROM "project_settings"
                            WHERE "project_settings"."project_id" = "projects"."id"
                        ), "stale_claim_minutes"),
                        "worktree_cleanup_policy" = COALESCE((
                            SELECT "worktree_cleanup_policy"
                            FROM "project_settings"
                            WHERE "project_settings"."project_id" = "projects"."id"
                        ), "worktree_cleanup_policy"),
                        "default_agent_tool" = COALESCE((
                            SELECT "default_agent_tool"
                            FROM "project_settings"
                            WHERE "project_settings"."project_id" = "projects"."id"
                        ), "default_agent_tool")
                    WHERE EXISTS (
                        SELECT 1
                        FROM "project_settings"
                        WHERE "project_settings"."project_id" = "projects"."id"
                    );
                    "#,
                ))
                .await?;
            drop_read_view(manager, "project_settings_read_view").await?;
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    r#"DROP TABLE IF EXISTS "project_settings";"#,
                ))
                .await?;
        }
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct DropClaudeCodeSupport;

impl MigrationName for DropClaudeCodeSupport {
    fn name(&self) -> &str {
        "m20260613_000009_drop_claude_code_support"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for DropClaudeCodeSupport {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let backend = manager.get_database_backend();
        for statement in [
            r#"UPDATE "projects" SET "default_agent_tool" = 'codex' WHERE "default_agent_tool" != 'codex';"#,
            r#"UPDATE "agent_runs" SET "tool_name" = 'codex' WHERE "tool_name" != 'codex';"#,
            r#"UPDATE "automation_triggers" SET "tool_name" = 'codex' WHERE "tool_name" != 'codex';"#,
            r#"DELETE FROM "agent_tools" WHERE "tool_name" != 'codex';"#,
        ] {
            manager
                .get_connection()
                .execute(Statement::from_string(backend, statement))
                .await?;
        }
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

struct RenameProjectRepoPath;

impl MigrationName for RenameProjectRepoPath {
    fn name(&self) -> &str {
        "m20260613_000010_rename_project_repo_path"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RenameProjectRepoPath {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_project_path_column(manager, "repo_path", "path").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        rename_project_path_column(manager, "path", "repo_path").await
    }
}

struct AddProjectPathStatus;

impl MigrationName for AddProjectPathStatus {
    fn name(&self) -> &str {
        "m20260613_000011_add_project_path_status"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectPathStatus {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_project_path_status_columns(manager).await?;
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct AddAutomationRunConfiguration;

impl MigrationName for AddAutomationRunConfiguration {
    fn name(&self) -> &str {
        "m20260613_000012_add_automation_run_configuration"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationRunConfiguration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column_if_missing(manager, "projects", "default_agent_model", "TEXT").await?;
        add_column_if_missing(
            manager,
            "projects",
            "default_agent_reasoning_effort",
            "TEXT",
        )
        .await?;
        add_column_if_missing(
            manager,
            "work_items",
            "automation_claimable",
            "BOOLEAN NOT NULL DEFAULT 1",
        )
        .await?;
        add_column_if_missing(manager, "work_items", "agent_model_override", "TEXT").await?;
        add_column_if_missing(
            manager,
            "work_items",
            "agent_reasoning_effort_override",
            "TEXT",
        )
        .await?;
        add_column_if_missing(manager, "agent_runs", "agent_model", "TEXT").await?;
        add_column_if_missing(manager, "agent_runs", "agent_reasoning_effort", "TEXT").await?;
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        drop_read_view(manager, "work_items_read_view").await?;
        create_read_view(manager, "work_items", "work_items_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        drop_read_view(manager, "work_items_read_view").await?;
        create_read_view(manager, "work_items", "work_items_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct RemoveAutomationTriggerDryRun;

impl MigrationName for RemoveAutomationTriggerDryRun {
    fn name(&self) -> &str {
        "m20260614_000013_remove_automation_trigger_dry_run"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RemoveAutomationTriggerDryRun {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_column_if_present(manager, "automation_triggers", "dry_run").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "dry_run",
            "BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }
}

struct AddAutomationRunTriggerOrigin;

impl MigrationName for AddAutomationRunTriggerOrigin {
    fn name(&self) -> &str {
        "m20260614_000014_add_automation_run_trigger_origin"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationRunTriggerOrigin {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column_if_missing(manager, "agent_runs", "trigger_id", "BIGINT").await?;
        add_column_if_missing(manager, "agent_runs", "trigger_name", "TEXT").await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_agent_runs_trigger_id")
                    .table(AgentRuns::Table)
                    .col(AgentRuns::TriggerId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_agent_runs_trigger_id")
                    .table(AgentRuns::Table)
                    .to_owned(),
            )
            .await?;
        drop_column_if_present(manager, "agent_runs", "trigger_name").await?;
        drop_column_if_present(manager, "agent_runs", "trigger_id").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct AddProjectMemoryEvents;

impl MigrationName for AddProjectMemoryEvents {
    fn name(&self) -> &str {
        "m20260614_000015_add_project_memory_events"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectMemoryEvents {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        add_column_if_missing(manager, "work_item_events", "actor_type", "TEXT").await?;
        add_column_if_missing(manager, "work_item_events", "actor_id", "TEXT").await?;
        add_column_if_missing(manager, "work_item_events", "agent_run_id", "BIGINT").await?;
        add_column_if_missing(manager, "agent_runs", "memory_event_id", "BIGINT").await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_work_item_events_project_type_id")
                    .table(WorkItemEvents::Table)
                    .col(WorkItemEvents::ProjectId)
                    .col(WorkItemEvents::EventType)
                    .col(WorkItemEvents::Id)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        manager
            .drop_index(
                Index::drop()
                    .name("idx_work_item_events_project_type_id")
                    .table(WorkItemEvents::Table)
                    .to_owned(),
            )
            .await?;
        drop_column_if_present(manager, "agent_runs", "memory_event_id").await?;
        drop_column_if_present(manager, "work_item_events", "agent_run_id").await?;
        drop_column_if_present(manager, "work_item_events", "actor_id").await?;
        drop_column_if_present(manager, "work_item_events", "actor_type").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct RemoveWorkItemAutomationClaimable;

impl MigrationName for RemoveWorkItemAutomationClaimable {
    fn name(&self) -> &str {
        "m20260615_000016_remove_work_item_automation_claimable"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RemoveWorkItemAutomationClaimable {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        if column_exists(manager, "work_items", "automation_claimable").await? {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    r#"
                    UPDATE "work_items"
                    SET "state" = 'idea'
                    WHERE "state" = 'open'
                      AND "automation_claimable" = 0;
                    "#,
                ))
                .await?;
        }
        drop_column_if_present(manager, "work_items", "automation_claimable").await?;
        create_read_view(manager, "work_items", "work_items_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        add_column_if_missing(
            manager,
            "work_items",
            "automation_claimable",
            "BOOLEAN NOT NULL DEFAULT 1",
        )
        .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                r#"
                UPDATE "work_items"
                SET
                    "automation_claimable" = 0,
                    "state" = 'open'
                WHERE "state" = 'idea';
                "#,
            ))
            .await?;
        create_read_view(manager, "work_items", "work_items_read_view").await
    }
}

struct AddLabelsAndSwimLanes;

impl MigrationName for AddLabelsAndSwimLanes {
    fn name(&self) -> &str {
        "m20260615_000017_add_labels_and_swim_lanes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddLabelsAndSwimLanes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        create_work_item_labels(manager).await?;
        create_swim_lanes(manager).await?;
        migrate_work_item_state_to_labels(manager).await?;
        drop_index_if_present(manager, "idx_work_items_project_state").await?;
        drop_column_if_present(manager, "work_items", "state").await?;
        create_work_items_read_view(manager).await?;
        create_read_view(manager, "work_item_labels", "work_item_labels_read_view").await?;
        create_read_view(manager, "swim_lanes", "swim_lanes_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "swim_lanes_read_view").await?;
        drop_read_view(manager, "work_item_labels_read_view").await?;
        drop_read_view(manager, "work_items_read_view").await?;
        add_column_if_missing(
            manager,
            "work_items",
            "state",
            "TEXT NOT NULL DEFAULT 'open'",
        )
        .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                r#"
                UPDATE "work_items"
                SET "state" = COALESCE((
                    SELECT "label_value"
                    FROM "work_item_labels"
                    WHERE "work_item_labels"."work_item_id" = "work_items"."id"
                      AND "label_key" = 'state'
                    LIMIT 1
                ), 'open');
                "#,
            ))
            .await?;
        manager
            .drop_table(Table::drop().table(SwimLanes::Table).if_exists().to_owned())
            .await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkItemLabels::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_work_items_project_state")
                    .table(WorkItems::Table)
                    .col(WorkItems::ProjectId)
                    .col(WorkItems::State)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        create_read_view(manager, "work_items", "work_items_read_view").await
    }
}

struct AddAutomationWorkItemSelectors;

impl MigrationName for AddAutomationWorkItemSelectors {
    fn name(&self) -> &str {
        "m20260615_000018_add_automation_work_item_selectors"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationWorkItemSelectors {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        add_column_if_missing(manager, "automation_triggers", "work_item_selector", "TEXT").await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "priority",
            "BIGINT NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "evaluation_count",
            "BIGINT NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "effect",
            "TEXT NOT NULL DEFAULT 'consume_work'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "pending_evaluation_count",
            "BIGINT NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "last_evaluation_queued_at",
            "TEXT",
        )
        .await?;
        seed_default_work_item_automations(manager).await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_column_if_present(manager, "automation_triggers", "last_evaluation_queued_at").await?;
        drop_column_if_present(manager, "automation_triggers", "pending_evaluation_count").await?;
        drop_column_if_present(manager, "automation_triggers", "effect").await?;
        drop_column_if_present(manager, "automation_triggers", "evaluation_count").await?;
        drop_column_if_present(manager, "automation_triggers", "priority").await?;
        drop_column_if_present(manager, "automation_triggers", "work_item_selector").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }
}

struct RenameAutomationActivationAndRequireScheduleTransientName;

impl MigrationName for RenameAutomationActivationAndRequireScheduleTransientName {
    fn name(&self) -> &str {
        "m20260615_000018_rename_automation_activation_require_schedule"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RenameAutomationActivationAndRequireScheduleTransientName {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

struct AddAutomationWorkItemSelectorsTransientName;

impl MigrationName for AddAutomationWorkItemSelectorsTransientName {
    fn name(&self) -> &str {
        "m20260615_000019_add_automation_work_item_selectors"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationWorkItemSelectorsTransientName {
    async fn up(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }

    async fn down(&self, _manager: &SchemaManager) -> Result<(), DbErr> {
        Ok(())
    }
}

struct RenameAutomationActivationAndRequireSchedule;

impl MigrationName for RenameAutomationActivationAndRequireSchedule {
    fn name(&self) -> &str {
        "m20260615_000020_rename_automation_activation_require_schedule"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RenameAutomationActivationAndRequireSchedule {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        rename_column_if_present(manager, "automation_triggers", "trigger_kind", "activation")
            .await?;
        rename_column_if_present(
            manager,
            "automation_triggers",
            "run_count",
            "evaluation_count",
        )
        .await?;
        rename_column_if_present(
            manager,
            "automation_triggers",
            "scheduled_run_count",
            "pending_evaluation_count",
        )
        .await?;
        rename_column_if_present(
            manager,
            "automation_triggers",
            "last_scheduled_run_at",
            "last_evaluation_queued_at",
        )
        .await?;
        rename_column_if_present(
            manager,
            "automation_triggers",
            "last_run_at",
            "last_evaluated_at",
        )
        .await?;
        rename_column_if_present(
            manager,
            "automation_triggers",
            "next_run_at",
            "next_evaluation_at",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "activation",
            "TEXT NOT NULL DEFAULT 'work_item'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "effect",
            "TEXT NOT NULL DEFAULT 'consume_work'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "schedule",
            "TEXT NOT NULL DEFAULT '@every 15s'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "evaluation_count",
            "BIGINT NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "pending_evaluation_count",
            "BIGINT NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "last_evaluation_queued_at",
            "TEXT",
        )
        .await?;
        add_column_if_missing(manager, "automation_triggers", "last_evaluated_at", "TEXT").await?;
        add_column_if_missing(manager, "automation_triggers", "next_evaluation_at", "TEXT").await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                r#"
                UPDATE "automation_triggers"
                SET
                    "activation" = CASE
                        WHEN "activation" = 'manual' THEN 'work_item'
                        ELSE COALESCE(NULLIF("activation", ''), 'work_item')
                    END,
                    "effect" = COALESCE(NULLIF("effect", ''), 'consume_work'),
                    "schedule" = COALESCE(NULLIF("schedule", ''), '@every 15s'),
                    "evaluation_count" = COALESCE("evaluation_count", 0),
                    "pending_evaluation_count" = COALESCE("pending_evaluation_count", 0);
                "#,
            ))
            .await?;
        drop_index_if_present(manager, "idx_automation_triggers_project_kind").await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_automation_triggers_project_activation")
                    .table(AutomationTriggers::Table)
                    .col(AutomationTriggers::ProjectId)
                    .col(AutomationTriggers::Activation)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_index_if_present(manager, "idx_automation_triggers_project_activation").await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_automation_triggers_project_kind")
                    .table(AutomationTriggers::Table)
                    .col(AutomationTriggers::ProjectId)
                    .col(AutomationTriggers::Activation)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        drop_column_if_present(manager, "automation_triggers", "last_evaluation_queued_at").await?;
        drop_column_if_present(manager, "automation_triggers", "pending_evaluation_count").await?;
        drop_column_if_present(manager, "automation_triggers", "effect").await?;
        rename_column_if_present(manager, "automation_triggers", "activation", "trigger_kind")
            .await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }
}

struct AddWorkItemStateLabelReadView;

impl MigrationName for AddWorkItemStateLabelReadView {
    fn name(&self) -> &str {
        "m20260615_000021_add_work_item_state_label_read_view"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddWorkItemStateLabelReadView {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_work_items_read_view(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        create_read_view(manager, "work_items", "work_items_read_view").await
    }
}

struct AddSwimLaneCreateItemFlag;

impl MigrationName for AddSwimLaneCreateItemFlag {
    fn name(&self) -> &str {
        "m20260616_000022_add_swim_lane_create_item_flag"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddSwimLaneCreateItemFlag {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "swim_lanes_read_view").await?;
        add_column_if_missing(
            manager,
            "swim_lanes",
            "can_create_items",
            "BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                r#"
                UPDATE "swim_lanes"
                SET "can_create_items" = 1
                WHERE "identifier" IN ('idea', 'open');
                "#
                .to_owned(),
            ))
            .await?;
        create_read_view(manager, "swim_lanes", "swim_lanes_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "swim_lanes_read_view").await?;
        drop_column_if_present(manager, "swim_lanes", "can_create_items").await?;
        create_read_view(manager, "swim_lanes", "swim_lanes_read_view").await
    }
}

struct AddProjectAgentExtraWritableRoots;

impl MigrationName for AddProjectAgentExtraWritableRoots {
    fn name(&self) -> &str {
        "m20260616_000023_add_project_agent_extra_writable_roots"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectAgentExtraWritableRoots {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        add_column_if_missing(
            manager,
            "projects",
            "agent_extra_writable_roots",
            "TEXT NOT NULL DEFAULT ''",
        )
        .await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_column_if_present(manager, "projects", "agent_extra_writable_roots").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct AddProjectAgentSandboxMode;

impl MigrationName for AddProjectAgentSandboxMode {
    fn name(&self) -> &str {
        "m20260616_000024_add_project_agent_sandbox_mode"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectAgentSandboxMode {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        add_column_if_missing(
            manager,
            "projects",
            "agent_sandbox_mode",
            "TEXT NOT NULL DEFAULT 'workspace_write'",
        )
        .await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_column_if_present(manager, "projects", "agent_sandbox_mode").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct DecoupleStatesAndSwimLanes;

impl MigrationName for DecoupleStatesAndSwimLanes {
    fn name(&self) -> &str {
        "m20260617_000025_decouple_states_and_swim_lanes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for DecoupleStatesAndSwimLanes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "swim_lanes_read_view").await?;
        create_work_item_states(manager).await?;
        add_column_if_missing(
            manager,
            "swim_lanes",
            "filter",
            "TEXT NOT NULL DEFAULT '{\"All\":[]}'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "swim_lanes",
            "item_order",
            "TEXT NOT NULL DEFAULT 'updated_desc'",
        )
        .await?;
        seed_work_item_states_from_existing_data(manager).await?;
        seed_swim_lane_filters_from_identifiers(manager).await?;
        create_read_view(manager, "work_item_states", "work_item_states_read_view").await?;
        create_read_view(manager, "swim_lanes", "swim_lanes_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_item_states_read_view").await?;
        drop_read_view(manager, "swim_lanes_read_view").await?;
        drop_column_if_present(manager, "swim_lanes", "item_order").await?;
        drop_column_if_present(manager, "swim_lanes", "filter").await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkItemStates::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        create_read_view(manager, "swim_lanes", "swim_lanes_read_view").await
    }
}

struct AddProjectCommitPolicy;

impl MigrationName for AddProjectCommitPolicy {
    fn name(&self) -> &str {
        "m20260617_000026_add_project_commit_policy"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectCommitPolicy {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        add_project_commit_policy_columns(manager).await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_column_if_present(manager, "projects", "revert_strategy").await?;
        drop_column_if_present(manager, "projects", "commit_standard").await?;
        drop_column_if_present(manager, "projects", "auto_commit").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct AddProjectAgentGitCommandPolicy;

impl MigrationName for AddProjectAgentGitCommandPolicy {
    fn name(&self) -> &str {
        "m20260617_000027_add_project_agent_git_command_policy"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddProjectAgentGitCommandPolicy {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        let default_git_policy = format!(
            "TEXT NOT NULL DEFAULT '{}'",
            DEFAULT_AGENT_GIT_COMMAND_POLICY
        );
        add_column_if_missing(
            manager,
            "projects",
            "agent_git_command_policy",
            &default_git_policy,
        )
        .await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_column_if_present(manager, "projects", "agent_git_command_policy").await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct AddAutomationRunCommitOutcomes;

impl MigrationName for AddAutomationRunCommitOutcomes {
    fn name(&self) -> &str {
        "m20260617_000028_add_automation_run_commit_outcomes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationRunCommitOutcomes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "commit_required",
            "BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "commit_outcome",
            "TEXT NOT NULL DEFAULT 'not_evaluated'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "commit_shas",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_column_if_present(manager, "agent_runs", "commit_shas").await?;
        drop_column_if_present(manager, "agent_runs", "commit_outcome").await?;
        drop_column_if_present(manager, "agent_runs", "commit_required").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct AddAutomationRunTokenUsage;

impl MigrationName for AddAutomationRunTokenUsage {
    fn name(&self) -> &str {
        "m20260617_000029_add_automation_run_token_usage"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationRunTokenUsage {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        add_column_if_missing(manager, "agent_runs", "input_tokens", "BIGINT").await?;
        add_column_if_missing(manager, "agent_runs", "cached_input_tokens", "BIGINT").await?;
        add_column_if_missing(manager, "agent_runs", "output_tokens", "BIGINT").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_column_if_present(manager, "agent_runs", "output_tokens").await?;
        drop_column_if_present(manager, "agent_runs", "cached_input_tokens").await?;
        drop_column_if_present(manager, "agent_runs", "input_tokens").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct AddRefinerVerifierAutomations;

impl MigrationName for AddRefinerVerifierAutomations {
    fn name(&self) -> &str {
        "m20260618_000030_add_refiner_verifier_automations"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddRefinerVerifierAutomations {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        update_default_open_work_selector(manager, DEFAULT_WORK_ITEM_SELECTOR).await?;
        seed_label_routed_automation(
            manager,
            "Refine needs-refinement work",
            "refine",
            DEFAULT_REFINEMENT_PROMPT,
            DEFAULT_REFINEMENT_SELECTOR,
            20,
        )
        .await?;
        seed_label_routed_automation(
            manager,
            "Verify needs-verification work",
            "refine",
            DEFAULT_VERIFICATION_PROMPT,
            DEFAULT_VERIFICATION_SELECTOR,
            10,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        delete_label_routed_automation(
            manager,
            "Verify needs-verification work",
            DEFAULT_VERIFICATION_PROMPT,
            DEFAULT_VERIFICATION_SELECTOR,
        )
        .await?;
        delete_label_routed_automation(
            manager,
            "Verify needs-verification work",
            PRE_FEEDBACK_VERIFICATION_PROMPT,
            DEFAULT_VERIFICATION_SELECTOR,
        )
        .await?;
        delete_label_routed_automation(
            manager,
            "Refine needs-refinement work",
            DEFAULT_REFINEMENT_PROMPT,
            DEFAULT_REFINEMENT_SELECTOR,
        )
        .await?;
        delete_label_routed_automation(
            manager,
            "Refine needs-refinement work",
            PRE_FEEDBACK_REFINEMENT_PROMPT,
            DEFAULT_REFINEMENT_SELECTOR,
        )
        .await?;
        update_automation_selector_if_unchanged(
            manager,
            "Claim open work",
            PRE_FEEDBACK_DEFAULT_WORK_ITEM_SELECTOR,
            OLD_DEFAULT_WORK_ITEM_SELECTOR,
        )
        .await?;
        update_default_open_work_selector(manager, OLD_DEFAULT_WORK_ITEM_SELECTOR).await
    }
}

struct RemoveAutomationModes;

impl MigrationName for RemoveAutomationModes {
    fn name(&self) -> &str {
        "m20260618_000031_remove_automation_modes"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RemoveAutomationModes {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_column_if_present(manager, "agent_runs", "mode").await?;
        drop_column_if_present(manager, "automation_triggers", "mode").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_read_view(manager, "automation_triggers_read_view").await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "mode",
            "TEXT NOT NULL DEFAULT 'execute'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "mode",
            "TEXT NOT NULL DEFAULT 'execute'",
        )
        .await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }
}

struct RemoveRefinementConcurrencySetting;

impl MigrationName for RemoveRefinementConcurrencySetting {
    fn name(&self) -> &str {
        "m20260618_000032_remove_refinement_concurrency_setting"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for RemoveRefinementConcurrencySetting {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_column_if_present(manager, "projects", REMOVED_REFINEMENT_CONCURRENCY_COLUMN).await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        add_column_if_missing(
            manager,
            "projects",
            REMOVED_REFINEMENT_CONCURRENCY_COLUMN,
            "BOOLEAN NOT NULL DEFAULT 0",
        )
        .await?;
        create_read_view(manager, "projects", "projects_read_view").await
    }
}

struct AddFeedbackRequestWorkflow;

impl MigrationName for AddFeedbackRequestWorkflow {
    fn name(&self) -> &str {
        "m20260618_000033_add_feedback_request_workflow"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddFeedbackRequestWorkflow {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        update_automation_selector_if_unchanged(
            manager,
            "Claim open work",
            PRE_FEEDBACK_DEFAULT_WORK_ITEM_SELECTOR,
            DEFAULT_WORK_ITEM_SELECTOR,
        )
        .await?;
        update_automation_prompt_if_unchanged(
            manager,
            "Refine needs-refinement work",
            PRE_FEEDBACK_REFINEMENT_PROMPT,
            DEFAULT_REFINEMENT_PROMPT,
        )
        .await?;
        update_automation_prompt_if_unchanged(
            manager,
            "Verify needs-verification work",
            PRE_FEEDBACK_VERIFICATION_PROMPT,
            DEFAULT_VERIFICATION_PROMPT,
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        update_automation_prompt_if_unchanged(
            manager,
            "Verify needs-verification work",
            DEFAULT_VERIFICATION_PROMPT,
            PRE_FEEDBACK_VERIFICATION_PROMPT,
        )
        .await?;
        update_automation_prompt_if_unchanged(
            manager,
            "Refine needs-refinement work",
            DEFAULT_REFINEMENT_PROMPT,
            PRE_FEEDBACK_REFINEMENT_PROMPT,
        )
        .await?;
        update_automation_selector_if_unchanged(
            manager,
            "Claim open work",
            DEFAULT_WORK_ITEM_SELECTOR,
            PRE_FEEDBACK_DEFAULT_WORK_ITEM_SELECTOR,
        )
        .await
    }
}

struct AddAutomationRunMutability;

impl MigrationName for AddAutomationRunMutability {
    fn name(&self) -> &str {
        "m20260618_000034_add_automation_run_mutability"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationRunMutability {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_read_view(manager, "automation_triggers_read_view").await?;
        add_column_if_missing(
            manager,
            "projects",
            "max_read_only_agents",
            "BIGINT NOT NULL DEFAULT 2",
        )
        .await?;
        add_column_if_missing(
            manager,
            "agent_runs",
            "mutability",
            "TEXT NOT NULL DEFAULT 'mutating'",
        )
        .await?;
        add_column_if_missing(
            manager,
            "automation_triggers",
            "mutability",
            "TEXT NOT NULL DEFAULT 'mutating'",
        )
        .await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "projects_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_column_if_present(manager, "automation_triggers", "mutability").await?;
        drop_column_if_present(manager, "agent_runs", "mutability").await?;
        drop_column_if_present(manager, "projects", "max_read_only_agents").await?;
        create_read_view(manager, "projects", "projects_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }
}

struct AddWorkItemRelationships;

impl MigrationName for AddWorkItemRelationships {
    fn name(&self) -> &str {
        "m20260618_000035_add_work_item_relationships"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddWorkItemRelationships {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        create_work_item_relationships(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(WorkItemRelationships::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await
    }
}

struct AddAutomationPersonalities;

impl MigrationName for AddAutomationPersonalities {
    fn name(&self) -> &str {
        "m20260619_000036_add_automation_personalities"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationPersonalities {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        create_personalities(manager).await?;
        add_column_if_missing(manager, "automation_triggers", "personality_id", "BIGINT").await?;
        seed_default_personalities(manager).await?;
        backfill_automation_personalities(manager).await?;
        create_read_view(manager, "personalities", "personalities_read_view").await?;
        create_automation_triggers_read_view(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_read_view(manager, "personalities_read_view").await?;
        drop_column_if_present(manager, "automation_triggers", "personality_id").await?;
        manager
            .drop_table(
                Table::drop()
                    .table(Personalities::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        create_read_view(
            manager,
            "automation_triggers",
            "automation_triggers_read_view",
        )
        .await
    }
}

struct SeparateAutomationRunInputs;

impl MigrationName for SeparateAutomationRunInputs {
    fn name(&self) -> &str {
        "m20260710_000037_separate_automation_run_inputs"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for SeparateAutomationRunInputs {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        add_column_if_missing(manager, "agent_runs", "developer_instructions_path", "TEXT").await?;
        add_column_if_missing(manager, "agent_runs", "user_prompt_path", "TEXT").await?;
        if column_exists(manager, "agent_runs", "prompt_path").await? {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    r#"
                    UPDATE "agent_runs"
                    SET "developer_instructions_path" = "prompt_path",
                        "user_prompt_path" = "prompt_path"
                    WHERE "prompt_path" IS NOT NULL;
                    "#
                    .to_owned(),
                ))
                .await?;
        }
        drop_column_if_present(manager, "agent_runs", "prompt_path").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "agent_runs_read_view").await?;
        add_column_if_missing(manager, "agent_runs", "prompt_path", "TEXT").await?;
        restore_legacy_automation_prompt_paths(manager).await?;
        drop_column_if_present(manager, "agent_runs", "user_prompt_path").await?;
        drop_column_if_present(manager, "agent_runs", "developer_instructions_path").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

struct AddAutomationWorkflowSupport;

impl MigrationName for AddAutomationWorkflowSupport {
    fn name(&self) -> &str {
        "m20260713_000038_add_automation_workflow_support"
    }
}

struct AddWorkItemGroups;

impl MigrationName for AddWorkItemGroups {
    fn name(&self) -> &str {
        "m20260714_000039_add_work_item_groups"
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddWorkItemGroups {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        manager
            .create_table(
                Table::create()
                    .table(WorkItemGroups::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(WorkItemGroups::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(WorkItemGroups::ProjectId)
                            .big_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(WorkItemGroups::GroupKey).string().not_null())
                    .col(ColumnDef::new(WorkItemGroups::Name).string().not_null())
                    .col(ColumnDef::new(WorkItemGroups::ActorId).string().null())
                    .col(
                        ColumnDef::new(WorkItemGroups::AgentRunId)
                            .big_integer()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(WorkItemGroups::CreatedAt)
                            .string()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .col(
                        ColumnDef::new(WorkItemGroups::UpdatedAt)
                            .string()
                            .not_null()
                            .default(Expr::current_timestamp()),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .from(WorkItemGroups::Table, WorkItemGroups::ProjectId)
                            .to(Projects::Table, Projects::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_work_item_groups_project_key")
                    .table(WorkItemGroups::Table)
                    .col(WorkItemGroups::ProjectId)
                    .col(WorkItemGroups::GroupKey)
                    .unique()
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        add_column_if_missing(
            manager,
            "work_items",
            "work_group_id",
            "BIGINT REFERENCES work_item_groups(id) ON DELETE SET NULL",
        )
        .await?;
        manager
            .create_index(
                Index::create()
                    .name("idx_work_items_project_group")
                    .table(WorkItems::Table)
                    .col(WorkItems::ProjectId)
                    .col(WorkItems::WorkGroupId)
                    .if_not_exists()
                    .to_owned(),
            )
            .await?;
        create_work_items_read_view(manager).await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "work_items_read_view").await?;
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                "DROP INDEX IF EXISTS idx_work_items_project_group;".to_owned(),
            ))
            .await?;
        drop_column_if_present(manager, "work_items", "work_group_id").await?;
        manager
            .drop_table(
                Table::drop()
                    .table(WorkItemGroups::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;
        create_work_items_read_view(manager).await
    }
}

#[async_trait::async_trait]
impl MigrationTrait for AddAutomationWorkflowSupport {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_read_view(manager, "personalities_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;

        add_automation_workflow_columns(manager).await?;
        create_automation_workflow_tables(manager).await?;
        create_automation_workflow_indexes(manager).await?;
        backfill_automation_revisions(manager).await?;
        backfill_personality_revisions(manager).await?;
        backfill_historical_work_item_origins(manager).await?;

        create_automation_triggers_read_view(manager).await?;
        create_read_view(manager, "personalities", "personalities_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        drop_read_view(manager, "automation_triggers_read_view").await?;
        drop_read_view(manager, "personalities_read_view").await?;
        drop_read_view(manager, "agent_runs_read_view").await?;

        for index in [
            "idx_automation_trigger_managed_key",
            "idx_personality_managed_key",
            "idx_trigger_revisions_project_trigger",
            "idx_personality_revisions_project_personality",
            "idx_automation_evaluations_project_trigger",
            "idx_work_item_origins_run",
            "idx_work_item_origins_trigger",
            "idx_bundle_applies_project_key",
            "idx_work_items_project_updated_search",
            "idx_relationships_project_kind",
        ] {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    format!(r#"DROP INDEX IF EXISTS "{index}";"#),
                ))
                .await?;
        }

        for table in [
            "automation_bundle_applies",
            "work_item_origins",
            "automation_evaluations",
            "personality_revisions",
            "automation_trigger_revisions",
        ] {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    format!(r#"DROP TABLE IF EXISTS "{table}";"#),
                ))
                .await?;
        }

        for column in [
            "semantic_postcondition_failures",
            "semantic_postcondition_status",
            "effective_timeout_seconds",
            "effective_concurrency_group",
            "effective_input_sha256",
            "system_prompt_event_id",
            "personality_revision_id",
            "trigger_revision_id",
        ] {
            drop_column_if_present(manager, "agent_runs", column).await?;
        }
        for column in [
            "managed_object_key",
            "managed_bundle_key",
            "current_revision_id",
        ] {
            drop_column_if_present(manager, "personalities", column).await?;
        }
        for column in [
            "managed_object_key",
            "managed_bundle_key",
            "current_revision_id",
            "concurrency_group",
            "max_concurrent_runs",
            "timeout_seconds",
            "reasoning_effort_override",
            "model_override",
            "postconditions_json",
            "produced_work_spec_json",
            "exclusive",
        ] {
            drop_column_if_present(manager, "automation_triggers", column).await?;
        }

        create_automation_triggers_read_view(manager).await?;
        create_read_view(manager, "personalities", "personalities_read_view").await?;
        create_read_view(manager, "agent_runs", "agent_runs_read_view").await
    }
}

async fn add_automation_workflow_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    add_column_if_missing(
        manager,
        "automation_triggers",
        "exclusive",
        "BOOLEAN NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        manager,
        "automation_triggers",
        "produced_work_spec_json",
        "TEXT",
    )
    .await?;
    add_column_if_missing(
        manager,
        "automation_triggers",
        "postconditions_json",
        "TEXT",
    )
    .await?;
    add_column_if_missing(manager, "automation_triggers", "model_override", "TEXT").await?;
    add_column_if_missing(
        manager,
        "automation_triggers",
        "reasoning_effort_override",
        "TEXT",
    )
    .await?;
    add_column_if_missing(manager, "automation_triggers", "timeout_seconds", "BIGINT").await?;
    add_column_if_missing(
        manager,
        "automation_triggers",
        "max_concurrent_runs",
        "BIGINT",
    )
    .await?;
    add_column_if_missing(manager, "automation_triggers", "concurrency_group", "TEXT").await?;
    add_column_if_missing(
        manager,
        "automation_triggers",
        "current_revision_id",
        "BIGINT",
    )
    .await?;
    add_column_if_missing(manager, "automation_triggers", "managed_bundle_key", "TEXT").await?;
    add_column_if_missing(manager, "automation_triggers", "managed_object_key", "TEXT").await?;

    add_column_if_missing(manager, "personalities", "current_revision_id", "BIGINT").await?;
    add_column_if_missing(manager, "personalities", "managed_bundle_key", "TEXT").await?;
    add_column_if_missing(manager, "personalities", "managed_object_key", "TEXT").await?;

    add_column_if_missing(manager, "agent_runs", "trigger_revision_id", "BIGINT").await?;
    add_column_if_missing(manager, "agent_runs", "personality_revision_id", "BIGINT").await?;
    add_column_if_missing(manager, "agent_runs", "system_prompt_event_id", "BIGINT").await?;
    add_column_if_missing(manager, "agent_runs", "effective_input_sha256", "TEXT").await?;
    add_column_if_missing(manager, "agent_runs", "effective_timeout_seconds", "BIGINT").await?;
    add_column_if_missing(manager, "agent_runs", "effective_concurrency_group", "TEXT").await?;
    add_column_if_missing(
        manager,
        "agent_runs",
        "semantic_postcondition_status",
        "TEXT NOT NULL DEFAULT 'not_configured'",
    )
    .await?;
    add_column_if_missing(
        manager,
        "agent_runs",
        "semantic_postcondition_failures",
        "TEXT NOT NULL DEFAULT '[]'",
    )
    .await
}

async fn execute_migration_sql(manager: &SchemaManager<'_>, sql: &str) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            sql.to_owned(),
        ))
        .await
        .map(|_| ())
}

async fn create_automation_workflow_tables(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    execute_migration_sql(
        manager,
        r#"
        CREATE TABLE IF NOT EXISTS "automation_trigger_revisions" (
            "id" INTEGER PRIMARY KEY AUTOINCREMENT,
            "trigger_id" BIGINT,
            "project_id" BIGINT NOT NULL,
            "trigger_name" TEXT NOT NULL,
            "revision_number" BIGINT NOT NULL,
            "configuration_json" TEXT NOT NULL,
            "sha256" TEXT NOT NULL,
            "change_operation" TEXT NOT NULL,
            "actor_type" TEXT,
            "actor_id" TEXT,
            "created_at" TEXT NOT NULL,
            FOREIGN KEY ("trigger_id") REFERENCES "automation_triggers" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON DELETE CASCADE,
            UNIQUE ("trigger_id", "revision_number")
        );
        "#,
    )
    .await?;
    execute_migration_sql(
        manager,
        r#"
        CREATE TABLE IF NOT EXISTS "personality_revisions" (
            "id" INTEGER PRIMARY KEY AUTOINCREMENT,
            "personality_id" BIGINT,
            "project_id" BIGINT NOT NULL,
            "personality_name" TEXT NOT NULL,
            "revision_number" BIGINT NOT NULL,
            "personality_description" TEXT NOT NULL,
            "sha256" TEXT NOT NULL,
            "change_operation" TEXT NOT NULL,
            "actor_type" TEXT,
            "actor_id" TEXT,
            "created_at" TEXT NOT NULL,
            FOREIGN KEY ("personality_id") REFERENCES "personalities" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON DELETE CASCADE,
            UNIQUE ("personality_id", "revision_number")
        );
        "#,
    )
    .await?;
    execute_migration_sql(
        manager,
        r#"
        CREATE TABLE IF NOT EXISTS "automation_evaluations" (
            "id" INTEGER PRIMARY KEY AUTOINCREMENT,
            "project_id" BIGINT NOT NULL,
            "trigger_id" BIGINT,
            "trigger_revision_id" BIGINT,
            "trigger_name" TEXT NOT NULL,
            "activation_cause" TEXT NOT NULL,
            "outcome" TEXT NOT NULL,
            "work_item_id" BIGINT,
            "run_id" BIGINT,
            "error" TEXT,
            "created_at" TEXT NOT NULL,
            "completed_at" TEXT,
            FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON DELETE CASCADE,
            FOREIGN KEY ("trigger_id") REFERENCES "automation_triggers" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("trigger_revision_id") REFERENCES "automation_trigger_revisions" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("work_item_id") REFERENCES "work_items" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("run_id") REFERENCES "agent_runs" ("id") ON DELETE SET NULL
        );
        "#,
    )
    .await?;
    execute_migration_sql(
        manager,
        r#"
        CREATE TABLE IF NOT EXISTS "work_item_origins" (
            "work_item_id" BIGINT PRIMARY KEY,
            "project_id" BIGINT NOT NULL,
            "origin_kind" TEXT NOT NULL,
            "actor_id" TEXT,
            "agent_run_id" BIGINT,
            "producing_evaluation_id" BIGINT,
            "trigger_id" BIGINT,
            "trigger_revision_id" BIGINT,
            "trigger_name" TEXT,
            "bundle_key" TEXT,
            "deduplication_key" TEXT,
            "created_at" TEXT NOT NULL,
            FOREIGN KEY ("work_item_id") REFERENCES "work_items" ("id") ON DELETE CASCADE,
            FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON DELETE CASCADE,
            FOREIGN KEY ("agent_run_id") REFERENCES "agent_runs" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("producing_evaluation_id") REFERENCES "automation_evaluations" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("trigger_id") REFERENCES "automation_triggers" ("id") ON DELETE SET NULL,
            FOREIGN KEY ("trigger_revision_id") REFERENCES "automation_trigger_revisions" ("id") ON DELETE SET NULL
        );
        "#,
    )
    .await?;
    execute_migration_sql(
        manager,
        r#"
        CREATE TABLE IF NOT EXISTS "automation_bundle_applies" (
            "id" INTEGER PRIMARY KEY AUTOINCREMENT,
            "project_id" BIGINT NOT NULL,
            "bundle_key" TEXT NOT NULL,
            "display_name" TEXT NOT NULL,
            "manifest_hash" TEXT NOT NULL,
            "applied_diff_json" TEXT NOT NULL,
            "actor_type" TEXT,
            "actor_id" TEXT,
            "status" TEXT NOT NULL,
            "created_at" TEXT NOT NULL,
            FOREIGN KEY ("project_id") REFERENCES "projects" ("id") ON DELETE CASCADE
        );
        "#,
    )
    .await
}

async fn create_automation_workflow_indexes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    for sql in [
        r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_automation_trigger_managed_key" ON "automation_triggers" ("project_id", "managed_bundle_key", "managed_object_key") WHERE "managed_bundle_key" IS NOT NULL;"#,
        r#"CREATE UNIQUE INDEX IF NOT EXISTS "idx_personality_managed_key" ON "personalities" ("project_id", "managed_bundle_key", "managed_object_key") WHERE "managed_bundle_key" IS NOT NULL;"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_trigger_revisions_project_trigger" ON "automation_trigger_revisions" ("project_id", "trigger_id", "revision_number");"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_personality_revisions_project_personality" ON "personality_revisions" ("project_id", "personality_id", "revision_number");"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_automation_evaluations_project_trigger" ON "automation_evaluations" ("project_id", "trigger_id", "created_at");"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_work_item_origins_run" ON "work_item_origins" ("project_id", "agent_run_id");"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_work_item_origins_trigger" ON "work_item_origins" ("project_id", "trigger_id", "deduplication_key");"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_bundle_applies_project_key" ON "automation_bundle_applies" ("project_id", "bundle_key", "id");"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_work_items_project_updated_search" ON "work_items" ("project_id", "updated_at" DESC, "id" DESC);"#,
        r#"CREATE INDEX IF NOT EXISTS "idx_relationships_project_kind" ON "work_item_relationships" ("project_id", "kind");"#,
    ] {
        execute_migration_sql(manager, sql).await?;
    }
    Ok(())
}

async fn backfill_automation_revisions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let rows = manager
        .get_connection()
        .query_all(Statement::from_string(
            manager.get_database_backend(),
            r#"
            SELECT "id", "project_id", "name",
                json_object(
                    'name', "name", 'enabled', "enabled", 'activation', "activation",
                    'effect', "effect", 'schedule', "schedule", 'tool_name', "tool_name",
                    'mutability', "mutability", 'personality_id', "personality_id",
                    'prompt', "prompt", 'work_item_selector', "work_item_selector",
                    'priority', "priority", 'exclusive', "exclusive",
                    'produced_work_spec_json', "produced_work_spec_json",
                    'postconditions_json', "postconditions_json", 'model_override', "model_override",
                    'reasoning_effort_override', "reasoning_effort_override",
                    'timeout_seconds', "timeout_seconds", 'max_concurrent_runs', "max_concurrent_runs",
                    'concurrency_group', "concurrency_group", 'managed_bundle_key', "managed_bundle_key",
                    'managed_object_key', "managed_object_key"
                ) AS "configuration_json"
            FROM "automation_triggers"
            WHERE "current_revision_id" IS NULL;
            "#
            .to_owned(),
        ))
        .await?;
    for row in rows {
        let id = row.try_get::<i64>("", "id")?;
        let project_id = row.try_get::<i64>("", "project_id")?;
        let name = row.try_get::<String>("", "name")?;
        let configuration = row.try_get::<String>("", "configuration_json")?;
        let hash = format!("{:x}", Sha256::digest(configuration.as_bytes()));
        let inserted = manager
            .get_connection()
            .execute(Statement::from_sql_and_values(
                manager.get_database_backend(),
                r#"
                INSERT INTO "automation_trigger_revisions"
                    ("trigger_id", "project_id", "trigger_name", "revision_number", "configuration_json", "sha256", "change_operation", "created_at")
                VALUES (?1, ?2, ?3, 1, ?4, ?5, 'migration', CURRENT_TIMESTAMP);
                "#,
                vec![id.into(), project_id.into(), name.into(), configuration.into(), hash.into()],
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_sql_and_values(
                manager.get_database_backend(),
                r#"UPDATE "automation_triggers" SET "current_revision_id" = ?1 WHERE "id" = ?2;"#,
                vec![(inserted.last_insert_id() as i64).into(), id.into()],
            ))
            .await?;
    }
    Ok(())
}

async fn backfill_personality_revisions(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let rows = manager
        .get_connection()
        .query_all(Statement::from_string(
            manager.get_database_backend(),
            r#"SELECT "id", "project_id", "name", "personality_description" FROM "personalities" WHERE "current_revision_id" IS NULL;"#.to_owned(),
        ))
        .await?;
    for row in rows {
        let id = row.try_get::<i64>("", "id")?;
        let project_id = row.try_get::<i64>("", "project_id")?;
        let name = row.try_get::<String>("", "name")?;
        let description = row.try_get::<String>("", "personality_description")?;
        let canonical =
            serde_json::to_string(&(name.as_str(), description.as_str())).map_err(|err| {
                DbErr::Migration(format!("failed to encode personality revision: {err}"))
            })?;
        let hash = format!("{:x}", Sha256::digest(canonical.as_bytes()));
        let inserted = manager
            .get_connection()
            .execute(Statement::from_sql_and_values(
                manager.get_database_backend(),
                r#"
                INSERT INTO "personality_revisions"
                    ("personality_id", "project_id", "personality_name", "revision_number", "personality_description", "sha256", "change_operation", "created_at")
                VALUES (?1, ?2, ?3, 1, ?4, ?5, 'migration', CURRENT_TIMESTAMP);
                "#,
                vec![id.into(), project_id.into(), name.into(), description.into(), hash.into()],
            ))
            .await?;
        manager
            .get_connection()
            .execute(Statement::from_sql_and_values(
                manager.get_database_backend(),
                r#"UPDATE "personalities" SET "current_revision_id" = ?1 WHERE "id" = ?2;"#,
                vec![(inserted.last_insert_id() as i64).into(), id.into()],
            ))
            .await?;
    }
    Ok(())
}

async fn backfill_historical_work_item_origins(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    execute_migration_sql(
        manager,
        r#"
        INSERT OR IGNORE INTO "work_item_origins"
            ("work_item_id", "project_id", "origin_kind", "created_at")
        SELECT "id", "project_id", 'historical', COALESCE("created_at", CURRENT_TIMESTAMP)
        FROM "work_items";
        "#,
    )
    .await
}

async fn restore_legacy_automation_prompt_paths(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let runs = manager
        .get_connection()
        .query_all(Statement::from_string(
            manager.get_database_backend(),
            r#"
            SELECT "id", "developer_instructions_path", "user_prompt_path"
            FROM "agent_runs";
            "#
            .to_owned(),
        ))
        .await?;

    for run in runs {
        let run_id = run.try_get::<i64>("", "id")?;
        let developer_instructions_path =
            run.try_get::<Option<String>>("", "developer_instructions_path")?;
        let user_prompt_path = run.try_get::<Option<String>>("", "user_prompt_path")?;
        let Some(prompt_path) = legacy_automation_prompt_path(
            run_id,
            developer_instructions_path.as_deref(),
            user_prompt_path.as_deref(),
        )?
        else {
            continue;
        };

        manager
            .get_connection()
            .execute(Statement::from_sql_and_values(
                manager.get_database_backend(),
                r#"
                UPDATE "agent_runs"
                SET "prompt_path" = ?1
                WHERE "id" = ?2;
                "#,
                vec![prompt_path.into(), run_id.into()],
            ))
            .await?;
    }

    Ok(())
}

fn legacy_automation_prompt_path(
    run_id: i64,
    developer_instructions_path: Option<&str>,
    user_prompt_path: Option<&str>,
) -> Result<Option<String>, DbErr> {
    match (developer_instructions_path, user_prompt_path) {
        (None, None) => Ok(None),
        (Some(path), None) | (None, Some(path)) => Ok(Some(path.to_owned())),
        (Some(developer_path), Some(user_path)) if developer_path == user_path => {
            Ok(Some(developer_path.to_owned()))
        }
        (Some(developer_path), Some(user_path)) => {
            let combined_path = legacy_combined_prompt_path(run_id, developer_path);
            let developer_instructions =
                read_prompt_artifact(run_id, "developer instructions", Path::new(developer_path))?;
            let user_prompt = read_prompt_artifact(run_id, "user prompt", Path::new(user_path))?;
            let combined_prompt = legacy_combined_prompt(&developer_instructions, &user_prompt);
            fs::write(&combined_path, combined_prompt).map_err(|err| {
                DbErr::Migration(format!(
                    "failed to write combined prompt artifact for automation run {run_id} to {}: {err}",
                    combined_path.display()
                ))
            })?;
            Ok(Some(combined_path.to_string_lossy().into_owned()))
        }
    }
}

fn legacy_combined_prompt(developer_instructions: &str, user_prompt: &str) -> String {
    let mut prompt = String::from("# Dispatch Automation Prompt\n\n");
    push_legacy_prompt_section(
        &mut prompt,
        "Developer Instructions",
        developer_instructions,
    );
    prompt.push('\n');
    push_legacy_prompt_section(&mut prompt, "User Prompt", user_prompt);
    prompt
}

fn push_legacy_prompt_section(prompt: &mut String, title: &str, body: &str) {
    prompt.push_str("## ");
    prompt.push_str(title);
    prompt.push_str("\n\n");
    prompt.push_str(body);
    if !body.ends_with('\n') {
        prompt.push('\n');
    }
}

fn legacy_combined_prompt_path(run_id: i64, developer_instructions_path: &str) -> PathBuf {
    Path::new(developer_instructions_path)
        .parent()
        .unwrap_or_else(|| Path::new(""))
        .join(format!("run-{run_id}.prompt.md"))
}

fn read_prompt_artifact(run_id: i64, role: &str, path: &Path) -> Result<String, DbErr> {
    fs::read_to_string(path).map_err(|err| {
        DbErr::Migration(format!(
            "failed to read {role} artifact for automation run {run_id} from {}: {err}",
            path.display()
        ))
    })
}

async fn create_personalities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(Personalities::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(Personalities::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(Personalities::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(Personalities::Name).string().not_null())
                .col(
                    ColumnDef::new(Personalities::PersonalityDescription)
                        .text()
                        .not_null()
                        .default(""),
                )
                .col(
                    ColumnDef::new(Personalities::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(Personalities::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_personalities_project_id")
                        .from(Personalities::Table, Personalities::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_personalities_project_name_unique")
                .table(Personalities::Table)
                .col(Personalities::ProjectId)
                .col(Personalities::Name)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn seed_default_personalities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            r#"
            INSERT INTO "personalities"
                (
                    "project_id",
                    "name",
                    "personality_description",
                    "created_at",
                    "updated_at"
                )
            SELECT
                "projects"."id",
                'Default',
                '',
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            FROM "projects"
            WHERE NOT EXISTS (
                SELECT 1
                FROM "personalities"
                WHERE "personalities"."project_id" = "projects"."id"
                  AND "personalities"."name" = 'Default'
            );
            "#,
        ))
        .await
        .map(|_| ())
}

async fn backfill_automation_personalities(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            r#"
            UPDATE "automation_triggers"
            SET "personality_id" = (
                    SELECT "personalities"."id"
                    FROM "personalities"
                    WHERE "personalities"."project_id" = "automation_triggers"."project_id"
                      AND "personalities"."name" = 'Default'
                    LIMIT 1
                ),
                "updated_at" = CURRENT_TIMESTAMP
            WHERE "personality_id" IS NULL;
            "#,
        ))
        .await
        .map(|_| ())
}

async fn create_work_item_relationships(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkItemRelationships::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkItemRelationships::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkItemRelationships::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemRelationships::SourceWorkItemId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemRelationships::TargetWorkItemId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemRelationships::Kind)
                        .text()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemRelationships::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(WorkItemRelationships::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_relationships_project_id")
                        .from(
                            WorkItemRelationships::Table,
                            WorkItemRelationships::ProjectId,
                        )
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_relationships_source")
                        .from(
                            WorkItemRelationships::Table,
                            WorkItemRelationships::SourceWorkItemId,
                        )
                        .to(WorkItems::Table, WorkItems::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_relationships_target")
                        .from(
                            WorkItemRelationships::Table,
                            WorkItemRelationships::TargetWorkItemId,
                        )
                        .to(WorkItems::Table, WorkItems::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_work_item_relationships_touching_source")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::SourceWorkItemId)
                .if_not_exists()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_work_item_relationships_touching_target")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::TargetWorkItemId)
                .if_not_exists()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_work_item_relationships_unique")
                .table(WorkItemRelationships::Table)
                .col(WorkItemRelationships::ProjectId)
                .col(WorkItemRelationships::SourceWorkItemId)
                .col(WorkItemRelationships::TargetWorkItemId)
                .col(WorkItemRelationships::Kind)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn update_default_open_work_selector(
    manager: &SchemaManager<'_>,
    target_selector: &str,
) -> Result<(), DbErr> {
    let source_selector = if target_selector == DEFAULT_WORK_ITEM_SELECTOR {
        OLD_DEFAULT_WORK_ITEM_SELECTOR
    } else {
        DEFAULT_WORK_ITEM_SELECTOR
    };
    manager
        .get_connection()
        .execute(Statement::from_sql_and_values(
            manager.get_database_backend(),
            r#"
            UPDATE "automation_triggers"
            SET "work_item_selector" = ?1,
                "updated_at" = CURRENT_TIMESTAMP
            WHERE "name" = 'Claim open work'
              AND "activation" = 'work_item'
              AND "effect" = 'consume_work'
              AND "work_item_selector" = ?2;
            "#,
            vec![
                target_selector.to_owned().into(),
                source_selector.to_owned().into(),
            ],
        ))
        .await
        .map(|_| ())
}

async fn update_automation_selector_if_unchanged(
    manager: &SchemaManager<'_>,
    name: &str,
    source_selector: &str,
    target_selector: &str,
) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_sql_and_values(
            manager.get_database_backend(),
            r#"
            UPDATE "automation_triggers"
            SET "work_item_selector" = ?3,
                "updated_at" = CURRENT_TIMESTAMP
            WHERE "name" = ?1
              AND "activation" = 'work_item'
              AND "effect" = 'consume_work'
              AND "work_item_selector" = ?2;
            "#,
            vec![
                name.to_owned().into(),
                source_selector.to_owned().into(),
                target_selector.to_owned().into(),
            ],
        ))
        .await
        .map(|_| ())
}

async fn update_automation_prompt_if_unchanged(
    manager: &SchemaManager<'_>,
    name: &str,
    source_prompt: &str,
    target_prompt: &str,
) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_sql_and_values(
            manager.get_database_backend(),
            r#"
            UPDATE "automation_triggers"
            SET "prompt" = ?3,
                "updated_at" = CURRENT_TIMESTAMP
            WHERE "name" = ?1
              AND "activation" = 'work_item'
              AND "effect" = 'consume_work'
              AND "prompt" = ?2;
            "#,
            vec![
                name.to_owned().into(),
                source_prompt.to_owned().into(),
                target_prompt.to_owned().into(),
            ],
        ))
        .await
        .map(|_| ())
}

async fn seed_label_routed_automation(
    manager: &SchemaManager<'_>,
    name: &str,
    mode: &str,
    prompt: &str,
    selector: &str,
    priority: i64,
) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_sql_and_values(
            manager.get_database_backend(),
            r#"
            INSERT INTO "automation_triggers"
                (
                    "project_id",
                    "name",
                    "enabled",
                    "activation",
                    "effect",
                    "schedule",
                    "mode",
                    "tool_name",
                    "prompt",
                    "work_item_selector",
                    "priority",
                    "evaluation_count",
                    "pending_evaluation_count",
                    "last_evaluation_queued_at",
                    "last_evaluated_at",
                    "next_evaluation_at",
                    "last_event_id",
                    "created_at",
                    "updated_at"
                )
            SELECT
                "projects"."id",
                ?1,
                1,
                'work_item',
                'consume_work',
                '@every 15s',
                ?2,
                COALESCE(NULLIF("projects"."default_agent_tool", ''), 'codex'),
                ?3,
                ?4,
                ?5,
                0,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            FROM "projects"
            WHERE NOT EXISTS (
                SELECT 1
                FROM "automation_triggers"
                WHERE "automation_triggers"."project_id" = "projects"."id"
                  AND "automation_triggers"."name" = ?6
            );
            "#,
            vec![
                name.to_owned().into(),
                mode.to_owned().into(),
                prompt.to_owned().into(),
                selector.to_owned().into(),
                priority.into(),
                name.to_owned().into(),
            ],
        ))
        .await
        .map(|_| ())
}

async fn delete_label_routed_automation(
    manager: &SchemaManager<'_>,
    name: &str,
    prompt: &str,
    selector: &str,
) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_sql_and_values(
            manager.get_database_backend(),
            r#"
            DELETE FROM "automation_triggers"
            WHERE "name" = ?1
              AND "activation" = 'work_item'
              AND "effect" = 'consume_work'
              AND "prompt" = ?2
              AND "work_item_selector" = ?3;
            "#,
            vec![
                name.to_owned().into(),
                prompt.to_owned().into(),
                selector.to_owned().into(),
            ],
        ))
        .await
        .map(|_| ())
}

async fn seed_default_work_item_automations(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            r#"
            INSERT INTO "automation_triggers"
                (
                    "project_id",
                    "name",
                    "enabled",
                    "activation",
                    "effect",
                    "schedule",
                    "mode",
                    "tool_name",
                    "prompt",
                    "work_item_selector",
                    "priority",
                    "evaluation_count",
                    "pending_evaluation_count",
                    "last_evaluation_queued_at",
                    "last_evaluated_at",
                    "next_evaluation_at",
                    "last_event_id",
                    "created_at",
                    "updated_at"
                )
            SELECT
                "projects"."id",
                'Claim open work',
                1,
                'work_item',
                'consume_work',
                '@every 15s',
                'execute',
                COALESCE(NULLIF("projects"."default_agent_tool", ''), 'codex'),
                '',
                '{"All":[{"column_name":"state","operator":"=","value":{"String":"open"}}]}',
                0,
                0,
                0,
                NULL,
                NULL,
                NULL,
                NULL,
                CURRENT_TIMESTAMP,
                CURRENT_TIMESTAMP
            FROM "projects"
            WHERE NOT EXISTS (
                SELECT 1
                FROM "automation_triggers"
                WHERE "automation_triggers"."project_id" = "projects"."id"
                  AND "automation_triggers"."activation" IN ('work_item', 'manual')
            );
            "#,
        ))
        .await
        .map(|_| ())
}

async fn create_work_item_labels(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkItemLabels::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkItemLabels::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkItemLabels::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemLabels::WorkItemId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(WorkItemLabels::LabelKey).string().not_null())
                .col(ColumnDef::new(WorkItemLabels::LabelValue).string().null())
                .col(
                    ColumnDef::new(WorkItemLabels::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(WorkItemLabels::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_labels_project_id")
                        .from(WorkItemLabels::Table, WorkItemLabels::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_labels_work_item_id")
                        .from(WorkItemLabels::Table, WorkItemLabels::WorkItemId)
                        .to(WorkItems::Table, WorkItems::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_work_item_labels_project_key_value")
                .table(WorkItemLabels::Table)
                .col(WorkItemLabels::ProjectId)
                .col(WorkItemLabels::LabelKey)
                .col(WorkItemLabels::LabelValue)
                .if_not_exists()
                .to_owned(),
        )
        .await?;
    manager
        .create_index(
            Index::create()
                .name("idx_work_item_labels_unique_item_key")
                .table(WorkItemLabels::Table)
                .col(WorkItemLabels::WorkItemId)
                .col(WorkItemLabels::LabelKey)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn create_swim_lanes(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(SwimLanes::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(SwimLanes::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(SwimLanes::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(ColumnDef::new(SwimLanes::Identifier).string().not_null())
                .col(ColumnDef::new(SwimLanes::Name).string().not_null())
                .col(
                    ColumnDef::new(SwimLanes::Position)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(SwimLanes::Filter)
                        .text()
                        .not_null()
                        .default("{\"All\":[]}"),
                )
                .col(
                    ColumnDef::new(SwimLanes::ItemOrder)
                        .string()
                        .not_null()
                        .default("updated_desc"),
                )
                .col(
                    ColumnDef::new(SwimLanes::CanCreateItems)
                        .boolean()
                        .not_null()
                        .default(false),
                )
                .col(
                    ColumnDef::new(SwimLanes::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(SwimLanes::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_swim_lanes_project_id")
                        .from(SwimLanes::Table, SwimLanes::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_swim_lanes_unique_project_identifier")
                .table(SwimLanes::Table)
                .col(SwimLanes::ProjectId)
                .col(SwimLanes::Identifier)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn create_work_item_states(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .create_table(
            Table::create()
                .table(WorkItemStates::Table)
                .if_not_exists()
                .col(
                    ColumnDef::new(WorkItemStates::Id)
                        .big_integer()
                        .not_null()
                        .auto_increment()
                        .primary_key(),
                )
                .col(
                    ColumnDef::new(WorkItemStates::ProjectId)
                        .big_integer()
                        .not_null(),
                )
                .col(
                    ColumnDef::new(WorkItemStates::Identifier)
                        .string()
                        .not_null(),
                )
                .col(ColumnDef::new(WorkItemStates::Name).string().not_null())
                .col(
                    ColumnDef::new(WorkItemStates::Position)
                        .big_integer()
                        .not_null()
                        .default(0),
                )
                .col(
                    ColumnDef::new(WorkItemStates::CreatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .col(
                    ColumnDef::new(WorkItemStates::UpdatedAt)
                        .string()
                        .not_null()
                        .default(Expr::current_timestamp()),
                )
                .foreign_key(
                    ForeignKey::create()
                        .name("fk_work_item_states_project_id")
                        .from(WorkItemStates::Table, WorkItemStates::ProjectId)
                        .to(Projects::Table, Projects::Id)
                        .on_delete(ForeignKeyAction::Cascade),
                )
                .to_owned(),
        )
        .await?;

    manager
        .create_index(
            Index::create()
                .name("idx_work_item_states_unique_project_identifier")
                .table(WorkItemStates::Table)
                .col(WorkItemStates::ProjectId)
                .col(WorkItemStates::Identifier)
                .unique()
                .if_not_exists()
                .to_owned(),
        )
        .await
}

async fn migrate_work_item_state_to_labels(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let conn = manager.get_connection();
    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "work_item_labels"
            ("project_id", "work_item_id", "label_key", "label_value", "created_at", "updated_at")
        SELECT
            "project_id",
            "id",
            'state',
            COALESCE(NULLIF("state", ''), 'open'),
            COALESCE("created_at", CURRENT_TIMESTAMP),
            COALESCE("updated_at", CURRENT_TIMESTAMP)
        FROM "work_items"
        WHERE "state" IS NOT NULL;
        "#,
    ))
    .await?;

    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "swim_lanes"
            ("project_id", "identifier", "name", "position", "can_create_items", "created_at", "updated_at")
        SELECT "id", 'idea', 'Idea', 10, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
        FROM "projects";
        "#,
    ))
    .await?;
    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "swim_lanes"
            ("project_id", "identifier", "name", "position", "can_create_items", "created_at", "updated_at")
        SELECT "id", 'open', 'Open', 20, 1, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
        FROM "projects";
        "#,
    ))
    .await?;
    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "swim_lanes"
            ("project_id", "identifier", "name", "position", "can_create_items", "created_at", "updated_at")
        SELECT "id", 'in_progress', 'In progress', 30, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
        FROM "projects";
        "#,
    ))
    .await?;
    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "swim_lanes"
            ("project_id", "identifier", "name", "position", "can_create_items", "created_at", "updated_at")
        SELECT "id", 'done', 'Done', 40, 0, CURRENT_TIMESTAMP, CURRENT_TIMESTAMP
        FROM "projects";
        "#,
    ))
    .await?;
    Ok(())
}

async fn seed_work_item_states_from_existing_data(
    manager: &SchemaManager<'_>,
) -> Result<(), DbErr> {
    let backend = manager.get_database_backend();
    let conn = manager.get_connection();

    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "work_item_states"
            ("project_id", "identifier", "name", "position", "created_at", "updated_at")
        SELECT
            "project_id",
            "identifier",
            "name",
            "position",
            COALESCE("created_at", CURRENT_TIMESTAMP),
            COALESCE("updated_at", CURRENT_TIMESTAMP)
        FROM "swim_lanes";
        "#,
    ))
    .await?;

    conn.execute(Statement::from_string(
        backend,
        r#"
        INSERT OR IGNORE INTO "work_item_states"
            ("project_id", "identifier", "name", "position", "created_at", "updated_at")
        SELECT
            "work_item_labels"."project_id",
            "work_item_labels"."label_value",
            REPLACE("work_item_labels"."label_value", '_', ' '),
            1000,
            CURRENT_TIMESTAMP,
            CURRENT_TIMESTAMP
        FROM "work_item_labels"
        WHERE "work_item_labels"."label_key" = 'state'
          AND "work_item_labels"."label_value" IS NOT NULL
          AND "work_item_labels"."label_value" != '';
        "#,
    ))
    .await
    .map(|_| ())
}

async fn seed_swim_lane_filters_from_identifiers(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            r#"
            UPDATE "swim_lanes"
            SET
                "filter" = '{"All":[{"column_name":"state","operator":"=","value":{"String":"' ||
                    REPLACE(REPLACE("identifier", '\', '\\'), '"', '\"') ||
                    '"}}]}',
                "item_order" = COALESCE(NULLIF("item_order", ''), 'updated_desc')
            WHERE "filter" IS NULL
               OR "filter" = ''
               OR "filter" = '{"All":[]}';
            "#,
        ))
        .await
        .map(|_| ())
}

async fn create_read_views(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    create_read_view(manager, "projects", "projects_read_view").await?;
    create_read_view(manager, "work_items", "work_items_read_view").await?;
    create_read_view(manager, "comments", "comments_read_view").await
}

async fn drop_read_views(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    drop_read_view(manager, "comments_read_view").await?;
    drop_read_view(manager, "work_items_read_view").await?;
    drop_read_view(manager, "projects_read_view").await
}

async fn create_read_view(
    manager: &SchemaManager<'_>,
    table_name: &str,
    view_name: &str,
) -> Result<(), DbErr> {
    drop_read_view(manager, view_name).await?;
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            format!(
                r#"
                CREATE VIEW "{view_name}" AS
                SELECT "{table_name}".*, 0 AS has_validation_errors
                FROM "{table_name}";
                "#
            ),
        ))
        .await
        .map(|_| ())
}

async fn create_work_items_read_view(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    drop_read_view(manager, "work_items_read_view").await?;
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            r#"
            CREATE VIEW "work_items_read_view" AS
            SELECT
                "work_items".*,
                (
                    SELECT "work_item_labels"."label_value"
                    FROM "work_item_labels"
                    WHERE "work_item_labels"."project_id" = "work_items"."project_id"
                      AND "work_item_labels"."work_item_id" = "work_items"."id"
                      AND "work_item_labels"."label_key" = 'state'
                    LIMIT 1
                ) AS "state_label",
                0 AS "has_validation_errors"
            FROM "work_items";
            "#,
        ))
        .await
        .map(|_| ())
}

async fn create_automation_triggers_read_view(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    drop_read_view(manager, "automation_triggers_read_view").await?;
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            r#"
            CREATE VIEW "automation_triggers_read_view" AS
            SELECT
                "automation_triggers".*,
                "personalities"."name" AS "personality_name",
                0 AS "has_validation_errors"
            FROM "automation_triggers"
            LEFT JOIN "personalities"
              ON "personalities"."id" = "automation_triggers"."personality_id"
             AND "personalities"."project_id" = "automation_triggers"."project_id";
            "#,
        ))
        .await
        .map(|_| ())
}

async fn drop_read_view(manager: &SchemaManager<'_>, view_name: &str) -> Result<(), DbErr> {
    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            format!(r#"DROP VIEW IF EXISTS "{view_name}";"#),
        ))
        .await
        .map(|_| ())
}

async fn rename_project_path_column(
    manager: &SchemaManager<'_>,
    from: &str,
    to: &str,
) -> Result<(), DbErr> {
    drop_read_view(manager, "projects_read_view").await?;
    if column_exists(manager, "projects", to).await? {
        if column_exists(manager, "projects", from).await? {
            manager
                .get_connection()
                .execute(Statement::from_string(
                    manager.get_database_backend(),
                    format!(
                        r#"
                        UPDATE "projects"
                        SET "{to}" = COALESCE(NULLIF("{to}", ''), "{from}")
                        WHERE "{from}" IS NOT NULL;
                        "#
                    ),
                ))
                .await?;
        }
    } else if column_exists(manager, "projects", from).await? {
        manager
            .get_connection()
            .execute(Statement::from_string(
                manager.get_database_backend(),
                format!(r#"ALTER TABLE "projects" RENAME COLUMN "{from}" TO "{to}";"#),
            ))
            .await?;
    } else {
        add_column_if_missing(manager, "projects", to, "TEXT").await?;
    }
    create_read_view(manager, "projects", "projects_read_view").await
}

async fn add_project_run_settings_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    add_column_if_missing(
        manager,
        "projects",
        "workspace_mode",
        "TEXT NOT NULL DEFAULT 'current_branch'",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "max_code_edit_agents",
        "BIGINT NOT NULL DEFAULT 1",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "max_read_only_agents",
        "BIGINT NOT NULL DEFAULT 2",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "create_pr",
        "BOOLEAN NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "stale_claim_minutes",
        "BIGINT NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "worktree_cleanup_policy",
        "TEXT NOT NULL DEFAULT 'manual'",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "default_agent_tool",
        "TEXT NOT NULL DEFAULT 'codex'",
    )
    .await?;
    add_column_if_missing(manager, "projects", "default_agent_model", "TEXT").await?;
    add_column_if_missing(
        manager,
        "projects",
        "default_agent_reasoning_effort",
        "TEXT",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "agent_sandbox_mode",
        "TEXT NOT NULL DEFAULT 'workspace_write'",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "agent_extra_writable_roots",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    let default_git_policy = format!(
        "TEXT NOT NULL DEFAULT '{}'",
        DEFAULT_AGENT_GIT_COMMAND_POLICY
    );
    add_column_if_missing(
        manager,
        "projects",
        "agent_git_command_policy",
        &default_git_policy,
    )
    .await
}

async fn add_project_commit_policy_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    add_column_if_missing(
        manager,
        "projects",
        "auto_commit",
        "BOOLEAN NOT NULL DEFAULT 1",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "commit_standard",
        "TEXT NOT NULL DEFAULT ''",
    )
    .await?;
    add_column_if_missing(
        manager,
        "projects",
        "revert_strategy",
        "TEXT NOT NULL DEFAULT 'manual'",
    )
    .await
}

async fn add_project_path_status_columns(manager: &SchemaManager<'_>) -> Result<(), DbErr> {
    add_column_if_missing(
        manager,
        "projects",
        "path_exists",
        "BOOLEAN NOT NULL DEFAULT 0",
    )
    .await?;
    add_column_if_missing(manager, "projects", "path_checked_at", "TEXT").await
}

async fn add_column_if_missing(
    manager: &SchemaManager<'_>,
    table_name: &str,
    column_name: &str,
    column_type: &str,
) -> Result<(), DbErr> {
    if column_exists(manager, table_name, column_name).await? {
        return Ok(());
    }

    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            format!("ALTER TABLE \"{table_name}\" ADD COLUMN \"{column_name}\" {column_type};"),
        ))
        .await
        .map(|_| ())
}

async fn drop_column_if_present(
    manager: &SchemaManager<'_>,
    table_name: &str,
    column_name: &str,
) -> Result<(), DbErr> {
    if !column_exists(manager, table_name, column_name).await? {
        return Ok(());
    }

    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            format!(r#"ALTER TABLE "{table_name}" DROP COLUMN "{column_name}";"#),
        ))
        .await
        .map(|_| ())
}

async fn rename_column_if_present(
    manager: &SchemaManager<'_>,
    table_name: &str,
    from: &str,
    to: &str,
) -> Result<(), DbErr> {
    if column_exists(manager, table_name, to).await? {
        return Ok(());
    }
    if !column_exists(manager, table_name, from).await? {
        return Ok(());
    }

    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            format!(r#"ALTER TABLE "{table_name}" RENAME COLUMN "{from}" TO "{to}";"#),
        ))
        .await
        .map(|_| ())
}

async fn drop_index_if_present(manager: &SchemaManager<'_>, index_name: &str) -> Result<(), DbErr> {
    if !index_exists(manager, index_name).await? {
        return Ok(());
    }

    manager
        .get_connection()
        .execute(Statement::from_string(
            manager.get_database_backend(),
            format!(r#"DROP INDEX "{index_name}";"#),
        ))
        .await
        .map(|_| ())
}

async fn index_exists(manager: &SchemaManager<'_>, index_name: &str) -> Result<bool, DbErr> {
    Ok(manager
        .get_connection()
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            format!("SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = '{index_name}'"),
        ))
        .await?
        .is_some())
}

async fn column_exists(
    manager: &SchemaManager<'_>,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DbErr> {
    Ok(manager
        .get_connection()
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            format!("SELECT 1 FROM pragma_table_info('{table_name}') WHERE name = '{column_name}'"),
        ))
        .await?
        .is_some())
}

async fn table_exists(manager: &SchemaManager<'_>, table_name: &str) -> Result<bool, DbErr> {
    Ok(manager
        .get_connection()
        .query_one(Statement::from_string(
            manager.get_database_backend(),
            format!("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '{table_name}'"),
        ))
        .await?
        .is_some())
}
