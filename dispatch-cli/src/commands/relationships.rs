use clap::{Args, Subcommand};

use super::ItemIdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum RelationshipCommand {
    /// List relationships touching an item.
    List(ItemIdArgs),
    /// Create a relationship from an item to a target item.
    Add(RelationshipAddArgs),
    /// Update a relationship kind.
    Update(RelationshipUpdateArgs),
    /// Delete a relationship.
    Delete(RelationshipDeleteArgs),
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
}

#[derive(Debug, Args)]
pub(crate) struct RelationshipUpdateArgs {
    /// Relationship id to update.
    pub(crate) relationship_id: i64,

    /// Replacement free-form relationship kind.
    #[arg(long)]
    pub(crate) kind: String,
}

#[derive(Debug, Args)]
pub(crate) struct RelationshipDeleteArgs {
    /// Relationship id to delete.
    pub(crate) relationship_id: i64,
}
