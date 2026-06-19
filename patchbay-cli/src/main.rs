mod commands;
mod context;
mod git_guard;
mod output;
mod render;
mod runner;

use clap::Parser;
use commands::Cli;
use context::resolve_context;
use rootcause::{Result, prelude::*};

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let context = resolve_context(&cli.context_overrides(), |key| std::env::var(key))
        .context("failed to resolve Patchbay context")?;
    runner::run(cli.into_command(), context).await
}
