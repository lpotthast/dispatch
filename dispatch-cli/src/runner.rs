use std::time::Duration;

use dispatch_types::{
    AddCommentRequest, ClaimWorkItemRequest, CreateWorkItemLabelRequest,
    CreateWorkItemRelationshipRequest, CreateWorkItemRequest, FinishWorkItemRequest,
    ProgressWorkItemRequest, ReleaseWorkItemRequest, RequestFeedbackWorkItemRequest,
    UpdateProjectMemoryRequest, UpdateWorkItemLabelRequest, UpdateWorkItemRelationshipRequest,
    UpdateWorkItemRequest,
};
use rootcause::Result;

use crate::{
    commands::{
        AutomationCommand, Command, CommentCommand, ItemCommand, ItemCreateArgs, LabelCommand,
        MemoryCommand, RelationshipCommand,
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
            let json = args.json;
            let request = create_work_item_request(args);
            let item = client.create_item(project, &request).await?;
            output::write(json, &item, |output| {
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

fn create_work_item_request(args: ItemCreateArgs) -> CreateWorkItemRequest {
    CreateWorkItemRequest {
        title: args.title,
        description: args.description,
        state: args.state,
        agent_model_override: args.agent_model,
        agent_reasoning_effort_override: args.agent_reasoning_effort,
        initial_labels: args
            .labels
            .into_iter()
            .map(create_work_item_label_request)
            .collect(),
    }
}

fn create_work_item_label_request(raw: String) -> CreateWorkItemLabelRequest {
    let (key, value) = match raw.split_once('=') {
        Some((key, value)) => (key.trim().to_owned(), trimmed_label_value(value)),
        None => (raw.trim().to_owned(), None),
    };
    CreateWorkItemLabelRequest { key, value }
}

fn trimmed_label_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
    };

    use dispatch_types::AgentReasoningEffort;
    use serde_json::json;

    use crate::context::{ContextOverrides, resolve_context};

    use super::*;

    struct CapturedRequest {
        request_line: String,
        body: String,
    }

    fn create_args(labels: &[&str]) -> ItemCreateArgs {
        ItemCreateArgs {
            title: "Title".to_owned(),
            description: "Description".to_owned(),
            labels: labels.iter().map(|label| (*label).to_owned()).collect(),
            state: None,
            agent_model: None,
            agent_reasoning_effort: None,
            json: false,
        }
    }

    fn env_from<'a>(
        entries: &'a [(&'a str, &'a str)],
    ) -> impl Fn(&str) -> std::result::Result<String, std::env::VarError> + 'a {
        move |key| {
            entries
                .iter()
                .find(|(entry_key, _)| *entry_key == key)
                .map(|(_, value)| value.to_string())
                .ok_or(std::env::VarError::NotPresent)
        }
    }

    fn spawn_create_item_server() -> (
        String,
        mpsc::Receiver<CapturedRequest>,
        thread::JoinHandle<()>,
    ) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base_url = format!("http://{}", listener.local_addr().unwrap());
        let (request_tx, request_rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_http_request(&mut stream);
            request_tx.send(request).unwrap();

            let response_body = json!({
                "id": 123,
                "project_id": 4,
                "title": "Created",
                "description": "Created through test",
                "state": "open",
                "labels": [
                    {
                        "id": 1,
                        "project_id": 4,
                        "work_item_id": 123,
                        "key": "state",
                        "value": "open",
                        "created_at": "2026-06-19T00:00:00Z",
                        "updated_at": "2026-06-19T00:00:00Z"
                    },
                    {
                        "id": 2,
                        "project_id": 4,
                        "work_item_id": 123,
                        "key": "type",
                        "value": "feature",
                        "created_at": "2026-06-19T00:00:00Z",
                        "updated_at": "2026-06-19T00:00:00Z"
                    },
                    {
                        "id": 3,
                        "project_id": 4,
                        "work_item_id": 123,
                        "key": "needs-verification",
                        "value": null,
                        "created_at": "2026-06-19T00:00:00Z",
                        "updated_at": "2026-06-19T00:00:00Z"
                    }
                ],
                "version": 1,
                "claimed_by": null,
                "claimed_at": null,
                "claim_expires_at": null,
                "claim_source": null,
                "finished_at": null,
                "agent_model_override": null,
                "agent_reasoning_effort_override": null,
                "created_at": "2026-06-19T00:00:00Z",
                "updated_at": "2026-06-19T00:00:00Z",
                "comment_count": 0
            })
            .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
        });
        (base_url, request_rx, handle)
    }

    fn read_http_request(stream: &mut std::net::TcpStream) -> CapturedRequest {
        let mut buffer = Vec::new();
        let mut chunk = [0; 1024];
        let header_end = loop {
            let read = stream.read(&mut chunk).unwrap();
            assert!(read > 0, "client closed before request headers completed");
            buffer.extend_from_slice(&chunk[..read]);
            if let Some(header_end) = find_header_end(&buffer) {
                break header_end;
            }
        };
        let headers = String::from_utf8(buffer[..header_end].to_vec()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        let body_start = header_end + b"\r\n\r\n".len();
        while buffer.len() < body_start + content_length {
            let read = stream.read(&mut chunk).unwrap();
            assert!(read > 0, "client closed before request body completed");
            buffer.extend_from_slice(&chunk[..read]);
        }
        let request_line = headers.lines().next().unwrap().to_owned();
        let body =
            String::from_utf8(buffer[body_start..body_start + content_length].to_vec()).unwrap();
        CapturedRequest { request_line, body }
    }

    fn find_header_end(buffer: &[u8]) -> Option<usize> {
        buffer
            .windows(b"\r\n\r\n".len())
            .position(|window| window == b"\r\n\r\n")
    }

    #[test]
    fn create_item_request_without_labels_has_empty_initial_labels() {
        let request = create_work_item_request(create_args(&[]));

        assert_eq!(request.title, "Title");
        assert_eq!(request.description, "Description");
        assert!(request.initial_labels.is_empty());
    }

    #[test]
    fn create_item_request_preserves_state_and_agent_overrides() {
        let request = create_work_item_request(ItemCreateArgs {
            title: "Title".to_owned(),
            description: "Description".to_owned(),
            labels: Vec::new(),
            state: Some("idea".to_owned()),
            agent_model: Some("gpt-5-codex".to_owned()),
            agent_reasoning_effort: Some(AgentReasoningEffort::High),
            json: true,
        });

        assert_eq!(request.state.as_deref(), Some("idea"));
        assert_eq!(request.agent_model_override.as_deref(), Some("gpt-5-codex"));
        assert_eq!(
            request.agent_reasoning_effort_override,
            Some(AgentReasoningEffort::High)
        );
    }

    #[test]
    fn create_item_request_parses_initial_labels() {
        let request = create_work_item_request(create_args(&[
            "type=feature",
            "needs-verification",
            "token=a=b",
        ]));

        assert_eq!(request.initial_labels.len(), 3);
        assert_eq!(request.initial_labels[0].key, "type");
        assert_eq!(request.initial_labels[0].value.as_deref(), Some("feature"));
        assert_eq!(request.initial_labels[1].key, "needs-verification");
        assert!(request.initial_labels[1].value.is_none());
        assert_eq!(request.initial_labels[2].key, "token");
        assert_eq!(request.initial_labels[2].value.as_deref(), Some("a=b"));
    }

    #[test]
    fn create_item_request_trims_keys_and_values() {
        let request = create_work_item_request(create_args(&[
            " type = feature ",
            " needs-verification ",
            " empty = ",
        ]));

        assert_eq!(request.initial_labels[0].key, "type");
        assert_eq!(request.initial_labels[0].value.as_deref(), Some("feature"));
        assert_eq!(request.initial_labels[1].key, "needs-verification");
        assert!(request.initial_labels[1].value.is_none());
        assert_eq!(request.initial_labels[2].key, "empty");
        assert!(request.initial_labels[2].value.is_none());
    }

    #[test]
    fn create_item_request_leaves_validation_to_server() {
        let request = create_work_item_request(create_args(&[
            "dup=one",
            "dup=two",
            "=missing-key",
            "state=blocked",
        ]));

        assert_eq!(request.initial_labels[0].key, "dup");
        assert_eq!(request.initial_labels[0].value.as_deref(), Some("one"));
        assert_eq!(request.initial_labels[1].key, "dup");
        assert_eq!(request.initial_labels[1].value.as_deref(), Some("two"));
        assert_eq!(request.initial_labels[2].key, "");
        assert_eq!(
            request.initial_labels[2].value.as_deref(),
            Some("missing-key")
        );
        assert_eq!(request.initial_labels[3].key, "state");
        assert_eq!(request.initial_labels[3].value.as_deref(), Some("blocked"));
    }

    #[tokio::test]
    async fn item_create_posts_initial_labels_without_agent_or_item_context() {
        let (api_url, request_rx, server_handle) = spawn_create_item_server();
        let context = resolve_context(
            &ContextOverrides::default(),
            env_from(&[
                ("DISPATCH_API_URL", api_url.as_str()),
                ("DISPATCH_PROJECT", "demo"),
                ("DISPATCH_CLAIMED_ITEM_ID", "999"),
            ]),
        )
        .unwrap();

        run(
            Command::Item {
                command: ItemCommand::Create(ItemCreateArgs {
                    title: "Created through CLI".to_owned(),
                    description: "Body".to_owned(),
                    labels: vec!["type=feature".to_owned(), "needs-verification".to_owned()],
                    state: Some("open".to_owned()),
                    agent_model: None,
                    agent_reasoning_effort: None,
                    json: false,
                }),
            },
            context,
        )
        .await
        .unwrap();

        let request = request_rx.recv().unwrap();
        server_handle.join().unwrap();
        assert_eq!(
            request.request_line,
            "POST /api/projects/demo/items HTTP/1.1"
        );

        let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(body["title"], "Created through CLI");
        assert_eq!(body["description"], "Body");
        assert_eq!(body["state"], "open");
        assert_eq!(
            body["initial_labels"],
            json!([
                { "key": "type", "value": "feature" },
                { "key": "needs-verification", "value": null }
            ])
        );
    }
}
