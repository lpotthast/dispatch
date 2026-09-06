use crate::{
    commands::{KnowledgeCommand, KnowledgeNodeCommand},
    context::ResolvedContext,
    output,
};
use dispatch_types::knowledge::{KnowledgeOperation, KnowledgeQuery};
use rootcause::Result;

pub(super) async fn run(
    command: KnowledgeCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let (operation, mut query, page) = match command {
        KnowledgeCommand::Root(page) => (KnowledgeOperation::Root, KnowledgeQuery::default(), page),
        KnowledgeCommand::Search(args) => (
            KnowledgeOperation::Search,
            KnowledgeQuery {
                text: Some(args.text),
                ..Default::default()
            },
            args.page,
        ),
        KnowledgeCommand::Node {
            command: KnowledgeNodeCommand::Show(args),
        } => (
            KnowledgeOperation::Node,
            KnowledgeQuery {
                selector: args.selector,
                id: args.id,
                path: args.path,
                ..Default::default()
            },
            args.page,
        ),
        KnowledgeCommand::Check(args) => (
            KnowledgeOperation::Check,
            KnowledgeQuery {
                selector: args.selector,
                id: args.id,
                path: args.path,
                ..Default::default()
            },
            args.page,
        ),
    };
    query.offset = page.offset;
    query.limit = Some(usize::from(page.limit));
    query.body_offset = page.body_offset;
    let result = context
        .project_client()?
        .knowledge_query(operation, &query)
        .await?;
    output::write(format, &result, |out| {
        writeln!(out, "Working copy: {}", result.working_directory)?;
        if let Some(document) = &result.document {
            writeln!(out, "{}", document.markdown)?;
            if let Some(offset) = document.next_body_offset {
                writeln!(out, "Continue with --body-offset {offset}")?;
            }
        }
        for doc in &result.documents {
            writeln!(out, "{} [{}]\n  {}", doc.title, doc.path, doc.summary)?;
        }
        if let Some(offset) = result.next_offset {
            writeln!(out, "More results: --offset {offset}")?;
        }
        for diagnostic in &result.diagnostics {
            writeln!(
                out,
                "{}: {}: {}",
                diagnostic.path, diagnostic.code, diagnostic.message
            )?;
        }
        Ok(())
    })
}
