use clap::Args;

#[derive(Debug, Args)]
pub(crate) struct ItemIdArgs {
    /// Item id; defaults to the claimed item when available.
    pub(crate) item_id: Option<i64>,
}
