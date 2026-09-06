use clap::{Args, Subcommand};
use dispatch_types::AuthorType;

use super::ItemIdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum CommentCommand {
    /// Add a comment to an item.
    Add(CommentAddArgs),
    /// List comments on an item.
    List(ItemIdArgs),
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
}
