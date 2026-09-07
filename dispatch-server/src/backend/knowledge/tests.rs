use super::runtime::read;
use super::*;
use crate::backend::projects::repository::ProjectRepository;
use crate::backend::storage::Store;
use assertr::prelude::*;
use dispatch_types::knowledge::*;
use std::{fs, path::Path};
use tempfile::TempDir;

fn write(root: &Path, path: &str, body: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}
fn document(id: &str, parent: Option<&str>, body: &str) -> String {
    format!(
        "---\nid: {id}\n{}---\n\n# {id}\n\n{body}\n",
        parent
            .map(|p| format!("refines: [{p}]\n"))
            .unwrap_or_default()
    )
}
fn view(root: &Path, operation: KnowledgeOperation, query: KnowledgeQuery) -> KnowledgeView {
    read(1, root.to_str().unwrap(), "knowledge", operation, &query).unwrap()
}

#[test]
fn ordinary_edits_are_current_without_sidecars_or_initialization() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        "knowledge/README.md",
        &document("project", None, "The project overview."),
    );
    write(
        root,
        "knowledge/detail.md",
        &document("detail", Some("project"), "Original behavior."),
    );
    write(
        root,
        "knowledge/low.md",
        &document("low", Some("detail"), "Implementation constraints."),
    );
    let before = view(root, KnowledgeOperation::Root, KnowledgeQuery::default());
    assert_that!(&before.documents.len()).is_equal_to(1);
    write(
        root,
        "knowledge/detail.md",
        &document("detail", Some("project"), "Updated behavior."),
    );
    let after = view(
        root,
        KnowledgeOperation::Node,
        KnowledgeQuery {
            id: Some("detail".into()),
            ..Default::default()
        },
    );
    assert_that!(&after.index_generation).is_not_equal_to(before.index_generation);
    assert_that!(&after.document.unwrap().markdown).contains("Updated behavior");
    assert_that!(&after.documents[0].path).is_equal_to("low.md");
    assert_that!(&after.documents[0].summary).is_equal_to("Implementation constraints.");
    assert_that!(&root.join("knowledge/.dispatch").exists()).is_false();
}

#[test]
fn routing_text_uses_markdown_structure_and_preserves_plain_file_access() {
    let temp = TempDir::new().unwrap();
    write(
        temp.path(),
        "knowledge/detail.md",
        "```markdown\n# Example\n```\n\n# Real *title*\n\nAn opening [summary](detail.md) with `code`.\n\nMore detail.",
    );
    write(
        temp.path(),
        "knowledge/notes.md",
        "Notes without a heading.",
    );
    let result = view(
        temp.path(),
        KnowledgeOperation::List,
        KnowledgeQuery::default(),
    );
    assert_that!(&result.documents[0].title).is_equal_to("Real title");
    assert_that!(&result.documents[0].summary).is_equal_to("An opening summary with code.");
    assert_that!(&result.documents[1].title).is_equal_to("notes.md");
}

#[test]
fn plain_and_malformed_documents_remain_readable_and_errors_stay_local() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        "knowledge/plain.md",
        "# Notes\n\nSearchable plain text.",
    );
    write(
        root,
        "knowledge/broken.md",
        "---\nid: broken\nid: duplicate\n---\n# Broken\n\nReadable raw bytes.",
    );
    let root_view = view(root, KnowledgeOperation::Root, KnowledgeQuery::default());
    assert_that!(&root_view.document).is_none();
    let found = view(
        root,
        KnowledgeOperation::Search,
        KnowledgeQuery {
            text: Some("Searchable".into()),
            ..Default::default()
        },
    );
    assert_that!(&found.documents.len()).is_equal_to(1);
    let raw = view(
        root,
        KnowledgeOperation::Node,
        KnowledgeQuery {
            path: Some("broken.md".into()),
            ..Default::default()
        },
    );
    assert_that!(&raw.document.unwrap().markdown).contains("Readable raw bytes");
    assert_that!(&raw.diagnostics.iter().any(|d| d.code == "invalid_metadata")).is_true();
}

#[test]
fn duplicate_ids_and_cycles_do_not_hide_unrelated_navigation() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        "knowledge/README.md",
        &document("project", None, "Overview."),
    );
    write(
        root,
        "knowledge/good.md",
        &document("good", Some("project"), "Useful detail."),
    );
    write(root, "knowledge/a.md", &document("a", Some("b"), "A."));
    write(root, "knowledge/b.md", &document("b", Some("a"), "B."));
    write(
        root,
        "knowledge/duplicate1.md",
        &document("duplicate", Some("project"), "One."),
    );
    write(
        root,
        "knowledge/duplicate2.md",
        &document("duplicate", Some("project"), "Two."),
    );
    let result = view(root, KnowledgeOperation::Root, KnowledgeQuery::default());
    assert_that!(
        &result
            .documents
            .iter()
            .map(|d| d.path.as_str())
            .collect::<Vec<_>>()
    )
    .is_equal_to(vec!["good.md"]);
    assert_that!(
        &result
            .diagnostics
            .iter()
            .any(|d| d.code == "refinement_cycle")
    )
    .is_true();
    assert_that!(
        &read(
            1,
            root.to_str().unwrap(),
            "knowledge",
            KnowledgeOperation::Node,
            &KnowledgeQuery {
                id: Some("duplicate".into()),
                ..Default::default()
            }
        )
        .is_err()
    )
    .is_true();
}

#[test]
fn ignore_families_are_independent_and_apply_to_tracked_files() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        "knowledge/README.md",
        &document("project", None, "Overview."),
    );
    for name in ["private.md", "git.md", "visible.md"] {
        write(root, &format!("knowledge/{name}"), "# Notes\n\nContent.");
    }
    let repository = git2::Repository::init(root).unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(Path::new("knowledge/private.md")).unwrap();
    index.write().unwrap();
    write(
        root,
        ".gitignore",
        "knowledge/private.md\nknowledge/git.md\n",
    );
    write(root, ".dispatchignore", "!knowledge/private.md\n");
    write(root, "knowledge/.gitignore", "!git.md\n");
    let result = view(root, KnowledgeOperation::List, KnowledgeQuery::default());
    assert_that!(
        &result
            .documents
            .iter()
            .map(|d| d.path.as_str())
            .collect::<Vec<_>>()
    )
    .is_equal_to(vec!["README.md", "git.md", "visible.md"]);
    write(root, "knowledge/.gitignore", "!git.md\n!private.md\n");
    write(root, ".dispatchignore", "knowledge/private.md\n");
    assert_that!(
        &view(root, KnowledgeOperation::List, KnowledgeQuery::default())
            .documents
            .len()
    )
    .is_equal_to(3);
    fs::remove_file(root.join(".dispatchignore")).unwrap();
    assert_that!(
        &view(root, KnowledgeOperation::List, KnowledgeQuery::default())
            .documents
            .len()
    )
    .is_equal_to(4);
    assert_that!(&fs::read_to_string(root.join("knowledge/private.md")).unwrap())
        .contains("Content");
}

#[test]
fn pruned_directories_do_not_load_nested_controls_or_follow_symlinks() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(root, ".dispatchignore", "knowledge/pruned/\n");
    write(root, "knowledge/pruned/.dispatchignore", "!hidden.md\n");
    write(
        root,
        "knowledge/pruned/hidden.md",
        "# Hidden\n\nHidden content.",
    );
    write(root, "knowledge/visible.md", "# Visible\n\nPublic content.");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        root.join("knowledge/pruned/hidden.md"),
        root.join("knowledge/link.md"),
    )
    .unwrap();
    let result = view(root, KnowledgeOperation::List, KnowledgeQuery::default());
    assert_that!(&result.documents.len()).is_equal_to(1);
    assert_that!(&result.documents[0].path).is_equal_to("visible.md");
}

#[test]
fn invalid_nested_ignore_controls_close_only_their_scope() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(root, "knowledge/valid.md", "# Valid\n\nSafe.");
    write(
        root,
        "knowledge/private/secret.md",
        "# Secret\n\nDo not serve.",
    );
    fs::create_dir(root.join("knowledge/private/.gitignore")).unwrap();
    let result = view(root, KnowledgeOperation::List, KnowledgeQuery::default());
    assert_that!(&result.documents.len()).is_equal_to(1);
    assert_that!(
        &result
            .diagnostics
            .iter()
            .any(|d| d.code == "discovery_failed")
    )
    .is_true();
}

#[test]
fn pagination_and_unicode_body_continuation_are_explicit() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let body = format!("# Large\n\n{}", "ä".repeat(40_000));
    write(root, "knowledge/large.md", &body);
    write(root, "knowledge/plain.md", "# Plain\n\nNotes.");
    let first = view(
        root,
        KnowledgeOperation::List,
        KnowledgeQuery {
            limit: Some(1),
            ..Default::default()
        },
    );
    assert_that!(&first.next_offset).is_equal_to(Some(1));
    let first = view(
        root,
        KnowledgeOperation::Node,
        KnowledgeQuery {
            path: Some("large.md".into()),
            ..Default::default()
        },
    )
    .document
    .unwrap();
    let second = view(
        root,
        KnowledgeOperation::Node,
        KnowledgeQuery {
            path: Some("large.md".into()),
            body_offset: first.next_body_offset.unwrap(),
            ..Default::default()
        },
    )
    .document
    .unwrap();
    assert_that!(&(first.markdown + &second.markdown)).is_equal_to(body);
}

#[tokio::test]
async fn server_reads_registered_run_working_copy_and_rejects_other_projects() {
    let event_bus = crate::backend::events::UiEventBus::new();

    use crate::backend::{
        entities::agent_run::AgentRunActiveModel, execution::identity as agent_ids,
        projects::CreateProject, storage::utc_now,
    };
    use sea_orm::{ActiveModelTrait, ActiveValue::Set};
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let main = root.join("main");
    let worktree = root.join("worktree");
    write(
        &main,
        "knowledge/README.md",
        &document("project", None, "Main checkout."),
    );
    write(
        &worktree,
        "knowledge/README.md",
        &document("project", None, "Assigned working copy."),
    );
    let store = Store::open(root.join("test.sqlite3")).await.unwrap();
    for name in ["demo", "other"] {
        crate::backend::projects::tests::service(&store, event_bus.clone())
            .create(CreateProject {
                name: name.into(),
                display_name: None,
                path: main.clone(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
    }
    let project_id = ProjectRepository::new(store.db()).id("demo").await.unwrap();
    let run = AgentRunActiveModel {
        project_id: Set(project_id),
        run_kind: Set("task".into()),
        tool_name: Set("codex".into()),
        mutability: Set("mutating".into()),
        status: Set("running".into()),
        command: Set(String::new()),
        working_dir: Set(worktree.to_str().unwrap().into()),
        created_at: Set(utc_now()),
        updated_at: Set(utc_now()),
        ..Default::default()
    }
    .insert(store.db().as_ref())
    .await
    .unwrap();
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "x-dispatch-agent-id",
        agent_ids::dispatch_run_agent_id(run.id).parse().unwrap(),
    );
    headers.insert(
        "x-dispatch-agent-run-id",
        run.id.to_string().parse().unwrap(),
    );
    let attribution = crate::backend::attribution::transport::parse(&headers).unwrap();
    let result = crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    )
    .state
    .knowledge
    .query(
        "demo",
        attribution.clone(),
        KnowledgeOperation::Root,
        KnowledgeQuery::default(),
    )
    .await
    .unwrap();
    assert_that!(&result.document.unwrap().markdown).contains("Assigned working copy");
    assert_that!(
        &crate::backend::attribution::transport::from_knowledge_headers(
            &crate::backend::attribution::tests_service(&store),
            "other",
            &headers
        )
        .await
        .is_err()
    )
    .is_true();
    write(&worktree, ".dispatchignore", "knowledge/README.md\n");
    let result = crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    )
    .state
    .knowledge
    .query(
        "demo",
        attribution.clone(),
        KnowledgeOperation::Root,
        KnowledgeQuery::default(),
    )
    .await
    .unwrap();
    assert_that!(&result.document).is_none();
}

#[test]
fn graph_pages_contain_visible_summaries_and_typed_valid_edges() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        "knowledge/README.md",
        &document("project", None, "Overview."),
    );
    write(
        root,
        "knowledge/a.md",
        "---\nid: a\nrefines: [project]\nrelated_to: [b]\ndepends_on: [b, missing]\n---\n# A\n\nSummary.\n\nPRIVATE BODY DETAIL\n",
    );
    write(
        root,
        "knowledge/b.md",
        "---\nid: b\nrefines: [project]\nrelated_to: [a]\n---\n# B\n\nSummary.\n",
    );
    let graph = view(
        root,
        KnowledgeOperation::Graph,
        KnowledgeQuery {
            limit: Some(100),
            ..Default::default()
        },
    );
    assert_that!(&graph.document).is_none();
    assert_that!(&graph.relations.len()).is_equal_to(4);
    assert_that!(
        &graph
            .relations
            .iter()
            .filter(|r| r.kind == KnowledgeRelationKind::RelatedTo)
            .count()
    )
    .is_equal_to(1);
    assert_that!(&serde_json::to_string(&graph).unwrap()).does_not_contain("PRIVATE BODY DETAIL");
    let first = view(
        root,
        KnowledgeOperation::Graph,
        KnowledgeQuery {
            limit: Some(1),
            ..Default::default()
        },
    );
    assert_that!(&first.next_offset).is_some();
    write(root, ".dispatchignore", "knowledge/b.md\n");
    let hidden = view(root, KnowledgeOperation::Graph, KnowledgeQuery::default());
    assert_that!(&hidden.documents.len()).is_equal_to(2);
    assert_that!(
        &hidden
            .relations
            .iter()
            .any(|r| r.from == "b.md" || r.to == "b.md")
    )
    .is_false();
}

#[test]
fn editor_saves_preserve_bytes_and_reject_stale_or_excluded_writes() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    let text =
        "---\r\nid: project\r\ncustom: keep this\r\n---\r\n# Project\r\n\r\nAn overview.\r\n";
    let create = KnowledgeSaveRequest {
        path: "README.md".into(),
        expected_fingerprint: None,
        markdown: text.into(),
    };
    let saved = editing::DocumentWriter::default()
        .save(root, "knowledge", &create)
        .unwrap();
    assert_that!(&fs::read_to_string(root.join("knowledge/README.md")).unwrap()).is_equal_to(text);
    assert_that!(
        &editing::DocumentWriter::default()
            .save(root, "knowledge", &create)
            .is_err()
    )
    .is_true();
    let edit = KnowledgeSaveRequest {
        expected_fingerprint: Some(saved.fingerprint),
        markdown: text.replace("An overview.", "The accepted contract."),
        ..create
    };
    write(root, "knowledge/README.md", "# Concurrent edit\n");
    assert_that!(
        &editing::DocumentWriter::default()
            .save(root, "knowledge", &edit)
            .is_err()
    )
    .is_true();
    assert_that!(&fs::read_to_string(root.join("knowledge/README.md")).unwrap())
        .is_equal_to("# Concurrent edit\n");
    write(root, "knowledge/README.md", text);
    write(root, ".gitignore", "knowledge/README.md\n");
    assert_that!(
        &editing::DocumentWriter::default()
            .save(root, "knowledge", &edit)
            .is_err()
    )
    .is_true();
    fs::remove_file(root.join(".gitignore")).unwrap();
    editing::DocumentWriter::default()
        .save(root, "knowledge", &edit)
        .unwrap();
    assert_that!(&fs::read_to_string(root.join("knowledge/README.md")).unwrap())
        .contains("custom: keep this\r\n");
}

#[test]
fn editor_cannot_create_through_exclusions_or_escape_the_knowledge_directory() {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    write(
        root,
        ".dispatchignore",
        "knowledge/private/\nknowledge/secret.md\n",
    );
    for path in [
        "../outside.md",
        "/absolute.md",
        ".dispatch/store.md",
        "private/new.md",
        "secret.md",
    ] {
        let request = KnowledgeSaveRequest {
            path: path.into(),
            expected_fingerprint: None,
            markdown: "# Document\n".into(),
        };
        assert_that!(
            &editing::DocumentWriter::default()
                .save(root, "knowledge", &request)
                .is_err()
        )
        .is_true();
    }
    assert_that!(&root.join("knowledge").exists()).is_false();
    #[cfg(unix)]
    {
        fs::create_dir(root.join("knowledge")).unwrap();
        std::os::unix::fs::symlink(root, root.join("knowledge/escape")).unwrap();
        let request = KnowledgeSaveRequest {
            path: "escape/new.md".into(),
            expected_fingerprint: None,
            markdown: "# Document\n".into(),
        };
        assert_that!(
            &editing::DocumentWriter::default()
                .save(root, "knowledge", &request)
                .is_err()
        )
        .is_true();
        assert_that!(&root.join("new.md").exists()).is_false();
    }
}
