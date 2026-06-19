use std::time::Duration;

use patchbay_types::{
    AddCommentRequest, ClaimWorkItemRequest, CreateWorkItemLabelRequest,
    CreateWorkItemRelationshipRequest, CreateWorkItemRequest, FinishWorkItemRequest,
    ProgressWorkItemRequest, ReleaseWorkItemRequest, RequestFeedbackWorkItemRequest,
    UpdateProjectMemoryRequest, UpdateWorkItemLabelRequest, UpdateWorkItemRelationshipRequest,
    UpdateWorkItemRequest,
};
use rootcause::Result;

use crate::{
    commands::{
        AutomationCommand, Command, CommentCommand, ItemCommand, LabelCommand, MemoryCommand,
        RelationshipCommand,
    },
    context::ResolvedContext,
    git_guard::run_git,
    output, render,
};

pub(crate) async fn run(command: Command, context: ResolvedContext) -> Result<()> {
    match command {
        Command::Item { command } => run_item(command, context).await,
        Command::Comment { command } => run_comment(command, context).await,
        Command::Label { command } => run_label(command, context).await,
        Command::Relationship { command } => run_relationship(command, context).await,
        Command::Memory { command } => run_memory(command, context).await,
        Command::Automation { command } => run_automation(command, context).await,
        Command::Git(args) => run_git(args.args),
    }
}

async fn run_item(command: ItemCommand, context: ResolvedContext) -> Result<()> {
    let client = context.client();
    let project = context.project()?;
    match command {
        ItemCommand::List(args) => {
            let items = client.list_items(project, args.state.as_deref()).await?;
            output::write(args.json, &items, |output| {
                render::write_item_rows(output, &items)
            })
        }
        ItemCommand::Show(args) => {
            let item = client
                .get_item(project, context.item_id(args.item_id)?)
                .await?;
            output::write(args.json, &item, |output| {
                render::write_item_detail(output, &item)
            })
        }
        ItemCommand::Create(args) => {
            let item = client
                .create_item(
                    project,
                    &CreateWorkItemRequest {
                        title: args.title,
                        description: args.description,
                        state: args.state,
                        agent_model_override: args.agent_model,
                        agent_reasoning_effort_override: args.agent_reasoning_effort,
                        initial_labels: Vec::new(),
                    },
                )
                .await?;
            output::write(args.json, &item, |output| {
                writeln!(output, "Created item #{}: {}", item.id, item.title)
            })
        }
        ItemCommand::Update(args) => {
            let item_id = context.item_id(args.item_id)?;
            let request = UpdateWorkItemRequest {
                title: args.title,
                description: args.description,
                state: args.state,
                agent_model_override: optional_override(args.agent_model, args.clear_agent_model),
                agent_reasoning_effort_override: optional_override(
                    args.agent_reasoning_effort,
                    args.clear_agent_reasoning_effort,
                ),
                expect_version: args.expect_version,
            };
            let item = client.update_item(project, item_id, &request).await?;
            output::write(args.json, &item, |output| {
                writeln!(output, "Updated item #{} v{}", item.id, item.version)
            })
        }
        ItemCommand::Claim(args) => {
            let agent_id = context.agent_id()?;
            let claimed = client
                .claim_item(
                    project,
                    &ClaimWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        state: args.state.clone(),
                    },
                )
                .await?;
            if let Some(item) = claimed.item {
                output::write(args.json, &item, |output| {
                    writeln!(output, "Claimed item #{} for {}", item.id, agent_id)
                })
            } else if args.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "claimed": false,
                        "project": project,
                        "state": args.state,
                    }))?
                );
                Ok(())
            } else {
                println!("No matching item available");
                Ok(())
            }
        }
        ItemCommand::Progress(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let comment = client
                .progress_item(
                    project,
                    item_id,
                    &ProgressWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        body: args.body,
                    },
                )
                .await?;
            output::write(args.json, &comment, |output| {
                writeln!(output, "Recorded progress comment #{}", comment.id)
            })
        }
        ItemCommand::Finish(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let item = client
                .finish_item(
                    project,
                    item_id,
                    &FinishWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        report: args.report,
                    },
                )
                .await?;
            output::write(args.json, &item, |output| {
                writeln!(output, "Finished item #{} v{}", item.id, item.version)
            })
        }
        ItemCommand::Release(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let item = client
                .release_item(
                    project,
                    item_id,
                    &ReleaseWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        comment: args.comment,
                    },
                )
                .await?;
            output::write(args.json, &item, |output| {
                writeln!(
                    output,
                    "Released item #{} back to {}",
                    item.id,
                    render::item_state_label(&item)
                )
            })
        }
        ItemCommand::RequestFeedback(args) => {
            let item_id = context.item_id(args.item_id)?;
            let agent_id = context.agent_id()?;
            let item = client
                .request_item_feedback(
                    project,
                    item_id,
                    &RequestFeedbackWorkItemRequest {
                        agent_id: agent_id.to_owned(),
                        body: args.body,
                    },
                )
                .await?;
            output::write(args.json, &item, |output| {
                writeln!(
                    output,
                    "Requested feedback for item #{} and restored state to {}",
                    item.id,
                    render::item_state_label(&item)
                )
            })
        }
        ItemCommand::Watch(args) => {
            let item_id = context.item_id(args.item_id)?;
            let mut last_version = args.since_version.unwrap_or(0);
            loop {
                let item = client.get_item(project, item_id).await?;
                if item.version > last_version {
                    last_version = item.version;
                    output::write(args.json, &item, |output| {
                        render::write_item_row(output, &item)
                    })?;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }
}

async fn run_label(command: LabelCommand, context: ResolvedContext) -> Result<()> {
    let client = context.client();
    let project = context.project()?;
    match command {
        LabelCommand::List(args) => {
            let item_id = context.item_id(args.item_id)?;
            let labels = client.list_item_labels(project, item_id).await?;
            output::write(args.json, &labels, |output| {
                render::write_item_labels(output, &labels)
            })
        }
        LabelCommand::Add(args) => {
            let item_id = context.item_id(args.item_id)?;
            let item = client
                .add_item_label(
                    project,
                    item_id,
                    &CreateWorkItemLabelRequest {
                        key: args.key,
                        value: args.value,
                    },
                    args.expect_version,
                )
                .await?;
            output::write(args.json, &item, |output| {
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
                .update_item_label(project, item_id, args.label_id, &request)
                .await?;
            output::write(args.json, &item, |output| {
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
                .delete_item_label(project, item_id, args.label_id, args.expect_version)
                .await?;
            output::write(args.json, &deleted, |output| {
                writeln!(output, "Deleted label #{}", deleted.label_id)
            })
        }
        LabelCommand::Suggestions(args) => {
            let labels = client.list_project_labels(project).await?;
            output::write(args.json, &labels, |output| {
                render::write_project_label_suggestions(output, &labels)
            })
        }
    }
}

async fn run_relationship(command: RelationshipCommand, context: ResolvedContext) -> Result<()> {
    let client = context.client();
    let project = context.project()?;
    match command {
        RelationshipCommand::List(args) => {
            let item_id = context.item_id(args.item_id)?;
            let relationships = client.list_item_relationships(project, item_id).await?;
            output::write(args.json, &relationships, |output| {
                render::write_relationship_rows(output, &relationships)
            })
        }
        RelationshipCommand::Add(args) => {
            let item_id = context.item_id(args.item_id)?;
            let relationship = client
                .create_item_relationship(
                    project,
                    item_id,
                    &CreateWorkItemRelationshipRequest {
                        target_work_item_id: args.target,
                        kind: args.kind,
                    },
                )
                .await?;
            output::write(args.json, &relationship, |output| {
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
                    project,
                    args.relationship_id,
                    &UpdateWorkItemRelationshipRequest { kind: args.kind },
                )
                .await?;
            output::write(args.json, &relationship, |output| {
                render::write_relationship_view(output, &relationship, "Updated")
            })
        }
        RelationshipCommand::Delete(args) => {
            let deleted = client
                .delete_relationship(project, args.relationship_id)
                .await?;
            output::write(args.json, &deleted, |output| {
                render::write_relationship_view(output, &deleted.relationship, "Deleted")
            })
        }
    }
}

async fn run_comment(command: CommentCommand, context: ResolvedContext) -> Result<()> {
    let client = context.client();
    let project = context.project()?;
    match command {
        CommentCommand::Add(args) => {
            let item_id = context.item_id(args.item_id)?;
            let comment = client
                .add_comment(
                    project,
                    item_id,
                    &AddCommentRequest {
                        author_type: args.author_type,
                        author_name: args.author,
                        body: args.body,
                    },
                )
                .await?;
            output::write(args.json, &comment, |output| {
                writeln!(
                    output,
                    "Added comment #{} to item #{}",
                    comment.id, comment.work_item_id
                )
            })
        }
        CommentCommand::List(args) => {
            let comments = client
                .list_comments(project, context.item_id(args.item_id)?)
                .await?;
            output::write(args.json, &comments, |output| {
                render::write_comments(output, &comments)
            })
        }
    }
}

async fn run_memory(command: MemoryCommand, context: ResolvedContext) -> Result<()> {
    let client = context.client();
    let project = context.project()?;
    match command {
        MemoryCommand::Show(args) => {
            let memory = client.get_project_memory(project).await?;
            output::write(args.json, &memory, |output| {
                writeln!(output, "{}", memory.memory)
            })
        }
        MemoryCommand::History(args) => {
            let events = client.list_project_memory_events(project).await?;
            output::write(args.json, &events, |output| {
                render::write_memory_events(output, &events)
            })
        }
        MemoryCommand::Set(args) => {
            let agent_id = context.agent_id()?;
            let update = client
                .set_project_memory(
                    project,
                    &UpdateProjectMemoryRequest {
                        agent_id: agent_id.to_owned(),
                        agent_run_id: None,
                        body: args.body,
                    },
                )
                .await?;
            output::write(args.json, &update, |output| {
                writeln!(
                    output,
                    "Updated memory for project {} with event #{}",
                    update.project.name, update.event.id
                )
            })
        }
        MemoryCommand::Append(args) => {
            let agent_id = context.agent_id()?;
            let update = client
                .append_project_memory(
                    project,
                    &UpdateProjectMemoryRequest {
                        agent_id: agent_id.to_owned(),
                        agent_run_id: None,
                        body: args.body,
                    },
                )
                .await?;
            output::write(args.json, &update, |output| {
                writeln!(
                    output,
                    "Appended memory for project {} with event #{}",
                    update.project.name, update.event.id
                )
            })
        }
    }
}

async fn run_automation(command: AutomationCommand, context: ResolvedContext) -> Result<()> {
    let client = context.client();
    let project = context.project()?;
    match command {
        AutomationCommand::Runs(args) => {
            let runs = client.list_runs(project, args.limit).await?;
            output::write(args.json, &runs, |output| {
                render::write_automation_runs(output, &runs)
            })
        }
        AutomationCommand::Log(args) => {
            let log = client.read_run_log(project, args.run_id).await?;
            output::write(args.json, &log, |output| {
                render::write_run_log(output, &log)
            })
        }
    }
}

fn optional_override<T>(value: Option<T>, clear: bool) -> Option<Option<T>> {
    if clear { Some(None) } else { value.map(Some) }
}
