use dispatch_types::{CreateWorkItemLabelRequest, UpdateWorkItemLabelRequest};
use rootcause::Result;

use crate::{commands::LabelCommand, context::ResolvedContext, output, render};

use super::items::optional_override;

pub(super) async fn run(
    command: LabelCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        LabelCommand::List(args) => {
            let item_id = context.item_id(args.item_id)?;
            let labels = client.list_item_labels(item_id).await?;
            output::write(format, &labels, |output| {
                render::write_item_labels(output, &labels)
            })
        }
        LabelCommand::Add(args) => {
            let item_id = context.item_id(args.item_id)?;
            let item = client
                .add_item_label(
                    item_id,
                    &CreateWorkItemLabelRequest {
                        key: args.key,
                        value: args.value,
                    },
                    args.expect_version,
                )
                .await?;
            output::write(format, &item, |output| {
                writeln!(output, "Added label on item #{} v{}", item.id, item.version)
            })
        }
        LabelCommand::Update(args) => {
            let item_id = context.item_id(args.item_id)?;
            let request = UpdateWorkItemLabelRequest {
                key: args.key,
                value: optional_override(args.value, args.clear_value),
                expect_version: args.expect_version,
            };
            let item = client
                .update_item_label(item_id, args.label_id, &request)
                .await?;
            output::write(format, &item, |output| {
                writeln!(
                    output,
                    "Updated label #{} on item #{} v{}",
                    args.label_id, item.id, item.version
                )
            })
        }
        LabelCommand::Delete(args) => {
            let item_id = context.item_id(args.item_id)?;
            let deleted = client
                .delete_item_label(item_id, args.label_id, args.expect_version)
                .await?;
            output::write(format, &deleted, |output| {
                writeln!(output, "Deleted label #{}", deleted.label_id)
            })
        }
        LabelCommand::Suggestions => {
            let labels = client.list_project_labels().await?;
            output::write(format, &labels, |output| {
                render::write_project_label_suggestions(output, &labels)
            })
        }
    }
}
