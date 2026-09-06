use clap::{Args, Subcommand};
use dispatch_types::AgentReasoningEffort;

use super::ItemIdArgs;

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

#[derive(Debug, Args)]
pub(crate) struct ItemListArgs {
    /// Filter items by state label value.
    #[arg(long)]
    pub(crate) state: Option<String>,
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
}

#[derive(Debug, Args)]
pub(crate) struct ItemClaimArgs {
    /// State label to claim from.
    #[arg(long, default_value = "open")]
    pub(crate) state: String,
}

#[derive(Debug, Args)]
pub(crate) struct ItemProgressArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Progress text to record.
    #[arg(long)]
    pub(crate) body: String,
}

#[derive(Debug, Args)]
pub(crate) struct ItemFinishArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Final report text.
    #[arg(long)]
    pub(crate) report: String,
}

#[derive(Debug, Args)]
pub(crate) struct ItemReleaseArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Optional release note.
    #[arg(long)]
    pub(crate) comment: Option<String>,
}

#[derive(Debug, Args)]
pub(crate) struct ItemRequestFeedbackArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Feedback request to show the user.
    #[arg(long)]
    pub(crate) body: String,
}

#[derive(Debug, Args)]
pub(crate) struct ItemWatchArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,

    /// Only print versions newer than this value.
    #[arg(long)]
    pub(crate) since_version: Option<i64>,
}
