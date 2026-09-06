use assertr::prelude::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::mpsc,
    thread,
};

use serde_json::json;

use crate::{
    commands::{ItemCommand, ItemCreateArgs},
    context::{ContextOverrides, resolve_context},
};

use super::*;

struct CapturedRequest {
    request_line: String,
    body: String,
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

fn spawn_project_list_server() -> (
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

        write!(
            stream,
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: 2\r\nconnection: close\r\n\r\n[]"
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
        assert_that!(&(read > 0))
            .with_detail_message("client closed before request headers completed")
            .is_true();
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
        assert_that!(&(read > 0))
            .with_detail_message("client closed before request body completed")
            .is_true();
        buffer.extend_from_slice(&chunk[..read]);
    }
    let request_line = headers.lines().next().unwrap().to_owned();
    let body = String::from_utf8(buffer[body_start..body_start + content_length].to_vec()).unwrap();
    CapturedRequest { request_line, body }
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(b"\r\n\r\n".len())
        .position(|window| window == b"\r\n\r\n")
}

#[tokio::test]
async fn project_list_gets_projects_without_project_context() {
    let (api_url, request_rx, server_handle) = spawn_project_list_server();
    let context = resolve_context(
        ContextOverrides::default(),
        env_from(&[("DISPATCH_API_URL", api_url.as_str())]),
    )
    .unwrap();

    run(
        Command::Project {
            command: ProjectCommand::List,
        },
        context,
        output::Format::Text,
    )
    .await
    .unwrap();

    let request = request_rx.recv().unwrap();
    server_handle.join().unwrap();
    assert_that!(&(request.request_line)).is_equal_to("GET /api/projects HTTP/1.1");
    assert_that!(&(request.body.is_empty())).is_true();
}

#[tokio::test]
async fn item_create_posts_initial_labels_without_agent_or_item_context() {
    let (api_url, request_rx, server_handle) = spawn_create_item_server();
    let context = resolve_context(
        ContextOverrides::default(),
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
            }),
        },
        context,
        output::Format::Text,
    )
    .await
    .unwrap();

    let request = request_rx.recv().unwrap();
    server_handle.join().unwrap();
    assert_that!(&(request.request_line)).is_equal_to("POST /api/projects/demo/items HTTP/1.1");

    let body: serde_json::Value = serde_json::from_str(&request.body).unwrap();
    assert_that!(&(body["title"])).is_equal_to("Created through CLI");
    assert_that!(&(body["description"])).is_equal_to("Body");
    assert_that!(&(body["state"])).is_equal_to("open");
    assert_that!(&(body["initial_labels"])).is_equal_to(json!([
        { "key": "type", "value": "feature" },
        { "key": "needs-verification", "value": null }
    ]));
}
