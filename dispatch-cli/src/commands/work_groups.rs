use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum GroupCommand {
    /// List project work-item groups.
    List,
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
}

#[derive(Debug, Args)]
pub(crate) struct GroupAssignArgs {
    /// Stable key of the target group.
    #[arg(long)]
    pub(crate) key: String,
    /// Item id to assign; may be repeated.
    #[arg(long = "item", required = true)]
    pub(crate) item_ids: Vec<i64>,
}
