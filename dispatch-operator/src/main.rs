use std::{fs, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use dispatch_api_client::DispatchClient;
use dispatch_types::{
    AutomationPersonalityInput, AutomationRuleInput, BundleYamlRequest,
    RemoveAutomationBundleRequest, RoutingExplainRequest,
};
use rootcause::{Result, prelude::*};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "dispatch-operator", about = "Dispatch operator HTTP client")]
struct Cli {
    #[arg(
        long,
        env = "DISPATCH_API_URL",
        default_value = "http://127.0.0.1:4000"
    )]
    api_url: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Automation {
        #[command(subcommand)]
        command: AutomationCommand,
    },
}

#[derive(Subcommand)]
enum AutomationCommand {
    Rule {
        #[command(subcommand)]
        command: RuleCommand,
    },
    Personality {
        #[command(subcommand)]
        command: PersonalityCommand,
    },
    Route {
        #[command(subcommand)]
        command: RouteCommand,
    },
    Bundle {
        #[command(subcommand)]
        command: BundleCommand,
    },
}

#[derive(Subcommand)]
enum RuleCommand {
    List(ProjectArgs),
    Show(IdOrKeyArgs),
    Create(FileArgs),
    Update(IdFileArgs),
    Delete(IdArgs),
    Schedule(IdArgs),
    History(IdArgs),
    Restore(RestoreArgs),
    Detach(IdArgs),
    Analytics(AnalyticsArgs),
    Evaluations(EvaluationsArgs),
}

#[derive(Subcommand)]
enum PersonalityCommand {
    List(ProjectArgs),
    Show(IdOrKeyArgs),
    Create(FileArgs),
    Update(IdFileArgs),
    Delete(IdArgs),
    History(IdArgs),
    Restore(RestoreArgs),
    Detach(IdArgs),
}

#[derive(Subcommand)]
enum RouteCommand {
    Explain(RouteExplainArgs),
}

#[derive(Subcommand)]
enum BundleCommand {
    List(ProjectArgs),
    Validate(BundleValidateArgs),
    Diff(BundleProjectFileArgs),
    Apply(BundleApplyArgs),
    Export(BundleExportArgs),
    Remove(BundleRemoveArgs),
}

#[derive(Args)]
struct ProjectArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
}

#[derive(Args)]
struct IdOrKeyArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    id_or_key: String,
}

#[derive(Args)]
struct IdArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    id: i64,
}

#[derive(Args)]
struct FileArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
struct IdFileArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    id: i64,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
struct RestoreArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    id: i64,
    revision_id: i64,
}

#[derive(Args)]
struct AnalyticsArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    revision_id: i64,
}

#[derive(Args)]
struct EvaluationsArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    #[arg(long)]
    trigger_id: Option<i64>,
    #[arg(long)]
    limit: Option<u64>,
}

#[derive(Args)]
struct RouteExplainArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    item_id: Option<i64>,
    #[arg(long)]
    rule_file: Option<PathBuf>,
}

#[derive(Args)]
struct BundleValidateArgs {
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
struct BundleProjectFileArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
struct BundleApplyArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    yes: bool,
}

#[derive(Args)]
struct BundleExportArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    #[arg(long)]
    bundle: String,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Args)]
struct BundleRemoveArgs {
    #[arg(long, env = "DISPATCH_PROJECT")]
    project: String,
    #[arg(long)]
    bundle: String,
    /// Confirm deletion of all objects managed by the bundle.
    #[arg(long)]
    yes: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = DispatchClient::new(cli.api_url);
    match cli.command {
        Command::Automation { command } => run_automation(&client, command).await,
    }
}

async fn run_automation(client: &DispatchClient, command: AutomationCommand) -> Result<()> {
    match command {
        AutomationCommand::Rule { command } => run_rule(client, command).await,
        AutomationCommand::Personality { command } => run_personality(client, command).await,
        AutomationCommand::Route { command } => run_route(client, command).await,
        AutomationCommand::Bundle { command } => run_bundle(client, command).await,
    }
}

async fn run_rule(client: &DispatchClient, command: RuleCommand) -> Result<()> {
    match command {
        RuleCommand::List(args) => print_json(&client.operator_list_rules(&args.project).await?),
        RuleCommand::Show(args) => print_json(
            &client
                .operator_get_rule(&args.project, &args.id_or_key)
                .await?,
        ),
        RuleCommand::Create(args) => {
            let input = read_yaml::<AutomationRuleInput>(&args.file)?;
            print_json(&client.operator_create_rule(&args.project, &input).await?)
        }
        RuleCommand::Update(args) => {
            let input = read_yaml::<AutomationRuleInput>(&args.file)?;
            print_json(
                &client
                    .operator_update_rule(&args.project, args.id, &input)
                    .await?,
            )
        }
        RuleCommand::Delete(args) => {
            client.operator_delete_rule(&args.project, args.id).await?;
            println!("Deleted automation rule {}", args.id);
            Ok(())
        }
        RuleCommand::Schedule(args) => print_json(
            &client
                .operator_schedule_rule(&args.project, args.id)
                .await?,
        ),
        RuleCommand::History(args) => print_json(
            &client
                .list_automation_revisions(&args.project, args.id)
                .await?,
        ),
        RuleCommand::Restore(args) => print_json(
            &client
                .operator_restore_rule(&args.project, args.id, args.revision_id)
                .await?,
        ),
        RuleCommand::Detach(args) => {
            print_json(&client.operator_detach_rule(&args.project, args.id).await?)
        }
        RuleCommand::Analytics(args) => print_json(
            &client
                .operator_revision_analytics(&args.project, args.revision_id)
                .await?,
        ),
        RuleCommand::Evaluations(args) => print_json(
            &client
                .operator_list_evaluations(&args.project, args.trigger_id, args.limit)
                .await?,
        ),
    }
}

async fn run_personality(client: &DispatchClient, command: PersonalityCommand) -> Result<()> {
    match command {
        PersonalityCommand::List(args) => {
            print_json(&client.operator_list_personalities(&args.project).await?)
        }
        PersonalityCommand::Show(args) => print_json(
            &client
                .operator_get_personality(&args.project, &args.id_or_key)
                .await?,
        ),
        PersonalityCommand::Create(args) => {
            let input = read_yaml::<AutomationPersonalityInput>(&args.file)?;
            print_json(
                &client
                    .operator_create_personality(&args.project, &input)
                    .await?,
            )
        }
        PersonalityCommand::Update(args) => {
            let input = read_yaml::<AutomationPersonalityInput>(&args.file)?;
            print_json(
                &client
                    .operator_update_personality(&args.project, args.id, &input)
                    .await?,
            )
        }
        PersonalityCommand::Delete(args) => {
            client
                .operator_delete_personality(&args.project, args.id)
                .await?;
            println!("Deleted automation personality {}", args.id);
            Ok(())
        }
        PersonalityCommand::History(args) => print_json(
            &client
                .operator_list_personality_revisions(&args.project, args.id)
                .await?,
        ),
        PersonalityCommand::Restore(args) => print_json(
            &client
                .operator_restore_personality(&args.project, args.id, args.revision_id)
                .await?,
        ),
        PersonalityCommand::Detach(args) => print_json(
            &client
                .operator_detach_personality(&args.project, args.id)
                .await?,
        ),
    }
}

async fn run_route(client: &DispatchClient, command: RouteCommand) -> Result<()> {
    match command {
        RouteCommand::Explain(args) => {
            let rule = args
                .rule_file
                .as_deref()
                .map(read_yaml::<AutomationRuleInput>)
                .transpose()?;
            print_json(
                &client
                    .explain_automation_routing(
                        &args.project,
                        &RoutingExplainRequest {
                            item_id: args.item_id,
                            rule,
                        },
                    )
                    .await?,
            )
        }
    }
}

async fn run_bundle(client: &DispatchClient, command: BundleCommand) -> Result<()> {
    match command {
        BundleCommand::List(args) => print_json(
            &client
                .list_installed_automation_bundles(&args.project)
                .await?,
        ),
        BundleCommand::Validate(args) => {
            let request = bundle_request(&args.file, None)?;
            print_json(&client.validate_automation_bundle(&request).await?)
        }
        BundleCommand::Diff(args) => {
            let request = bundle_request(&args.file, None)?;
            print_json(
                &client
                    .diff_automation_bundle(&args.project, &request)
                    .await?,
            )
        }
        BundleCommand::Apply(args) => {
            let mut request = bundle_request(&args.file, None)?;
            let diff = client
                .diff_automation_bundle(&args.project, &request)
                .await?;
            print_json(&diff)?;
            if diff.has_deletions && !args.yes {
                bail!("bundle diff deletes managed objects; rerun with --yes to apply");
            }
            request.expected_current_hash = diff.current_hash;
            print_json(
                &client
                    .apply_automation_bundle(&args.project, &request)
                    .await?,
            )
        }
        BundleCommand::Export(args) => {
            let export = client
                .export_automation_bundle(&args.project, &args.bundle)
                .await?;
            fs::write(&args.output, export.yaml)
                .context_with(|| format!("failed to write {}", args.output.display()))?;
            println!(
                "Exported bundle '{}' to {}",
                args.bundle,
                args.output.display()
            );
            Ok(())
        }
        BundleCommand::Remove(args) => {
            if !args.yes {
                bail!("bundle removal deletes every managed object; rerun with --yes");
            }
            let installed = client
                .list_installed_automation_bundles(&args.project)
                .await?;
            let bundle = installed
                .iter()
                .find(|bundle| bundle.bundle_key == args.bundle)
                .ok_or_else(|| report!("bundle '{}' is not installed", args.bundle))?;
            print_json(
                &client
                    .remove_automation_bundle(
                        &args.project,
                        &args.bundle,
                        &RemoveAutomationBundleRequest {
                            expected_current_hash: Some(bundle.manifest_hash.clone()),
                        },
                    )
                    .await?,
            )
        }
    }
}

fn read_yaml<T>(path: &std::path::Path) -> Result<T>
where
    T: serde::de::DeserializeOwned,
{
    let text =
        fs::read_to_string(path).context_with(|| format!("failed to read {}", path.display()))?;
    yaml_serde::from_str(&text)
        .map_err(|error| report!("invalid YAML in {}: {error}", path.display()))
}

fn bundle_request(
    path: &std::path::Path,
    expected_current_hash: Option<String>,
) -> Result<BundleYamlRequest> {
    Ok(BundleYamlRequest {
        yaml: fs::read_to_string(path)
            .context_with(|| format!("failed to read {}", path.display()))?,
        expected_current_hash,
    })
}

fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
