use dispatch_types::{CreateWorkItemRelationshipRequest, UpdateWorkItemRelationshipRequest};
use rootcause::Result;

use crate::{commands::RelationshipCommand, context::ResolvedContext, output, render};

pub(super) async fn run(
    command: RelationshipCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        RelationshipCommand::List(args) => {
            let item_id = context.item_id(args.item_id)?;
            let relationships = client.list_item_relationships(item_id).await?;
            output::write(format, &relationships, |output| {
                render::write_relationship_rows(output, &relationships)
            })
        }
        RelationshipCommand::Add(args) => {
            let item_id = context.item_id(args.item_id)?;
            let relationship = client
                .create_item_relationship(
                    item_id,
                    &CreateWorkItemRelationshipRequest {
                        target_work_item_id: args.target,
                        kind: args.kind,
                    },
                )
                .await?;
            output::write(format, &relationship, |output| {
                writeln!(
                    output,
                    "Created relationship #{}: #{} {} #{}",
                    relationship.relationship.id,
                    relationship.relationship.source_work_item_id,
                    relationship.relationship.kind,
                    relationship.relationship.target_work_item_id
                )
            })
        }
        RelationshipCommand::Update(args) => {
            let relationship = client
                .update_relationship(
                    args.relationship_id,
                    &UpdateWorkItemRelationshipRequest { kind: args.kind },
                )
                .await?;
            output::write(format, &relationship, |output| {
                render::write_relationship_view(output, &relationship, "Updated")
            })
        }
        RelationshipCommand::Delete(args) => {
            let deleted = client.delete_relationship(args.relationship_id).await?;
            output::write(format, &deleted, |output| {
                render::write_relationship_view(output, &deleted.relationship, "Deleted")
            })
        }
    }
}
