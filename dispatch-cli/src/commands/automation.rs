use clap::{Args, Subcommand};

use super::ItemIdArgs;

#[derive(Debug, Subcommand)]
pub(crate) enum AutomationCommand {
    /// List automation runs.
    Runs(AutomationRunsArgs),
    /// Show one automation run log.
    Log(AutomationRunLogArgs),
    /// Inspect configured automation triggers.
    Triggers {
        #[command(subcommand)]
        command: AutomationTriggersCommand,
    },
    /// Explain current automation routing for an item.
    Routing {
        #[command(subcommand)]
        command: AutomationRoutingCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AutomationTriggersCommand {
    /// List automation triggers.
    List,
    /// Show one automation trigger by id or managed key.
    Show(AutomationTriggerShowArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum AutomationRoutingCommand {
    /// Explain matching, exclusivity, fairness, and admission blockers.
    Explain(ItemIdArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AutomationTriggerShowArgs {
    /// Automation trigger id, managed object key, or name.
    pub(crate) id_or_key: String,
}

#[derive(Debug, Args)]
pub(crate) struct AutomationRunsArgs {
    /// Maximum number of runs to show.
    #[arg(long)]
    pub(crate) limit: Option<u64>,
}

#[derive(Debug, Args)]
pub(crate) struct AutomationRunLogArgs {
    /// Automation run id.
    pub(crate) run_id: i64,
}
