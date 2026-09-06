mod automation;
mod comments;
mod common;
mod items;
mod knowledge;
mod labels;
mod relationships;
mod work_groups;

use clap::{Args, Parser, Subcommand};

use crate::{context::ContextOverrides, output};
pub(crate) use automation::{
    AutomationCommand, AutomationRoutingCommand, AutomationTriggersCommand,
};
pub(crate) use comments::CommentCommand;
pub(crate) use common::ItemIdArgs;
pub(crate) use items::{ItemCommand, ItemCreateArgs};
pub(crate) use knowledge::{
    KnowledgeCommand, KnowledgeJobCommand, KnowledgeNodeCommand, KnowledgeSourceCommand,
};
pub(crate) use labels::LabelCommand;
pub(crate) use relationships::RelationshipCommand;
pub(crate) use work_groups::GroupCommand;

#[derive(Debug, Parser)]
#[command(name = "dispatch")]
#[command(about = "Dispatch agent-facing API relay")]
pub(crate) struct Cli {
    #[command(flatten)]
    context: ContextOverrides,

    /// Print JSON instead of text.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

impl Cli {
    pub(crate) fn into_parts(self) -> (ContextOverrides, output::Format, Command) {
        (
            self.context,
            output::Format::from_json_flag(self.json),
            self.command,
        )
    }
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Inspect projects available from the Dispatch server.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
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
    /// Read and check project Markdown knowledge.
    Knowledge {
        #[command(subcommand)]
        command: KnowledgeCommand,
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
pub(crate) enum ProjectCommand {
    /// List projects available from the Dispatch server.
    List,
}

#[derive(Debug, Args)]
pub(crate) struct GitArgs {
    /// Git arguments passed by the run-specific Dispatch git shim.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
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
    fn knowledge_coverage_accepts_active_job_context_without_a_positional_id() {
        let cli = Cli::try_parse_from([
            "dispatch",
            "knowledge",
            "job",
            "coverage",
            "--aspect",
            "recovery",
            "--json",
        ]);
        assert_that!(&cli.is_ok()).is_true();
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
        assert_that!(&(root.contains("Inspect projects available from the Dispatch server")))
            .is_true();
        assert_that!(&(root.contains("Work with project-scoped items"))).is_true();
        assert_that!(&(root.contains("Read and add item comments"))).is_true();
        assert_that!(&(root.contains("Manage work item labels"))).is_true();
        assert_that!(&(root.contains("Manage directed relationships between work items")))
            .is_true();
        assert_that!(&(root.contains("Group related work items for board and swim-lane display")))
            .is_true();
        assert_that!(&(root.contains("Read and check project Markdown knowledge"))).is_true();
        assert_that!(&(root.contains("Inspect automation runs and logs"))).is_true();
        assert_that!(&(root.contains("Override the Dispatch API URL"))).is_true();
        assert_that!(&(root.contains("Override the project context"))).is_true();
        assert_that!(&(root.contains("Override the agent id"))).is_true();

        let project = help_output(&["dispatch", "project", "--help"]);
        assert_that!(&(project.contains("List projects available from the Dispatch server")))
            .is_true();

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

        let knowledge = help_output(&["dispatch", "knowledge", "--help"]);
        assert_that!(&knowledge).contains("without launching an agent");
        assert_that!(&knowledge).contains("Check frontmatter");
        assert_that!(&knowledge.contains("signed changes")).is_false();
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

        let knowledge = help_output(&["dispatch", "knowledge", "node", "show", "--help"]);
        assert_that!(&(knowledge.contains("Document ID or knowledge-directory-relative path")))
            .is_true();
        assert_that!(&(knowledge.contains("--body-offset"))).is_true();

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

        let (_, _, command) = cli.into_parts();
        match command {
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
    fn json_output_is_available_before_or_after_subcommands() {
        for arguments in [
            ["dispatch", "--json", "project", "list"],
            ["dispatch", "project", "list", "--json"],
        ] {
            let cli = Cli::parse_from(arguments);

            let (_, format, command) = cli.into_parts();
            match command {
                Command::Project {
                    command: ProjectCommand::List,
                } => {
                    assert_that!(&(format)).is_equal_to(output::Format::Json);
                }
                command => panic!("expected project list command, got {command:?}"),
            }
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

        let (_, _, command) = cli.into_parts();
        match command {
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
