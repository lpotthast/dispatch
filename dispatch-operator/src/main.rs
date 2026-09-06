use std::{fs, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use dispatch_api_client::{DispatchClient, OperatorProjectClient, ProjectClient};
use dispatch_types::{
    AutomationPersonalityInput, AutomationRuleInput, BundleYamlRequest,
    RemoveAutomationBundleRequest, RoutingExplainRequest,
};
use rootcause::{Result, option_ext::OptionExt, prelude::*};
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
    /// Project used by project-scoped operator commands.
    #[arg(long, env = "DISPATCH_PROJECT", global = true)]
    project: Option<String>,
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
    List,
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
    List,
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
    List,
    Validate(FileArgs),
    Diff(FileArgs),
    Apply(BundleApplyArgs),
    Export(BundleExportArgs),
    Remove(BundleRemoveArgs),
}

#[derive(Args)]
struct IdOrKeyArgs {
    id_or_key: String,
}

#[derive(Args)]
struct IdArgs {
    id: i64,
}

#[derive(Args)]
struct FileArgs {
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
struct IdFileArgs {
    id: i64,
    #[arg(long)]
    file: PathBuf,
}

#[derive(Args)]
struct RestoreArgs {
    id: i64,
    revision_id: i64,
}

#[derive(Args)]
struct AnalyticsArgs {
    revision_id: i64,
}

#[derive(Args)]
struct EvaluationsArgs {
    #[arg(long)]
    trigger_id: Option<i64>,
    #[arg(long)]
    limit: Option<u64>,
}

#[derive(Args)]
struct RouteExplainArgs {
    item_id: Option<i64>,
    #[arg(long)]
    rule_file: Option<PathBuf>,
}

#[derive(Args)]
struct BundleApplyArgs {
    #[arg(long)]
    file: PathBuf,
    #[arg(long)]
    yes: bool,
}

#[derive(Args)]
struct BundleExportArgs {
    #[arg(long)]
    bundle: String,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Args)]
struct BundleRemoveArgs {
    #[arg(long)]
    bundle: String,
    /// Confirm deletion of all objects managed by the bundle.
    #[arg(long)]
    yes: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let Cli {
        api_url,
        project,
        command,
    } = Cli::parse();
    let client = DispatchClient::new(api_url)?;
    match command {
        Command::Automation { command } => {
            run_automation(&client, project.as_deref(), command).await
        }
    }
}

async fn run_automation(
    client: &DispatchClient,
    project: Option<&str>,
    command: AutomationCommand,
) -> Result<()> {
    match command {
        AutomationCommand::Rule { command } => {
            run_rule(operator_project(client, project)?, command).await
        }
        AutomationCommand::Personality { command } => {
            run_personality(operator_project(client, project)?, command).await
        }
        AutomationCommand::Route { command } => {
            run_route(agent_project(client, project)?, command).await
        }
        AutomationCommand::Bundle { command } => run_bundle(client, project, command).await,
    }
}

async fn run_rule(client: OperatorProjectClient<'_>, command: RuleCommand) -> Result<()> {
    match command {
        RuleCommand::List => print_json(&client.operator_list_rules().await?),
        RuleCommand::Show(args) => print_json(&client.operator_get_rule(&args.id_or_key).await?),
        RuleCommand::Create(args) => {
            let input = read_yaml::<AutomationRuleInput>(&args.file)?;
            print_json(&client.operator_create_rule(&input).await?)
        }
        RuleCommand::Update(args) => {
            let input = read_yaml::<AutomationRuleInput>(&args.file)?;
            print_json(&client.operator_update_rule(args.id, &input).await?)
        }
        RuleCommand::Delete(args) => {
            client.operator_delete_rule(args.id).await?;
            println!("Deleted automation rule {}", args.id);
            Ok(())
        }
        RuleCommand::Schedule(args) => print_json(&client.operator_schedule_rule(args.id).await?),
        RuleCommand::History(args) => print_json(&client.list_automation_revisions(args.id).await?),
        RuleCommand::Restore(args) => print_json(
            &client
                .operator_restore_rule(args.id, args.revision_id)
                .await?,
        ),
        RuleCommand::Detach(args) => print_json(&client.operator_detach_rule(args.id).await?),
        RuleCommand::Analytics(args) => {
            print_json(&client.operator_revision_analytics(args.revision_id).await?)
        }
        RuleCommand::Evaluations(args) => print_json(
            &client
                .operator_list_evaluations(args.trigger_id, args.limit)
                .await?,
        ),
    }
}

async fn run_personality(
    client: OperatorProjectClient<'_>,
    command: PersonalityCommand,
) -> Result<()> {
    match command {
        PersonalityCommand::List => print_json(&client.operator_list_personalities().await?),
        PersonalityCommand::Show(args) => {
            print_json(&client.operator_get_personality(&args.id_or_key).await?)
        }
        PersonalityCommand::Create(args) => {
            let input = read_yaml::<AutomationPersonalityInput>(&args.file)?;
            print_json(&client.operator_create_personality(&input).await?)
        }
        PersonalityCommand::Update(args) => {
            let input = read_yaml::<AutomationPersonalityInput>(&args.file)?;
            print_json(&client.operator_update_personality(args.id, &input).await?)
        }
        PersonalityCommand::Delete(args) => {
            client.operator_delete_personality(args.id).await?;
            println!("Deleted automation personality {}", args.id);
            Ok(())
        }
        PersonalityCommand::History(args) => {
            print_json(&client.operator_list_personality_revisions(args.id).await?)
        }
        PersonalityCommand::Restore(args) => print_json(
            &client
                .operator_restore_personality(args.id, args.revision_id)
                .await?,
        ),
        PersonalityCommand::Detach(args) => {
            print_json(&client.operator_detach_personality(args.id).await?)
        }
    }
}

async fn run_route(client: ProjectClient<'_>, command: RouteCommand) -> Result<()> {
    match command {
        RouteCommand::Explain(args) => {
            let rule = args
                .rule_file
                .as_deref()
                .map(read_yaml::<AutomationRuleInput>)
                .transpose()?;
            print_json(
                &client
                    .explain_automation_routing(&RoutingExplainRequest {
                        item_id: args.item_id,
                        rule,
                    })
                    .await?,
            )
        }
    }
}

async fn run_bundle(
    client: &DispatchClient,
    project: Option<&str>,
    command: BundleCommand,
) -> Result<()> {
    match command {
        BundleCommand::List => print_json(
            &operator_project(client, project)?
                .list_installed_automation_bundles()
                .await?,
        ),
        BundleCommand::Validate(args) => {
            let request = bundle_request(&args.file, None)?;
            print_json(&client.validate_automation_bundle(&request).await?)
        }
        BundleCommand::Diff(args) => {
            let request = bundle_request(&args.file, None)?;
            print_json(
                &operator_project(client, project)?
                    .diff_automation_bundle(&request)
                    .await?,
            )
        }
        BundleCommand::Apply(args) => {
            let mut request = bundle_request(&args.file, None)?;
            let client = operator_project(client, project)?;
            let diff = client.diff_automation_bundle(&request).await?;
            print_json(&diff)?;
            if diff.has_deletions && !args.yes {
                bail!("bundle diff deletes managed objects; rerun with --yes to apply");
            }
            request.expected_current_hash = diff.current_hash;
            print_json(&client.apply_automation_bundle(&request).await?)
        }
        BundleCommand::Export(args) => {
            let export = operator_project(client, project)?
                .export_automation_bundle(&args.bundle)
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
            let client = operator_project(client, project)?;
            let installed = client.list_installed_automation_bundles().await?;
            let bundle = installed
                .iter()
                .find(|bundle| bundle.bundle_key == args.bundle)
                .ok_or_else(|| report!("bundle '{}' is not installed", args.bundle))?;
            print_json(
                &client
                    .remove_automation_bundle(
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

fn agent_project<'a>(
    client: &'a DispatchClient,
    project: Option<&'a str>,
) -> Result<ProjectClient<'a>> {
    Ok(client.project(required_project(project)?))
}

fn operator_project<'a>(
    client: &'a DispatchClient,
    project: Option<&'a str>,
) -> Result<OperatorProjectClient<'a>> {
    Ok(client.operator_project(required_project(project)?))
}

fn required_project(project: Option<&str>) -> Result<&str> {
    Ok(project.context("missing Dispatch project; pass --project or set DISPATCH_PROJECT")?)
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

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use clap::Parser;

    use super::*;

    #[test]
    fn project_option_is_global_across_operator_commands() {
        let cli = Cli::try_parse_from([
            "dispatch-operator",
            "automation",
            "rule",
            "list",
            "--project",
            "demo",
        ])
        .unwrap();

        assert_that!(&(cli.project.as_deref())).is_equal_to(Some("demo"));
        assert_that!(&matches!(
            cli.command,
            Command::Automation {
                command: AutomationCommand::Rule {
                    command: RuleCommand::List
                }
            }
        ))
        .is_true();
    }

    #[test]
    fn bundle_validation_remains_server_scoped() {
        let cli = Cli::try_parse_from([
            "dispatch-operator",
            "automation",
            "bundle",
            "validate",
            "--file",
            "bundle.yaml",
        ])
        .unwrap();

        assert_that!(&(cli.project.is_none())).is_true();
        assert_that!(&matches!(
            cli.command,
            Command::Automation {
                command: AutomationCommand::Bundle {
                    command: BundleCommand::Validate(_)
                }
            }
        ))
        .is_true();
    }

    #[test]
    fn project_scoped_commands_reject_missing_context() {
        let error = required_project(None).unwrap_err();

        assert_that!(&(error.to_string()))
            .contains("missing Dispatch project; pass --project or set DISPATCH_PROJECT");
    }
}
