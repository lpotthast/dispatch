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
    let command = match command {
        KnowledgeCommand::Job { command } => return job(command, context, format).await,
        KnowledgeCommand::Source { command } => return source(command, context, format).await,
        other => other,
    };
    let (operation, mut query, page) = match command {
        KnowledgeCommand::Job { .. } | KnowledgeCommand::Source { .. } => unreachable!(),
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

use crate::commands::{KnowledgeJobCommand, KnowledgeSourceCommand};
use dispatch_types::knowledge::jobs::*;
fn print_json<T: serde::Serialize>(format: output::Format, result: &T) -> Result<()> {
    output::write(format, result, |out| {
        writeln!(out, "{}", serde_json::to_string_pretty(result)?)?;
        Ok(())
    })
}
fn read_json<T: serde::de::DeserializeOwned>(path: &str) -> Result<T> {
    use std::io::Read;
    let mut text = String::new();
    if path == "-" {
        std::io::stdin()
            .take(2 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
    } else {
        text = std::fs::read_to_string(path)?;
    }
    Ok(serde_json::from_str(&text)?)
}
async fn source(
    command: KnowledgeSourceCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let (operation, query) = match command {
        KnowledgeSourceCommand::List {
            path,
            offset,
            limit,
        } => (
            "list",
            SourceQuery {
                path,
                offset,
                limit: Some(limit),
                ..Default::default()
            },
        ),
        KnowledgeSourceCommand::Search {
            text,
            path,
            offset,
            limit,
        } => (
            "search",
            SourceQuery {
                text: Some(text),
                path,
                offset,
                limit: Some(limit),
                ..Default::default()
            },
        ),
        KnowledgeSourceCommand::Read { path, start, end } => (
            "read",
            SourceQuery {
                path: Some(path),
                start: Some(start),
                end,
                ..Default::default()
            },
        ),
    };
    print_json(
        format,
        &context
            .project_client()?
            .knowledge_source(context.knowledge_job_id()?, operation, &query)
            .await?,
    )
}
async fn job(
    command: KnowledgeJobCommand,
    context: ResolvedContext,
    format: output::Format,
) -> Result<()> {
    let client = context.project_client()?;
    match command {
        KnowledgeJobCommand::Progress { body, area, aspect } => {
            client
                .knowledge_job_progress(
                    context.knowledge_job_id()?,
                    &JobProgress { body, area, aspect },
                )
                .await?;
            print_json(format, &"Progress recorded")
        }
        KnowledgeJobCommand::Report { file } => {
            client
                .knowledge_job_report(context.knowledge_job_id()?, &read_json(&file)?)
                .await?;
            print_json(format, &"Report recorded")
        }
        KnowledgeJobCommand::Assess { file } => {
            client
                .knowledge_job_assess(context.knowledge_job_id()?, &read_json(&file)?)
                .await?;
            print_json(format, &"Assessment recorded")
        }
        KnowledgeJobCommand::List => print_json(format, &client.knowledge_jobs().await?),
        KnowledgeJobCommand::Inspect { id } => print_json(format, &client.knowledge_job(id).await?),
        KnowledgeJobCommand::Coverage { id, aspect } => print_json(
            format,
            &client
                .knowledge_coverage(
                    match id {
                        Some(id) => id,
                        None => context.knowledge_job_id()?,
                    },
                    aspect.as_deref(),
                )
                .await?,
        ),
        KnowledgeJobCommand::Start {
            request_id,
            previous_job,
            context,
            budget_seconds,
            token_budget,
        } => print_json(
            format,
            &client
                .start_knowledge_job(&StartKnowledgeJob {
                    request_id,
                    previous_job_id: previous_job,
                    context,
                    budget_seconds,
                    token_budget,
                    ..Default::default()
                })
                .await?,
        ),
        other => {
            let (id, action) = match other {
                KnowledgeJobCommand::Continue { id, request_id } => {
                    (id, JobAction::Continue { request_id })
                }
                KnowledgeJobCommand::Retry { id, request_id } => {
                    (id, JobAction::Retry { request_id })
                }
                KnowledgeJobCommand::Cancel { id } => (id, JobAction::Cancel),
                KnowledgeJobCommand::Apply { id } => (id, JobAction::Apply),
                KnowledgeJobCommand::Reject { id } => (id, JobAction::Reject),
                _ => unreachable!(),
            };
            print_json(format, &client.knowledge_job_action(id, &action).await?)
        }
    }
}
