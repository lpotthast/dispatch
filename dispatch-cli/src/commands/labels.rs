use clap::{Args, Subcommand};

use super::ItemIdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum LabelCommand {
    /// List labels on an item.
    List(ItemIdArgs),
    /// Add a label to an item.
    Add(LabelAddArgs),
    /// Update a label on an item.
    Update(LabelUpdateArgs),
    /// Delete a label from an item.
    Delete(LabelDeleteArgs),
    /// List labels already used in this project.
    Suggestions,
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
}
