use dispatch_types::RoutingExplainRequest;
use rootcause::Result;

use crate::{
    commands::{AutomationCommand, AutomationRoutingCommand, AutomationTriggersCommand},
    context::ResolvedContext,
    output, render,
};

pub(super) async fn run(
    command: AutomationCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        AutomationCommand::Runs(args) => {
            let runs = client.list_runs(args.limit).await?;
            output::write(format, &runs, |output| {
                render::write_automation_runs(output, &runs)
            })
        }
        AutomationCommand::Log(args) => {
            let log = client.read_run_log(args.run_id).await?;
            output::write(format, &log, |output| render::write_run_log(output, &log))
        }
        AutomationCommand::Triggers { command } => match command {
            AutomationTriggersCommand::List => {
                let triggers = client.list_automation_triggers().await?;
                output::write(format, &triggers, |output| {
                    for trigger in &triggers {
                        writeln!(
                            output,
                            "#{} {} [{} / {}]{}",
                            trigger.id,
                            trigger.name,
                            trigger.activation,
                            trigger.effect,
                            if trigger.exclusive { " exclusive" } else { "" }
                        )?;
                    }
                    Ok(())
                })
            }
            AutomationTriggersCommand::Show(args) => {
                let trigger = client.get_automation_trigger(&args.id_or_key).await?;
                output::write(format, &trigger, |output| {
                    writeln!(output, "#{} {}", trigger.id, trigger.name)?;
                    writeln!(output, "Activation: {}", trigger.activation)?;
                    writeln!(output, "Effect: {}", trigger.effect)?;
                    writeln!(output, "Schedule: {}", trigger.schedule)?;
                    writeln!(output, "Exclusive: {}", trigger.exclusive)
                })
            }
        },
        AutomationCommand::Routing { command } => match command {
            AutomationRoutingCommand::Explain(args) => {
                let explanation = client
                    .explain_automation_routing(&RoutingExplainRequest {
                        item_id: Some(context.item_id(args.item_id)?),
                        rule: None,
                    })
                    .await?;
                output::write(format, &explanation, |output| {
                    writeln!(
                        output,
                        "Winner: {}",
                        explanation
                            .winner_trigger_id
                            .map(|id| format!("#{id}"))
                            .unwrap_or_else(|| "none".to_owned())
                    )?;
                    for rule in &explanation.rules {
                        writeln!(
                            output,
                            "{}: match={} due={} admission={} score={}{}{}",
                            rule.trigger_name,
                            rule.selector_matches,
                            rule.due,
                            rule.admission_allowed,
                            rule.fairness_score,
                            if rule.exclusive { " exclusive" } else { "" },
                            if rule.would_win { " winner" } else { "" }
                        )?;
                        for blocker in &rule.blockers {
                            writeln!(output, "  blocked: {blocker}")?;
                        }
                    }
                    Ok(())
                })
            }
        },
    }
}
