use assertr::prelude::*;
use std::fs;

use dispatch_types::AgentGitCommandPolicy;
use git2::Signature;

use tempfile::TempDir;

use super::repository::encoding::{decode, default_agent_git_command_policy_json};
use super::runtime::{
    expand_home_path, expand_home_path_with, inspect_project_git_status,
    validate_knowledge_directory_update,
};
use super::settings::apply_update;
use crate::backend::{
    entities::project::ProjectModel,
    events::UiEventBus,
    knowledge::DEFAULT_KNOWLEDGE_DIRECTORY,
    storage::{Store, TransactionManager},
};
use dispatch_types::*;
use git2::Repository;
use std::{path::PathBuf, sync::Arc, time::Duration};

use super::model::ProjectChangeSource;
use super::*;

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
        .await
        .unwrap();
    (temp, store)
}

fn project_path(temp: &TempDir, name: &str) -> PathBuf {
    let path = temp.path().join(name);
    fs::create_dir_all(&path).unwrap();
    path
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().unwrap();
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .unwrap();
    let tree_id = index.write_tree().unwrap();
    index.write().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = Signature::now("Dispatch Test", "dispatch@example.com").unwrap();
    repository
        .commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            message,
            &tree,
            &[],
        )
        .unwrap();
    repository.set_head("refs/heads/main").unwrap();
}

fn project_model(path: PathBuf) -> ProjectModel {
    ProjectModel {
        id: 1,
        name: "demo".to_owned(),
        display_name: "Demo".to_owned(),
        path: Some(path.to_string_lossy().into_owned()),
        knowledge_directory: DEFAULT_KNOWLEDGE_DIRECTORY.to_owned(),
        knowledge_source_lineage_id: None,
        path_exists: true,
        path_checked_at: Some("2026-06-19T00:00:00Z".to_owned()),
        system_prompt: String::new(),
        memory: String::new(),
        workspace_mode: WorkspaceMode::CurrentBranch.as_storage().to_owned(),
        max_code_edit_agents: 1,
        max_read_only_agents: 2,
        create_pr: false,
        auto_commit: true,
        commit_standard: String::new(),
        revert_strategy: RevertStrategy::Manual.as_storage().to_owned(),
        stale_claim_minutes: 0,
        worktree_cleanup_policy: WorktreeCleanupPolicy::Manual.as_storage().to_owned(),
        default_agent_tool: AgentToolName::Codex.as_storage().to_owned(),
        default_agent_model: Some(CodexAgentModel::newest().as_storage().to_owned()),
        default_agent_reasoning_effort: Some(
            AgentReasoningEffort::highest().as_storage().to_owned(),
        ),
        agent_sandbox_mode: AgentSandboxMode::WorkspaceWrite.as_storage().to_owned(),
        agent_extra_writable_roots: String::new(),
        agent_git_command_policy: default_agent_git_command_policy_json(),
        created_at: "2026-06-19T00:00:00Z".to_owned(),
        updated_at: "2026-06-19T00:00:00Z".to_owned(),
    }
}

#[tokio::test]
async fn pooled_project_lookup_waits_for_the_only_connection_but_transaction_lookup_completes() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("dispatch.sqlite3"), 1)
        .await
        .unwrap();
    crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "one-connection".to_owned(),
            display_name: None,
            path: project_path(&temp, "workspace"),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    let transaction = TransactionManager::new(&store).begin().await.unwrap();

    let pooled_lookup = tokio::time::timeout(
        Duration::from_millis(100),
        crate::backend::projects::repository::ProjectRepository::new(store.db())
            .by_name("one-connection"),
    )
    .await;
    assert_that!(&pooled_lookup.is_err()).is_true();

    let transaction_lookup = tokio::time::timeout(
        Duration::from_secs(2),
        repository::ProjectRepository::new(store.db()).by_name_in(&transaction, "one-connection"),
    )
    .await;

    assert_that!(&transaction_lookup.is_ok()).is_true();
    assert_that!(&transaction_lookup.unwrap().unwrap().name)
        .is_equal_to("one-connection".to_owned());
    transaction.rollback().await.unwrap();
}

#[test]
fn existing_documents_require_explicit_knowledge_directory_relocation() {
    let temp = TempDir::new().unwrap();
    let workspace = project_path(&temp, "demo");
    fs::create_dir_all(workspace.join("knowledge")).unwrap();
    fs::write(
        workspace.join("knowledge/README.md"),
        "# Project\n\nKeep these documents in their current location.",
    )
    .unwrap();
    let project = decode(project_model(workspace)).unwrap();

    let error = validate_knowledge_directory_update(&project, "design").unwrap_err();

    assert_that!(&(error.to_string()))
        .contains("cannot change the knowledge directory while it contains files");
}

async fn create_demo_project(store: &Store, path: PathBuf) {
    let event_bus = crate::backend::events::UiEventBus::new();

    crate::backend::projects::tests::service(store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path,
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
}

#[test]
fn settings_plan_merges_validates_and_applies_updates() {
    let temp = TempDir::new().unwrap();
    let project = decode(project_model(project_path(&temp, "demo"))).unwrap();
    let database_path = temp.path().join("dispatch.sqlite3");

    let plan = apply_update(
        UpdateProjectSettings {
            workspace_mode: Some(WorkspaceMode::GitBranch),
            max_read_only_agents: Some(0),
            create_pr: Some(true),
            commit_standard: Some(" Use short subjects. ".to_owned()),
            default_agent_model: Some(Some("  ".to_owned())),
            agent_extra_writable_roots: Some(vec![
                " ~/Library/Caches/chrome-for-testing-manager ".to_owned(),
                "~/Library/Caches/chrome-for-testing-manager".to_owned(),
                "/tmp/dispatch-browser".to_owned(),
            ]),
            agent_git_command_policy: Some(AgentGitCommandPolicy {
                add: true,
                commit: false,
                push: true,
                reset: false,
                ..Default::default()
            }),
            ..Default::default()
        },
        &project,
        &database_path,
    )
    .unwrap();

    assert_that!(&(plan.workspace_mode)).is_equal_to(WorkspaceMode::GitBranch);
    assert_that!(&(plan.max_code_edit_agents)).is_equal_to(1);
    assert_that!(&(plan.max_read_only_agents)).is_equal_to(0);
    assert_that!(&(plan.create_pr)).is_true();
    assert_that!(&(plan.commit_standard)).is_equal_to("Use short subjects.");
    assert_that!(&(plan.default_agent_model)).is_equal_to(None);
    assert_that!(&(plan.agent_extra_writable_roots)).is_equal_to(vec![
        expand_home_path("~/Library/Caches/chrome-for-testing-manager"),
        "/tmp/dispatch-browser".to_owned(),
    ]);

    let active = plan;

    assert_that!(&(active.workspace_mode)).is_equal_to(WorkspaceMode::GitBranch);
    assert_that!(&(active.max_read_only_agents)).is_equal_to(0);
    assert_that!(&(active.create_pr)).is_equal_to(true);
    assert_that!(&(active.commit_standard)).is_equal_to("Use short subjects.".to_owned());
    assert_that!(&(active.default_agent_model)).is_equal_to(None);
}

#[test]
fn settings_plan_rejects_empty_update() {
    let temp = TempDir::new().unwrap();
    let project = decode(project_model(project_path(&temp, "demo"))).unwrap();
    let database_path = temp.path().join("dispatch.sqlite3");

    let err = apply_update(UpdateProjectSettings::default(), &project, &database_path).unwrap_err();

    assert_that!(
        &(err
            .to_string()
            .contains("project settings update requires at least one field"))
    )
    .is_true();
}

#[test]
fn project_view_reports_invalid_persisted_settings_instead_of_panicking() {
    let temp = TempDir::new().unwrap();
    let mut project = project_model(project_path(&temp, "demo"));
    project.workspace_mode = "parallel_everywhere".to_owned();

    let err = decode(project).unwrap_err();

    assert_that!(&(format!("{err:#}").contains("project has invalid workspace mode"))).is_true();
}

#[test]
fn project_settings_reject_invalid_persisted_json_at_the_storage_boundary() {
    let temp = TempDir::new().unwrap();
    let mut project = project_model(project_path(&temp, "demo"));
    project.agent_git_command_policy = "{ definitely not json }".to_owned();

    let err = decode(project).unwrap_err();

    assert_that!(&(format!("{err:#}").contains("project has invalid agent Git command policy")))
        .is_true();
}

#[tokio::test]
async fn missing_project_is_rejected() {
    let (_temp, store) = test_store().await;

    let err = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .by_name("missing")
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("project 'missing' does not exist"))).is_true();
}

#[tokio::test]
async fn creating_project_requires_path() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: PathBuf::new(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("project path is required"))).is_true();
}

#[test]
fn project_path_expands_home_prefix() {
    let home = std::ffi::OsString::from("/Users/example");

    assert_that!(&(expand_home_path_with("~/dev/vibetest", Some(&home))))
        .is_equal_to("/Users/example/dev/vibetest");
    assert_that!(&(expand_home_path_with("~", Some(&home)))).is_equal_to("/Users/example");
}

#[test]
fn project_git_status_reports_branch_and_diff_counts() {
    let temp = TempDir::new().unwrap();
    let repository = Repository::init(temp.path()).unwrap();
    fs::write(temp.path().join("notes.txt"), "one\ntwo\n").unwrap();
    commit_all(&repository, "Initial commit");
    fs::write(temp.path().join("notes.txt"), "one\nthree\nfour\n").unwrap();

    let status = inspect_project_git_status(Some(temp.path().to_str().unwrap()), true).unwrap();

    assert_that!(&(status.is_repository)).is_true();
    assert_that!(&(status.branch.as_deref())).is_equal_to(Some("main"));
    assert_that!(&(status.added_lines)).is_equal_to(2);
    assert_that!(&(status.deleted_lines)).is_equal_to(1);
    assert_that!(&(status.error.is_none())).is_true();
}

#[test]
fn project_git_status_reports_existing_non_repository() {
    let temp = TempDir::new().unwrap();

    let status = inspect_project_git_status(Some(temp.path().to_str().unwrap()), true).unwrap();

    assert_that!(&(!status.is_repository)).is_true();
    assert_that!(&(status.branch.is_none())).is_true();
    assert_that!(&(status.added_lines)).is_equal_to(0);
    assert_that!(&(status.deleted_lines)).is_equal_to(0);
    assert_that!(&(status.error.is_none())).is_true();
}

#[tokio::test]
async fn project_crud_preserves_name_and_updates_path() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    let demo_path = project_path(&temp, "demo-path");
    let new_demo_path = project_path(&temp, "new-demo-path");

    let created = crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: demo_path.clone(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: Some("Prefer small changes.".to_owned()),
            memory: Some("Initial memory.".to_owned()),
        })
        .await
        .unwrap();

    assert_that!(&(created.name)).is_equal_to("demo");
    assert_that!(&(created.display_name)).is_equal_to("demo");
    assert_that!(&(created.path.as_deref())).is_equal_to(Some(demo_path.to_str().unwrap()));
    assert_that!(&(created.system_prompt)).is_equal_to("Prefer small changes.");
    let updated = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update(
            "demo",
            UpdateProject {
                display_name: Some("Demo Project".to_owned()),
                path: Some(ProjectPathUpdate::Set(new_demo_path.clone())),
            },
        )
        .await
        .unwrap();

    assert_that!(&(updated.display_name)).is_equal_to("Demo Project");
    assert_that!(&(updated.path.as_deref())).is_equal_to(Some(new_demo_path.to_str().unwrap()));
    assert_that!(&(updated.system_prompt)).is_equal_to("Prefer small changes.");
}

#[tokio::test]
async fn path_status_refresh_detects_deleted_path() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    let demo_path = project_path(&temp, "demo-path");
    crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: demo_path.clone(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    fs::remove_dir_all(&demo_path).unwrap();

    let refreshed =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .refresh_path_statuses()
            .await
            .unwrap();

    assert_that!(&(refreshed.len())).is_equal_to(1);
    assert_that!(&(!refreshed[0].path_exists)).is_true();
    assert_that!(&(refreshed[0].path_checked_at.is_some())).is_true();
}

#[tokio::test]
async fn system_prompt_history_snapshots_initial_and_updated_values() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: project_path(&temp, "demo"),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: Some("Initial prompt.".to_owned()),
            memory: None,
        })
        .await
        .unwrap();

    let initial_events =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .system_prompt_events("demo")
            .await
            .unwrap();
    assert_that!(&(initial_events.len())).is_equal_to(1);
    assert_that!(&(initial_events[0].operation)).is_equal_to("initial");
    assert_that!(&(initial_events[0].system_prompt)).is_equal_to("Initial prompt.");
    assert_that!(&(initial_events[0].actor_type.as_deref())).is_equal_to(Some("system"));

    let updated = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_system_prompt_with_source(
            "demo",
            "Updated prompt.".to_owned(),
            ProjectChangeSource::User,
        )
        .await
        .unwrap();
    assert_that!(&(updated.project.system_prompt)).is_equal_to("Updated prompt.");
    assert_that!(&(updated.event.operation)).is_equal_to("set");
    assert_that!(&(updated.event.system_prompt)).is_equal_to("Updated prompt.");
    assert_that!(&(updated.event.actor_type.as_deref())).is_equal_to(Some("user"));

    let current = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    assert_that!(&(current.system_prompt)).is_equal_to("Updated prompt.");

    let events =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .system_prompt_events("demo")
            .await
            .unwrap();
    assert_that!(&(events.len())).is_equal_to(2);
    assert_that!(&(events[0].id)).is_equal_to(updated.event.id);

    let cleared = crate::backend::projects::tests::service(&store, event_bus.clone())
        .clear_system_prompt_history("demo")
        .await
        .unwrap();
    assert_that!(&(cleared.deleted_events)).is_equal_to(2);
    assert_that!(
        &(crate::backend::projects::tests::service(
            &store,
            crate::backend::events::UiEventBus::new()
        )
        .system_prompt_events("demo")
        .await
        .unwrap()
        .is_empty())
    )
    .is_true();
    let current = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    assert_that!(&(current.system_prompt)).is_equal_to("Updated prompt.");

    let cleared = crate::backend::projects::tests::service(&store, event_bus.clone())
        .clear_system_prompt_history("demo")
        .await
        .unwrap();
    assert_that!(&(cleared.deleted_events)).is_equal_to(0);
}

#[tokio::test]
async fn settings_are_created_with_safe_defaults() {
    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let settings = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .settings("demo")
        .await
        .unwrap();

    assert_that!(&settings.knowledge_directory).is_equal_to("knowledge");
    assert_that!(&(settings.workspace_mode)).is_equal_to(WorkspaceMode::CurrentBranch);
    assert_that!(&(allowed_code_edit_agents(&settings))).is_equal_to(1);
    assert_that!(&(settings.max_read_only_agents)).is_equal_to(2);
    assert_that!(&(!settings.create_pr)).is_true();
    assert_that!(&(settings.auto_commit)).is_true();
    assert_that!(&(settings.commit_standard)).is_equal_to("");
    assert_that!(&(settings.revert_strategy)).is_equal_to(RevertStrategy::Manual);
    assert_that!(&(settings.stale_claim_minutes)).is_equal_to(0);
    assert_that!(&(settings.worktree_cleanup_policy)).is_equal_to(WorktreeCleanupPolicy::Manual);
    assert_that!(&(settings.default_agent_tool)).is_equal_to(AgentToolName::Codex);
    assert_that!(&(settings.default_agent_model.as_deref()))
        .is_equal_to(Some(CodexAgentModel::newest().as_storage()));
    assert_that!(&(settings.default_agent_reasoning_effort))
        .is_equal_to(Some(AgentReasoningEffort::highest()));
    assert_that!(&(settings.agent_sandbox_mode)).is_equal_to(AgentSandboxMode::WorkspaceWrite);
    assert_that!(&(settings.agent_extra_writable_roots.is_empty())).is_true();
    assert_that!(&(settings.agent_git_command_policy))
        .is_equal_to(AgentGitCommandPolicy::default());
}

#[tokio::test]
async fn project_create_accepts_known_default_agent_model() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;

    let project = crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: project_path(&temp, "demo"),
            default_agent_model: Some("gpt-5.4-mini".to_owned()),
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();

    assert_that!(&(project.default_agent_model.as_deref())).is_equal_to(Some("gpt-5.4-mini"));
    assert_that!(&(project.default_agent_reasoning_effort))
        .is_equal_to(Some(AgentReasoningEffort::XHigh));
}

#[tokio::test]
async fn project_create_accepts_default_agent_reasoning_effort() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;

    let project = crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: project_path(&temp, "demo"),
            default_agent_model: None,
            default_agent_reasoning_effort: Some(AgentReasoningEffort::High),
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();

    assert_that!(&(project.default_agent_reasoning_effort))
        .is_equal_to(Some(AgentReasoningEffort::High));
}

#[tokio::test]
async fn settings_reject_unknown_default_agent_model() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                default_agent_model: Some(Some("gpt-4.1-codex".to_owned())),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(
        &(err
            .to_string()
            .contains("default agent model must be one of"))
    )
    .is_true();
}

#[tokio::test]
async fn settings_reject_incompatible_model_and_reasoning_effort() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                default_agent_model: Some(Some("gpt-5.6-sol".to_owned())),
                default_agent_reasoning_effort: Some(Some(AgentReasoningEffort::Minimal)),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("default agent model"))).is_true();
    assert_that!(&(err.to_string().contains("incompatible"))).is_true();
}

#[tokio::test]
async fn settings_update_the_project_row() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let settings = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                knowledge_directory: Some("project-notes".to_owned()),
                workspace_mode: Some(WorkspaceMode::GitBranch),
                max_read_only_agents: Some(4),
                create_pr: Some(true),
                auto_commit: Some(false),
                commit_standard: Some(" Use Conventional Commits. ".to_owned()),
                revert_strategy: Some(RevertStrategy::GitReset),
                default_agent_tool: Some(AgentToolName::Codex),
                agent_sandbox_mode: Some(AgentSandboxMode::DangerFullAccess),
                agent_extra_writable_roots: Some(vec![
                    " ~/Library/Caches/chrome-for-testing-manager ".to_owned(),
                    "".to_owned(),
                    "~/Library/Caches/chrome-for-testing-manager".to_owned(),
                    "/tmp/dispatch-browser".to_owned(),
                ]),
                agent_git_command_policy: Some(AgentGitCommandPolicy {
                    add: true,
                    commit: false,
                    push: true,
                    reset: false,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let project = crate::backend::projects::repository::ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();

    assert_that!(&(settings.project_id)).is_equal_to(project.id);
    assert_that!(&(settings.workspace_mode)).is_equal_to(WorkspaceMode::GitBranch);
    assert_that!(&settings.knowledge_directory).is_equal_to("project-notes");
    assert_that!(&(project.workspace_mode)).is_equal_to(WorkspaceMode::GitBranch);
    assert_that!(&project.knowledge_directory).is_equal_to("project-notes");
    assert_that!(&(settings.max_read_only_agents)).is_equal_to(4);
    assert_that!(&(project.max_read_only_agents)).is_equal_to(4);
    assert_that!(&(!settings.auto_commit)).is_true();
    assert_that!(&(!project.auto_commit)).is_true();
    assert_that!(&(settings.commit_standard)).is_equal_to("Use Conventional Commits.");
    assert_that!(&(project.commit_standard)).is_equal_to("Use Conventional Commits.");
    assert_that!(&(settings.revert_strategy)).is_equal_to(RevertStrategy::GitReset);
    assert_that!(&(project.revert_strategy)).is_equal_to(RevertStrategy::GitReset);
    assert_that!(&(project.default_agent_tool)).is_equal_to(AgentToolName::Codex);
    assert_that!(&(settings.agent_sandbox_mode)).is_equal_to(AgentSandboxMode::DangerFullAccess);
    assert_that!(&(project.agent_sandbox_mode)).is_equal_to(AgentSandboxMode::DangerFullAccess);
    assert_that!(&(settings.agent_extra_writable_roots)).is_equal_to(vec![
        expand_home_path("~/Library/Caches/chrome-for-testing-manager"),
        "/tmp/dispatch-browser".to_owned(),
    ]);
    assert_that!(&(project.agent_extra_writable_roots))
        .is_equal_to(settings.agent_extra_writable_roots);
    assert_that!(&(settings.agent_git_command_policy)).is_equal_to(AgentGitCommandPolicy {
        add: true,
        commit: false,
        push: true,
        reset: false,
        ..Default::default()
    });
    assert_that!(&(project.agent_git_command_policy))
        .is_equal_to(settings.agent_git_command_policy);
}

#[tokio::test]
async fn settings_reject_roots_that_include_database() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                agent_extra_writable_roots: Some(vec![temp.path().to_string_lossy().into_owned()]),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("includes Dispatch database"))).is_true();
}

#[tokio::test]
async fn settings_reject_zero_code_edit_agents() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                max_code_edit_agents: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("at least 1"))).is_true();
}

#[tokio::test]
async fn settings_allow_zero_read_only_agents_but_reject_negative_values() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let settings = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                max_read_only_agents: Some(0),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_that!(&(settings.max_read_only_agents)).is_equal_to(0);
    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                max_read_only_agents: Some(-1),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_that!(&(err.to_string().contains("max read-only agents"))).is_true();
}

#[tokio::test]
async fn non_worktree_strategy_rejects_parallel_agents() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::GitBranch),
                max_code_edit_agents: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("only git_worktree"))).is_true();
}

#[tokio::test]
async fn current_branch_rejects_pull_request_creation() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::CurrentBranch),
                create_pr: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("pull requests"))).is_true();
}

#[tokio::test]
async fn branch_strategy_allows_pull_requests_but_caps_concurrency() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let settings = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                workspace_mode: Some(WorkspaceMode::GitBranch),
                create_pr: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_that!(&(settings.create_pr)).is_true();
    assert_that!(&(allowed_code_edit_agents(&settings))).is_equal_to(1);
}

#[tokio::test]
async fn stale_claim_timeout_cannot_be_negative() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    create_demo_project(&store, project_path(&temp, "demo")).await;

    let err = crate::backend::projects::tests::service(&store, event_bus.clone())
        .update_settings(
            "demo",
            UpdateProjectSettings {
                stale_claim_minutes: Some(-1),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();

    assert_that!(&(err.to_string().contains("stale claim"))).is_true();
}

pub(crate) fn service(store: &Store, events: UiEventBus) -> Arc<super::service::ProjectService> {
    Arc::new(super::service::ProjectService::new(
        Arc::new(TransactionManager::new(store)),
        Arc::new(repository::ProjectRepository::new(store.db())),
        Arc::new(repository::ProjectDefaultsRepository::new()),
        Arc::new(runtime::ProjectRuntime),
        events,
        store.path().to_owned(),
    ))
}

#[tokio::test]
async fn failed_project_initialization_rolls_back_defaults_and_notifications() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("db.sqlite3"), 1)
        .await
        .unwrap();
    let events = UiEventBus::new();
    let service = service(&store, events.clone());
    let mut notifications = events.subscribe();
    store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_initial_lane BEFORE INSERT ON swim_lanes BEGIN SELECT RAISE(ABORT, 'reject initial lane'); END".to_owned())).await.unwrap();
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        service.create(CreateProject {
            name: "rollback".into(),
            display_name: None,
            path: temp.path().to_owned(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: Some("Initial prompt".into()),
            memory: None,
        }),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_that!(&format!("{error:#}")).contains("reject initial lane");
    assert_that!(&service.list().await.unwrap()).is_empty();
    for table in [
        "label_keys",
        "personalities",
        "work_item_states",
        "work_item_events",
    ] {
        let row = store
            .db()
            .query_one(Statement::from_string(
                DbBackend::Sqlite,
                format!("SELECT COUNT(*) AS count FROM {table}"),
            ))
            .await
            .unwrap()
            .unwrap();
        assert_that!(&row.try_get::<i64>("", "count").unwrap()).is_equal_to(0);
    }
    assert_that!(&notifications.try_recv().is_err()).is_true();
}

#[tokio::test]
async fn failed_prompt_history_write_rolls_back_prompt_without_success_notification() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let temp = TempDir::new().unwrap();
    let store = Store::open_with_max_connections(temp.path().join("db.sqlite3"), 1)
        .await
        .unwrap();
    let events = UiEventBus::new();
    let service = service(&store, events.clone());
    service
        .create(CreateProject {
            name: "rollback".into(),
            display_name: None,
            path: temp.path().to_owned(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: Some("Original prompt".into()),
            memory: None,
        })
        .await
        .unwrap();
    let history = service.system_prompt_events("rollback").await.unwrap();
    store.db().execute(Statement::from_string(DbBackend::Sqlite,
        "CREATE TRIGGER reject_prompt_history BEFORE INSERT ON work_item_events WHEN NEW.event_type='SystemPromptChanged' BEGIN SELECT RAISE(ABORT, 'reject prompt history'); END".to_owned())).await.unwrap();
    let mut notifications = events.subscribe();
    let error = tokio::time::timeout(
        Duration::from_secs(3),
        service.update_system_prompt("rollback", "Discard me".into()),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_that!(&format!("{error:#}")).contains("reject prompt history");
    assert_that!(&service.get("rollback").await.unwrap().system_prompt)
        .is_equal_to("Original prompt");
    assert_that!(&service.system_prompt_events("rollback").await.unwrap()).is_equal_to(history);
    assert_that!(&notifications.try_recv().is_err()).is_true();
}

#[tokio::test]
async fn stale_admin_identity_cannot_edit_a_recreated_project() {
    use sea_orm::EntityTrait;
    let (temp, store) = test_store().await;
    let events = UiEventBus::new();
    let service = service(&store, events.clone());
    let input = CreateProject {
        name: "demo".into(),
        display_name: None,
        path: temp.path().to_owned(),
        default_agent_model: None,
        default_agent_reasoning_effort: None,
        system_prompt: None,
        memory: None,
    };
    let old = service.create(input.clone()).await.unwrap();
    // A fixture simulates deletion between CrudKit's fetch and the service's transaction.
    crate::backend::entities::project::Project::delete_by_id(old.id)
        .exec(store.db().as_ref())
        .await
        .unwrap();
    let current = service.create(input).await.unwrap();
    let mut notifications = events.subscribe();
    let error = service
        .edit(
            "demo",
            Some(old.id),
            UpdateProject {
                display_name: Some("stale".into()),
                path: None,
            },
            None,
        )
        .await
        .unwrap_err();
    assert_that!(&error.to_string()).contains("changed while it was being edited");
    assert_that!(&service.get("demo").await.unwrap()).is_equal_to(current);
    assert_that!(&notifications.try_recv().is_err()).is_true();
}

#[tokio::test]
async fn settings_update_can_repair_a_persisted_policy_violation() {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (temp, store) = test_store().await;
    create_demo_project(&store, temp.path().to_owned()).await;
    store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "UPDATE projects SET max_code_edit_agents=0 WHERE name='demo'".to_owned(),
        ))
        .await
        .unwrap();
    let service = service(&store, UiEventBus::new());
    assert_that!(&service.get("demo").await.is_err()).is_true();
    let repaired = service
        .update_settings(
            "demo",
            UpdateProjectSettings {
                max_code_edit_agents: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_that!(&repaired.max_code_edit_agents).is_equal_to(1);
    assert_that!(&service.get("demo").await.unwrap().max_code_edit_agents).is_equal_to(1);
}
