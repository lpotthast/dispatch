use dispatch_types::{AssignWorkItemGroupRequest, CreateWorkItemGroupRequest};
use rootcause::Result;

use crate::{commands::GroupCommand, context::ResolvedContext, output};

pub(super) async fn run(
    command: GroupCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        GroupCommand::List => {
            let groups = client.list_work_item_groups().await?;
            output::write(format, &groups, |output| {
                for group in &groups {
                    writeln!(
                        output,
                        "{} ({}) - {} item{}",
                        group.name,
                        group.key,
                        group.item_count,
                        if group.item_count == 1 { "" } else { "s" }
                    )?;
                }
                Ok(())
            })
        }
        GroupCommand::Create(args) => {
            let group = client
                .create_work_item_group(&CreateWorkItemGroupRequest {
                    key: args.key,
                    name: args.name,
                })
                .await?;
            output::write(format, &group, |output| {
                writeln!(output, "Created work group {} ({})", group.name, group.key)
            })
        }
        GroupCommand::Assign(args) => {
            let assigned = args.item_ids.len();
            let group = client
                .assign_work_item_group_items(
                    &args.key,
                    &AssignWorkItemGroupRequest {
                        item_ids: args.item_ids,
                    },
                )
                .await?;
            output::write(format, &group, |output| {
                writeln!(
                    output,
                    "Assigned {assigned} item{} to {} ({})",
                    if assigned == 1 { "" } else { "s" },
                    group.name,
                    group.key
                )
            })
        }
    }
}
