use clap::{Args, Parser, Subcommand};
use dispatch_types::{AgentReasoningEffort, AuthorType};

use crate::context::ContextOverrides;

#[derive(Debug, Parser)]
#[command(name = "dispatch")]
#[command(about = "Dispatch agent-facing API relay")]
pub(crate) struct Cli {
    /// Override the Dispatch API URL.
    #[arg(long)]
    api_url: Option<String>,

    /// Override the project context.
    #[arg(long)]
    project: Option<String>,

    /// Override the agent id.
    #[arg(long)]
    agent: Option<String>,

    /// Override the Dispatch agent-run id used for request attribution.
    #[arg(long)]
    agent_run: Option<i64>,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) fn context_overrides(&self) -> ContextOverrides {
        ContextOverrides {
            api_url: self.api_url.clone(),
            project: self.project.clone(),
            agent_id: self.agent.clone(),
            agent_run_id: self.agent_run,
        }
    }

    pub(crate) fn into_command(self) -> Command {
        self.command
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Work with project-scoped items.
    Item {
        #[command(subcommand)]
        command: ItemCommand,
    },
    /// Read and add item comments.
    Comment {
        #[command(subcommand)]
        command: CommentCommand,
    },
    /// Manage work item labels.
    Label {
        #[command(subcommand)]
        command: LabelCommand,
    },
    /// Manage directed relationships between work items.
    Relationship {
        #[command(subcommand)]
        command: RelationshipCommand,
    },
    /// Group related work items for board and swim-lane display.
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    /// Read and update project memory.
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },
    /// Inspect automation runs and logs.
    Automation {
        #[command(subcommand)]
        command: AutomationCommand,
    },
    /// Guarded git entrypoint used by Dispatch automation.
    #[command(hide = true)]
    Git(GitArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum ItemCommand {
    /// List project work items.
    List(ItemListArgs),
    /// Search project work items with composable filters and cursor pagination.
    Search(ItemSearchArgs),
    /// Show one item; defaults to the claimed item.
    Show(ItemIdArgs),
    /// Create a new work item.
    Create(ItemCreateArgs),
    /// Edit item fields.
    Update(ItemUpdateArgs),
    /// Claim the next available item for this agent.
    Claim(ItemClaimArgs),
    /// Add an agent progress comment.
    Progress(ItemProgressArgs),
    /// Mark an item done with a final report.
    Finish(ItemFinishArgs),
    /// Release an item back to the queue.
    Release(ItemReleaseArgs),
    /// Ask the user for feedback and pause automation.
    RequestFeedback(ItemRequestFeedbackArgs),
    /// Poll an item and print version changes.
    Watch(ItemWatchArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum CommentCommand {
    /// Add a comment to an item.
    Add(CommentAddArgs),
    /// List comments on an item.
    List(ItemIdArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum LabelCommand {
    /// List labels on an item.
    List(LabelListArgs),
    /// Add a label to an item.
    Add(LabelAddArgs),
    /// Update a label on an item.
    Update(LabelUpdateArgs),
    /// Delete a label from an item.
    Delete(LabelDeleteArgs),
    /// List labels already used in this project.
    Suggestions(JsonArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum RelationshipCommand {
    /// List relationships touching an item.
    List(RelationshipListArgs),
    /// Create a relationship from an item to a target item.
    Add(RelationshipAddArgs),
    /// Update a relationship kind.
    Update(RelationshipUpdateArgs),
    /// Delete a relationship.
    Delete(RelationshipDeleteArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum GroupCommand {
    /// List project work-item groups.
    List(JsonArgs),
    /// Create an idempotent project work-item group.
    Create(GroupCreateArgs),
    /// Assign one or more items to an existing group atomically.
    Assign(GroupAssignArgs),
}

#[derive(Debug, Args)]
pub(crate) struct GroupCreateArgs {
    /// Stable project-scoped group key.
    #[arg(long)]
    pub(crate) key: String,
    /// Human-readable group name.
    #[arg(long)]
    pub(crate) name: String,
    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GroupAssignArgs {
    /// Stable key of the target group.
    #[arg(long)]
    pub(crate) key: String,
    /// Item id to assign; may be repeated.
    #[arg(long = "item", required = true)]
    pub(crate) item_ids: Vec<i64>,
    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Subcommand)]
pub(crate) enum MemoryCommand {
    /// Show current project memory.
    Show(JsonArgs),
    /// List project memory change events.
    History(JsonArgs),
    /// Replace project memory.
    Set(MemoryWriteArgs),
    /// Append text to project memory.
    Append(MemoryWriteArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum AutomationCommand {
    /// List automation runs.
    Runs(AutomationRunsArgs),
    /// Show one automation run log.
    Log(AutomationRunLogArgs),
    /// Inspect configured automation triggers.
    Triggers {
        #[command(subcommand)]
        command: AutomationTriggersCommand,
    },
    /// Explain current automation routing for an item.
    Routing {
        #[command(subcommand)]
        command: AutomationRoutingCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AutomationTriggersCommand {
    /// List automation triggers.
    List(JsonArgs),
    /// Show one automation trigger by id or managed key.
    Show(AutomationTriggerShowArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum AutomationRoutingCommand {
    /// Explain matching, exclusivity, fairness, and admission blockers.
    Explain(AutomationRoutingExplainArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AutomationRoutingExplainArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,
    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct GitArgs {
    /// Git arguments passed by the run-specific Dispatch git shim.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ItemListArgs {
    /// Filter items by state label value.
    #[arg(long)]
    pub(crate) state: Option<String>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemSearchArgs {
    /// Filter by state; may be repeated.
    #[arg(long = "state")]
    pub(crate) states: Vec<String>,
    /// Filter by label key or key=value; may be repeated.
    #[arg(long = "label", value_name = "KEY[=VALUE]")]
    pub(crate) labels: Vec<String>,
    /// Additional CrudKit Condition selector as JSON.
    #[arg(long)]
    pub(crate) selector_json: Option<String>,
    /// Search title and description.
    #[arg(long)]
    pub(crate) text: Option<String>,
    /// Return only finished items.
    #[arg(long, conflicts_with = "unfinished")]
    pub(crate) finished: bool,
    /// Return only unfinished items.
    #[arg(long, conflicts_with = "finished")]
    pub(crate) unfinished: bool,
    /// Filter items created by an attributed run.
    #[arg(long)]
    pub(crate) created_by_run: Option<i64>,
    /// Filter items produced by an automation trigger.
    #[arg(long)]
    pub(crate) produced_by_trigger: Option<i64>,
    /// Filter items touching a relationship of this kind.
    #[arg(long)]
    pub(crate) relationship_kind: Option<String>,
    /// Filter by an RFC3339 lower bound.
    #[arg(long)]
    pub(crate) updated_since: Option<String>,
    /// Page size, from 1 through 200.
    #[arg(long)]
    pub(crate) limit: Option<u64>,
    /// Opaque cursor from a previous search page.
    #[arg(long)]
    pub(crate) cursor: Option<String>,
    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct AutomationTriggerShowArgs {
    /// Automation trigger id, managed object key, or name.
    pub(crate) id_or_key: String,
    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemIdArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemCreateArgs {
    /// Title for the new item.
    #[arg(long)]
    pub(crate) title: String,

    /// Full task description.
    #[arg(long)]
    pub(crate) description: String,

    /// Initial label key or key/value pair; may be repeated.
    #[arg(long = "label", value_name = "KEY[=VALUE]")]
    pub(crate) labels: Vec<String>,

    /// Initial item state label; defaults to open.
    #[arg(long)]
    pub(crate) state: Option<String>,

    /// Agent model override for this item.
    #[arg(long)]
    pub(crate) agent_model: Option<String>,

    /// Reasoning effort override for this item.
    #[arg(long)]
    pub(crate) agent_reasoning_effort: Option<AgentReasoningEffort>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemUpdateArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Replace the item title.
    #[arg(long)]
    pub(crate) title: Option<String>,

    /// Replace the item description.
    #[arg(long)]
    pub(crate) description: Option<String>,

    /// Move the item to a new state label.
    #[arg(long)]
    pub(crate) state: Option<String>,

    /// Set the item-specific agent model.
    #[arg(long)]
    pub(crate) agent_model: Option<String>,

    /// Clear the item-specific agent model.
    #[arg(long)]
    pub(crate) clear_agent_model: bool,

    /// Set the item-specific reasoning effort.
    #[arg(long)]
    pub(crate) agent_reasoning_effort: Option<AgentReasoningEffort>,

    /// Clear the item-specific reasoning effort.
    #[arg(long)]
    pub(crate) clear_agent_reasoning_effort: bool,

    /// Require the current item version.
    #[arg(long)]
    pub(crate) expect_version: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemClaimArgs {
    /// State label to claim from.
    #[arg(long, default_value = "open")]
    pub(crate) state: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemProgressArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Progress text to record.
    #[arg(long)]
    pub(crate) body: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemFinishArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Final report text.
    #[arg(long)]
    pub(crate) report: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemReleaseArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Optional release note.
    #[arg(long)]
    pub(crate) comment: Option<String>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemRequestFeedbackArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Feedback request to show the user.
    #[arg(long)]
    pub(crate) body: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct ItemWatchArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Only print versions newer than this value.
    #[arg(long)]
    pub(crate) since_version: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct LabelListArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct LabelAddArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Label key.
    #[arg(long)]
    pub(crate) key: String,

    /// Optional label value.
    #[arg(long)]
    pub(crate) value: Option<String>,

    /// Require the current item version.
    #[arg(long)]
    pub(crate) expect_version: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct LabelUpdateArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Label id to update.
    pub(crate) label_id: i64,

    /// Replacement label key.
    #[arg(long)]
    pub(crate) key: Option<String>,

    /// Replacement label value.
    #[arg(long)]
    pub(crate) value: Option<String>,

    /// Clear the label value.
    #[arg(long)]
    pub(crate) clear_value: bool,

    /// Require the current item version.
    #[arg(long)]
    pub(crate) expect_version: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct LabelDeleteArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Label id to delete.
    pub(crate) label_id: i64,

    /// Require the current item version.
    #[arg(long)]
    pub(crate) expect_version: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RelationshipListArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RelationshipAddArgs {
    /// Source item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Target item id.
    #[arg(long)]
    pub(crate) target: i64,

    /// Free-form relationship kind.
    #[arg(long)]
    pub(crate) kind: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RelationshipUpdateArgs {
    /// Relationship id to update.
    pub(crate) relationship_id: i64,

    /// Replacement free-form relationship kind.
    #[arg(long)]
    pub(crate) kind: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct RelationshipDeleteArgs {
    /// Relationship id to delete.
    pub(crate) relationship_id: i64,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct CommentAddArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Comment text.
    #[arg(long)]
    pub(crate) body: String,

    /// Display name for the author.
    #[arg(long)]
    pub(crate) author: Option<String>,

    /// Author type for the comment.
    #[arg(long, default_value = "user")]
    pub(crate) author_type: AuthorType,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct AutomationRunsArgs {
    /// Maximum number of runs to show.
    #[arg(long)]
    pub(crate) limit: Option<u64>,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct AutomationRunLogArgs {
    /// Automation run id.
    pub(crate) run_id: i64,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct JsonArgs {
    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[derive(Debug, Args)]
pub(crate) struct MemoryWriteArgs {
    /// Memory text to write.
    #[arg(long)]
    pub(crate) body: String,

    /// Print JSON instead of text.
    #[arg(long)]
    pub(crate) json: bool,
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use clap::Parser;

    use super::*;

    fn help_output(args: &[&str]) -> String {
        let error = Cli::try_parse_from(args).expect_err("help should stop parsing");
        assert_that!(&(error.kind())).is_equal_to(clap::error::ErrorKind::DisplayHelp);
        error.to_string()
    }

    fn assert_command_tree_has_help(command: &clap::Command, path: &mut Vec<String>) {
        for arg in command.get_arguments() {
            let id = arg.get_id().as_str();
            if id == "help" || id == "version" {
                continue;
            }
            assert_that!(
                &(arg
                    .get_help()
                    .is_some_and(|help| !help.to_string().trim().is_empty()))
            )
            .with_detail_message(format!(
                "missing help for argument {id} on {}",
                path.join(" ")
            ))
            .is_true();
        }

        for subcommand in command.get_subcommands() {
            if subcommand.get_name() == "help" {
                continue;
            }
            path.push(subcommand.get_name().to_owned());
            assert_that!(
                &(subcommand
                    .get_about()
                    .or_else(|| subcommand.get_long_about())
                    .is_some_and(|about| !about.to_string().trim().is_empty()))
            )
            .with_detail_message(format!("missing help for command {}", path.join(" ")))
            .is_true();
            assert_command_tree_has_help(subcommand, path);
            path.pop();
        }
    }

    #[test]
    fn clap_metadata_covers_every_command_and_argument() {
        let command = <Cli as clap::CommandFactory>::command();
        let mut path = vec![command.get_name().to_owned()];
        assert_command_tree_has_help(&command, &mut path);
    }

    #[test]
    fn help_describes_command_groups_and_subcommands() {
        let root = help_output(&["dispatch", "--help"]);
        assert_that!(&(root.contains("Work with project-scoped items"))).is_true();
        assert_that!(&(root.contains("Read and add item comments"))).is_true();
        assert_that!(&(root.contains("Manage work item labels"))).is_true();
        assert_that!(&(root.contains("Manage directed relationships between work items")))
            .is_true();
        assert_that!(&(root.contains("Group related work items for board and swim-lane display")))
            .is_true();
        assert_that!(&(root.contains("Read and update project memory"))).is_true();
        assert_that!(&(root.contains("Inspect automation runs and logs"))).is_true();
        assert_that!(&(root.contains("Override the Dispatch API URL"))).is_true();
        assert_that!(&(root.contains("Override the project context"))).is_true();
        assert_that!(&(root.contains("Override the agent id"))).is_true();

        let item = help_output(&["dispatch", "item", "--help"]);
        assert_that!(&(item.contains("List project work items"))).is_true();
        assert_that!(&(item.contains("Show one item; defaults to the claimed item"))).is_true();
        assert_that!(&(item.contains("Create a new work item"))).is_true();
        assert_that!(&(item.contains("Edit item fields"))).is_true();
        assert_that!(&(item.contains("Claim the next available item for this agent"))).is_true();
        assert_that!(&(item.contains("Add an agent progress comment"))).is_true();
        assert_that!(&(item.contains("Mark an item done with a final report"))).is_true();
        assert_that!(&(item.contains("Release an item back to the queue"))).is_true();
        assert_that!(&(item.contains("Ask the user for feedback and pause automation"))).is_true();
        assert_that!(&(item.contains("Poll an item and print version changes"))).is_true();

        let comment = help_output(&["dispatch", "comment", "--help"]);
        assert_that!(&(comment.contains("Add a comment to an item"))).is_true();
        assert_that!(&(comment.contains("List comments on an item"))).is_true();

        let label = help_output(&["dispatch", "label", "--help"]);
        assert_that!(&(label.contains("List labels on an item"))).is_true();
        assert_that!(&(label.contains("Add a label to an item"))).is_true();
        assert_that!(&(label.contains("Update a label on an item"))).is_true();
        assert_that!(&(label.contains("Delete a label from an item"))).is_true();
        assert_that!(&(label.contains("List labels already used in this project"))).is_true();

        let relationship = help_output(&["dispatch", "relationship", "--help"]);
        assert_that!(&(relationship.contains("List relationships touching an item"))).is_true();
        assert_that!(
            &(relationship.contains("Create a relationship from an item to a target item"))
        )
        .is_true();
        assert_that!(&(relationship.contains("Update a relationship kind"))).is_true();
        assert_that!(&(relationship.contains("Delete a relationship"))).is_true();

        let group = help_output(&["dispatch", "group", "--help"]);
        assert_that!(&(group.contains("List project work-item groups"))).is_true();
        assert_that!(&(group.contains("Create an idempotent project work-item group"))).is_true();
        assert_that!(&(group.contains("Assign one or more items to an existing group atomically")))
            .is_true();

        let memory = help_output(&["dispatch", "memory", "--help"]);
        assert_that!(&(memory.contains("Show current project memory"))).is_true();
        assert_that!(&(memory.contains("List project memory change events"))).is_true();
        assert_that!(&(memory.contains("Replace project memory"))).is_true();
        assert_that!(&(memory.contains("Append text to project memory"))).is_true();

        let automation = help_output(&["dispatch", "automation", "--help"]);
        assert_that!(&(automation.contains("List automation runs"))).is_true();
        assert_that!(&(automation.contains("Show one automation run log"))).is_true();
    }

    #[test]
    fn leaf_help_describes_arguments() {
        let create = help_output(&["dispatch", "item", "create", "--help"]);
        assert_that!(&(create.contains("Title for the new item"))).is_true();
        assert_that!(&(create.contains("Full task description"))).is_true();
        assert_that!(&(create.contains("--label <KEY[=VALUE]>"))).is_true();
        assert_that!(&(create.contains("Initial label key or key/value pair"))).is_true();
        assert_that!(&(create.contains("Initial item state label"))).is_true();
        assert_that!(&(create.contains("Reasoning effort override for this item"))).is_true();
        assert_that!(&(create.contains("Print JSON instead of text"))).is_true();

        let update = help_output(&["dispatch", "item", "update", "--help"]);
        assert_that!(&(update.contains("Item id; defaults to the claimed item"))).is_true();
        assert_that!(&(update.contains("Move the item to a new state label"))).is_true();
        assert_that!(&(update.contains("Clear the item-specific agent model"))).is_true();
        assert_that!(&(update.contains("Require the current item version"))).is_true();

        let label_add = help_output(&["dispatch", "label", "add", "--help"]);
        assert_that!(&(label_add.contains("Label key"))).is_true();
        assert_that!(&(label_add.contains("Optional label value"))).is_true();

        let relationship_add = help_output(&["dispatch", "relationship", "add", "--help"]);
        assert_that!(&(relationship_add.contains("Source item id; defaults to the claimed item")))
            .is_true();
        assert_that!(&(relationship_add.contains("Target item id"))).is_true();
        assert_that!(&(relationship_add.contains("Free-form relationship kind"))).is_true();

        let relationship_update = help_output(&["dispatch", "relationship", "update", "--help"]);
        assert_that!(&(relationship_update.contains("Relationship id to update"))).is_true();
        assert_that!(&(relationship_update.contains("Replacement free-form relationship kind")))
            .is_true();

        let progress = help_output(&["dispatch", "item", "progress", "--help"]);
        assert_that!(&(progress.contains("Progress text to record"))).is_true();

        let request_feedback = help_output(&["dispatch", "item", "request-feedback", "--help"]);
        assert_that!(&(request_feedback.contains("Feedback request to show the user"))).is_true();

        let comment = help_output(&["dispatch", "comment", "add", "--help"]);
        assert_that!(&(comment.contains("Comment text"))).is_true();
        assert_that!(&(comment.contains("Author type for the comment"))).is_true();

        let memory = help_output(&["dispatch", "memory", "append", "--help"]);
        assert_that!(&(memory.contains("Memory text to write"))).is_true();

        let automation = help_output(&["dispatch", "automation", "log", "--help"]);
        assert_that!(&(automation.contains("Automation run id"))).is_true();
    }

    #[test]
    fn item_create_accepts_repeated_label_options() {
        let cli = Cli::parse_from([
            "dispatch",
            "item",
            "create",
            "--title",
            "Title",
            "--description",
            "Description",
            "--label",
            "type=feature",
            "--label",
            "needs-verification",
        ]);

        match cli.into_command() {
            Command::Item {
                command: ItemCommand::Create(args),
            } => {
                assert_that!(&(args.labels)).is_equal_to(vec![
                    "type=feature".to_owned(),
                    "needs-verification".to_owned(),
                ]);
            }
            command => panic!("expected item create command, got {command:?}"),
        }
    }

    #[test]
    fn group_assign_accepts_repeated_item_options() {
        let cli = Cli::parse_from([
            "dispatch",
            "group",
            "assign",
            "--key",
            "review-42",
            "--item",
            "41",
            "--item",
            "42",
        ]);

        match cli.into_command() {
            Command::Group {
                command: GroupCommand::Assign(args),
            } => {
                assert_that!(&(args.key)).is_equal_to("review-42");
                assert_that!(&(args.item_ids)).is_equal_to(vec![41, 42]);
            }
            command => panic!("expected group assign command, got {command:?}"),
        }
    }
}
