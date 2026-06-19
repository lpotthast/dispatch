use clap::{Args, Parser, Subcommand};
use patchbay_types::{AgentReasoningEffort, AuthorType};

use crate::context::ContextOverrides;

#[derive(Debug, Parser)]
#[command(name = "patchbay")]
#[command(about = "Patchbay agent-facing API relay")]
pub(crate) struct Cli {
    /// Override the Patchbay API URL.
    #[arg(long)]
    api_url: Option<String>,

    /// Override the project context.
    #[arg(long)]
    project: Option<String>,

    /// Override the agent id.
    #[arg(long)]
    agent: Option<String>,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) fn context_overrides(&self) -> ContextOverrides {
        ContextOverrides {
            api_url: self.api_url.clone(),
            project: self.project.clone(),
            agent_id: self.agent.clone(),
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
    /// Guarded git entrypoint used by Patchbay automation.
    #[command(hide = true)]
    Git(GitArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum ItemCommand {
    /// List project work items.
    List(ItemListArgs),
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
}

#[derive(Debug, Args)]
pub(crate) struct GitArgs {
    /// Git arguments passed by the run-specific Patchbay git shim.
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
    use clap::Parser;

    use super::*;

    fn help_output(args: &[&str]) -> String {
        let error = Cli::try_parse_from(args).expect_err("help should stop parsing");
        assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        error.to_string()
    }

    fn assert_command_tree_has_help(command: &clap::Command, path: &mut Vec<String>) {
        for arg in command.get_arguments() {
            let id = arg.get_id().as_str();
            if id == "help" || id == "version" {
                continue;
            }
            assert!(
                arg.get_help()
                    .is_some_and(|help| !help.to_string().trim().is_empty()),
                "missing help for argument {id} on {}",
                path.join(" ")
            );
        }

        for subcommand in command.get_subcommands() {
            if subcommand.get_name() == "help" {
                continue;
            }
            path.push(subcommand.get_name().to_owned());
            assert!(
                subcommand
                    .get_about()
                    .or_else(|| subcommand.get_long_about())
                    .is_some_and(|about| !about.to_string().trim().is_empty()),
                "missing help for command {}",
                path.join(" ")
            );
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
        let root = help_output(&["patchbay", "--help"]);
        assert!(root.contains("Work with project-scoped items"));
        assert!(root.contains("Read and add item comments"));
        assert!(root.contains("Manage work item labels"));
        assert!(root.contains("Manage directed relationships between work items"));
        assert!(root.contains("Read and update project memory"));
        assert!(root.contains("Inspect automation runs and logs"));
        assert!(root.contains("Override the Patchbay API URL"));
        assert!(root.contains("Override the project context"));
        assert!(root.contains("Override the agent id"));

        let item = help_output(&["patchbay", "item", "--help"]);
        assert!(item.contains("List project work items"));
        assert!(item.contains("Show one item; defaults to the claimed item"));
        assert!(item.contains("Create a new work item"));
        assert!(item.contains("Edit item fields"));
        assert!(item.contains("Claim the next available item for this agent"));
        assert!(item.contains("Add an agent progress comment"));
        assert!(item.contains("Mark an item done with a final report"));
        assert!(item.contains("Release an item back to the queue"));
        assert!(item.contains("Ask the user for feedback and pause automation"));
        assert!(item.contains("Poll an item and print version changes"));

        let comment = help_output(&["patchbay", "comment", "--help"]);
        assert!(comment.contains("Add a comment to an item"));
        assert!(comment.contains("List comments on an item"));

        let label = help_output(&["patchbay", "label", "--help"]);
        assert!(label.contains("List labels on an item"));
        assert!(label.contains("Add a label to an item"));
        assert!(label.contains("Update a label on an item"));
        assert!(label.contains("Delete a label from an item"));
        assert!(label.contains("List labels already used in this project"));

        let relationship = help_output(&["patchbay", "relationship", "--help"]);
        assert!(relationship.contains("List relationships touching an item"));
        assert!(relationship.contains("Create a relationship from an item to a target item"));
        assert!(relationship.contains("Update a relationship kind"));
        assert!(relationship.contains("Delete a relationship"));

        let memory = help_output(&["patchbay", "memory", "--help"]);
        assert!(memory.contains("Show current project memory"));
        assert!(memory.contains("List project memory change events"));
        assert!(memory.contains("Replace project memory"));
        assert!(memory.contains("Append text to project memory"));

        let automation = help_output(&["patchbay", "automation", "--help"]);
        assert!(automation.contains("List automation runs"));
        assert!(automation.contains("Show one automation run log"));
    }

    #[test]
    fn leaf_help_describes_arguments() {
        let create = help_output(&["patchbay", "item", "create", "--help"]);
        assert!(create.contains("Title for the new item"));
        assert!(create.contains("Full task description"));
        assert!(create.contains("--label <KEY[=VALUE]>"));
        assert!(create.contains("Initial label key or key/value pair"));
        assert!(create.contains("Initial item state label"));
        assert!(create.contains("Reasoning effort override for this item"));
        assert!(create.contains("Print JSON instead of text"));

        let update = help_output(&["patchbay", "item", "update", "--help"]);
        assert!(update.contains("Item id; defaults to the claimed item"));
        assert!(update.contains("Move the item to a new state label"));
        assert!(update.contains("Clear the item-specific agent model"));
        assert!(update.contains("Require the current item version"));

        let label_add = help_output(&["patchbay", "label", "add", "--help"]);
        assert!(label_add.contains("Label key"));
        assert!(label_add.contains("Optional label value"));

        let relationship_add = help_output(&["patchbay", "relationship", "add", "--help"]);
        assert!(relationship_add.contains("Source item id; defaults to the claimed item"));
        assert!(relationship_add.contains("Target item id"));
        assert!(relationship_add.contains("Free-form relationship kind"));

        let relationship_update = help_output(&["patchbay", "relationship", "update", "--help"]);
        assert!(relationship_update.contains("Relationship id to update"));
        assert!(relationship_update.contains("Replacement free-form relationship kind"));

        let progress = help_output(&["patchbay", "item", "progress", "--help"]);
        assert!(progress.contains("Progress text to record"));

        let request_feedback = help_output(&["patchbay", "item", "request-feedback", "--help"]);
        assert!(request_feedback.contains("Feedback request to show the user"));

        let comment = help_output(&["patchbay", "comment", "add", "--help"]);
        assert!(comment.contains("Comment text"));
        assert!(comment.contains("Author type for the comment"));

        let memory = help_output(&["patchbay", "memory", "append", "--help"]);
        assert!(memory.contains("Memory text to write"));

        let automation = help_output(&["patchbay", "automation", "log", "--help"]);
        assert!(automation.contains("Automation run id"));
    }

    #[test]
    fn item_create_accepts_repeated_label_options() {
        let cli = Cli::parse_from([
            "patchbay",
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
                assert_eq!(
                    args.labels,
                    vec!["type=feature".to_owned(), "needs-verification".to_owned()]
                );
            }
            command => panic!("expected item create command, got {command:?}"),
        }
    }
}
