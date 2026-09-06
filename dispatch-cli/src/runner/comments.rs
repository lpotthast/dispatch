use dispatch_types::AddCommentRequest;
use rootcause::Result;

use crate::{commands::CommentCommand, context::ResolvedContext, output, render};

pub(super) async fn run(
    command: CommentCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        CommentCommand::Add(args) => {
            let item_id = context.item_id(args.item_id)?;
            let comment = client
                .add_comment(
                    item_id,
                    &AddCommentRequest {
                        author_type: args.author_type,
                        author_name: args.author,
                        body: args.body,
                    },
                )
                .await?;
            output::write(format, &comment, |output| {
                writeln!(
                    output,
                    "Added comment #{} to item #{}",
                    comment.id, comment.work_item_id
                )
            })
        }
        CommentCommand::List(args) => {
            let comments = client.list_comments(context.item_id(args.item_id)?).await?;
            output::write(format, &comments, |output| {
                render::write_comments(output, &comments)
            })
        }
    }
}
