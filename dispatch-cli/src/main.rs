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
    let (overrides, format, command) = Cli::parse().into_parts();
    let context = resolve_context(overrides, |key| std::env::var(key))
        .context("failed to resolve Dispatch context")?;
    runner::run(command, context, format).await
}
