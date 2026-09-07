use crate::backend::execution::workspaces::runs::WorkspacePlan;
use crate::backend::runs::model::AutomationTriggerOrigin;
use crate::backend::storage::Store;
use std::fs;

use crate::backend::entities::agent_run::{AgentRun, AgentRunActiveModel, AgentRunModel};
use assertr::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set, EntityTrait};
use tempfile::TempDir;

use super::*;
use crate::backend::{
    execution::identity as agent_ids,
    items::CreateWorkItem,
    projects::{CreateProject, UpdateProjectSettings},
};
use crate::shared::view_models::AUTOMATION_BLOCKED_LABEL_KEY;

#[tokio::test]
async fn awaited_automation_execution_uses_a_fresh_task_boundary() {
    let (caller, execution) = tokio::spawn(async {
        let caller = tokio::task::id();
        let execution = await_automation_execution(0, None, async { Ok(tokio::task::id()) })
            .await
            .unwrap();
        (caller, execution)
    })
    .await
    .unwrap();

    assert_that!(&(caller)).is_not_equal_to(execution);
}

#[tokio::test]
async fn awaited_automation_execution_finishes_session_after_caller_is_aborted() {
    let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
    sessions.begin(ProcessSessionStart {
        run_id: 7,
        project_id: 1,
        project_name: "demo".to_owned(),
        tool_name: "codex".to_owned(),
        command: "codex app-server".to_owned(),
        working_dir: "/tmp/demo".to_owned(),
    });
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (complete_tx, complete_rx) = tokio::sync::oneshot::channel();
    let sessions_for_caller = sessions.clone();

    let caller = tokio::spawn(async move {
        await_automation_execution(7, Some(sessions_for_caller), async move {
            started_tx.send(()).unwrap();
            complete_rx.await.unwrap();
            Err::<(), _>(report!("expected execution failure"))
        })
        .await
    });
    started_rx.await.unwrap();
    caller.abort();
    assert_that!(&(caller.await.unwrap_err().is_cancelled())).is_true();

    complete_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if sessions.get_for_project(1, 7).is_none() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn awaited_automation_execution_finishes_session_after_execution_panics() {
    let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
    sessions.begin(ProcessSessionStart {
        run_id: 8,
        project_id: 1,
        project_name: "demo".to_owned(),
        tool_name: "codex".to_owned(),
        command: "codex app-server".to_owned(),
        working_dir: "/tmp/demo".to_owned(),
    });
    let sessions_for_execution = sessions.clone();

    let result: Result<()> =
        await_automation_execution(8, Some(sessions_for_execution), async move {
            panic!("expected automation execution panic")
        })
        .await;

    assert_that!(&(result.unwrap_err().to_string()))
        .contains("automation execution task terminated unexpectedly");
    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if sessions.get_for_project(1, 8).is_none() {
                return;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn delayed_old_project_session_cannot_claim_same_name_replacement_work() {
    let (temp, store) = test_store().await;
    let old_project = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    let started = service(&store)
        .begin(
            "demo",
            StartAutomation {
                tool: Some(AgentToolName::Codex),
                launch_target: AgentLaunchTargetV1::none(),
                work_item_selector: None,
                extra_prompt: None,
                mutability: Some(AutomationRunMutability::Mutating),
                personality_id: None,
                trigger: None,
                execution: Default::default(),
                postconditions: None,
            },
            None,
        )
        .await
        .unwrap();
    let old_run_id = started.run.id;
    let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
    let deletion = sessions.begin_project_deletion(old_project.id).unwrap();

    crate::backend::entities::project::Project::delete_by_id(old_project.id)
        .exec(store.db().as_ref())
        .await
        .unwrap();
    deletion.mark_deleted();
    let replacement =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
    let replacement_item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Replacement work".to_owned(),
            description: "Must not be claimed by the deleted project run".to_owned(),
            state: crate::shared::view_models::DEFAULT_STATE_LABEL.to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let cancellation = register_pending_session(&started, Some(&sessions), None);
    assert_that!(&(cancellation_requested(&cancellation))).is_true();
    assert_that!(
        &(sessions
            .get_for_project(old_project.id, old_run_id)
            .is_none())
    )
    .is_true();

    let result = service_with_sessions(&store, Some(sessions.clone()))
        .complete(started, cancellation)
        .await;
    let unchanged_item =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", replacement_item.id)
            .await
            .unwrap();
    assert_that!(&(result.is_err())).is_true();
    assert_that!(&(unchanged_item.claimed_by)).is_equal_to(None);

    let replacement_session = sessions.begin(ProcessSessionStart {
        run_id: old_run_id + 1,
        project_id: replacement.id,
        project_name: replacement.name,
        tool_name: "codex".to_owned(),
        command: String::new(),
        working_dir: temp.path().to_string_lossy().into_owned(),
    });
    assert_that!(&(!replacement_session.cancellation_requested())).is_true();
    assert_that!(&(replacement_session.is_registered())).is_true();
}

async fn test_store() -> (TempDir, Store) {
    let temp = TempDir::new().unwrap();
    let store = Store::open(temp.path().join("dispatch.sqlite3"))
        .await
        .unwrap();
    crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
        .create(CreateProject {
            name: "demo".to_owned(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: Some("Prefer concise automation.".to_owned()),
            memory: Some("Use Dispatch comments.".to_owned()),
        })
        .await
        .unwrap();
    (temp, store)
}

async fn create_preview_test_run(
    store: &Store,
    project_id: i64,
    item_id: i64,
    created_at: &str,
    status: AgentRunStatus,
) -> AgentRunModel {
    let event_bus = crate::backend::events::UiEventBus::new();

    let run = crate::backend::runs::tests::service(store, event_bus.clone())
        .create(
            project_id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::Mutating,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();
    let mut active: AgentRunActiveModel = AgentRun::find_by_id(run.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap()
        .into();
    active.work_item_id = Set(Some(item_id));
    active.status = Set(status.as_storage().to_owned());
    active.result_summary = Set(format!("summary {created_at}"));
    active.created_at = Set(created_at.to_owned());
    active.updated_at = Set(created_at.to_owned());
    active.update(store.db().as_ref()).await.unwrap()
}

#[tokio::test]
async fn item_run_previews_are_batched_scoped_and_limited_to_newest_three() {
    let (temp, store) = test_store().await;
    crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
        .create(CreateProject {
            name: "other".to_owned(),
            display_name: None,
            path: temp.path().to_path_buf(),
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    let demo = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    let other = ProjectRepository::new(store.db())
        .by_name("other")
        .await
        .unwrap();
    let target = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Target".to_owned(),
            description: "Target description".to_owned(),
            state: "backlog".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let unrelated = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Unrelated".to_owned(),
            description: "Unrelated description".to_owned(),
            state: "backlog".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let other_item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("other"),
        CreateWorkItem {
            title: "Other project".to_owned(),
            description: "Other project description".to_owned(),
            state: "backlog".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    let mut target_runs = Vec::new();
    for (created_at, status) in [
        ("2026-07-14T10:00:01Z", AgentRunStatus::Completed),
        ("2026-07-14T10:00:02Z", AgentRunStatus::Failed),
        ("2026-07-14T10:00:03Z", AgentRunStatus::Cancelled),
        ("2026-07-14T10:00:04Z", AgentRunStatus::Running),
        ("2026-07-14T10:00:04Z", AgentRunStatus::Failed),
    ] {
        target_runs
            .push(create_preview_test_run(&store, demo.id, target.id, created_at, status).await);
    }
    create_preview_test_run(
        &store,
        demo.id,
        unrelated.id,
        "2026-07-14T10:00:05Z",
        AgentRunStatus::Completed,
    )
    .await;
    create_preview_test_run(
        &store,
        other.id,
        other_item.id,
        "2026-07-14T10:00:06Z",
        AgentRunStatus::Completed,
    )
    .await;

    let previews = crate::backend::runs::queries::tests::service(&store)
        .previews("demo", &[target.id, other_item.id])
        .await
        .unwrap();
    let target_preview = previews.get(&target.id).unwrap();

    assert_that!(&(target_preview.total)).is_equal_to(5);
    assert_that!(
        &(target_preview
            .latest
            .iter()
            .map(|run| run.id)
            .collect::<Vec<_>>())
    )
    .is_equal_to(vec![
        target_runs[4].id,
        target_runs[3].id,
        target_runs[2].id,
    ]);
    assert_that!(&(target_preview.latest[0].status)).is_equal_to(AgentRunStatus::Failed);
    assert_that!(&(target_preview.latest[0].result_summary))
        .is_equal_to("summary 2026-07-14T10:00:04Z".to_owned());
    assert_that!(&(target_preview.latest[0].created_at))
        .is_equal_to("2026-07-14T10:00:04Z".to_owned());
    assert_that!(&(!previews.contains_key(&unrelated.id))).is_true();
    assert_that!(&(!previews.contains_key(&other_item.id))).is_true();

    let empty = crate::backend::runs::queries::tests::service(&store)
        .previews("demo", &[])
        .await
        .unwrap();
    assert_that!(&(empty.is_empty())).is_true();
}

#[tokio::test]
async fn run_log_uses_active_session_output_when_available() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    let project = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    let run = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            project.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::Mutating,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();
    let log_path = temp.path().join("run.output.json");
    write_run_output_log(
        &log_path,
        &[new_output_piece(
            1,
            AgentRunOutputKind::ModelMessage,
            None,
            "persisted",
            "persisted output",
            serde_json::json!({}),
        )],
    )
    .unwrap();
    let developer_instructions_path = temp.path().join("run.developer-instructions.md");
    let user_prompt_path = temp.path().join("run.user-prompt.md");
    fs::write(&developer_instructions_path, "Follow Dispatch policy.").unwrap();
    fs::write(&user_prompt_path, "Implement the requested change.").unwrap();
    let run = crate::backend::runs::tests::service(&store, event_bus.clone())
        .launch(
            run,
            LaunchDetails {
                work_item_id: None,
                command: "codex app-server".to_owned(),
                workspace: WorkspacePlan {
                    working_dir: temp.path().to_path_buf(),
                    worktree_path: None,
                    branch_name: None,
                },
                developer_instructions_path: Some(
                    developer_instructions_path.to_string_lossy().into_owned(),
                ),
                user_prompt_path: Some(user_prompt_path.to_string_lossy().into_owned()),
                log_path: Some(log_path.to_string_lossy().into_owned()),
                agent_model: None,
                agent_reasoning_effort: None,
                commit_required: false,
                pr_requested: false,
                system_prompt_event_id: None,
                effective_input_sha256: String::new(),
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
            },
        )
        .await
        .unwrap();
    let sessions = ProcessSessionRegistry::new(crate::backend::events::UiEventBus::new());
    let _cancel = sessions.begin(ProcessSessionStart {
        run_id: run.id,
        project_id: run.project_id,
        project_name: "demo".to_owned(),
        tool_name: "codex".to_owned(),
        command: "codex app-server".to_owned(),
        working_dir: temp.path().to_string_lossy().into_owned(),
    });
    sessions.append_output_piece(
        run.id,
        new_output_piece(
            1,
            AgentRunOutputKind::ModelMessage,
            None,
            "active",
            "active output",
            serde_json::json!({}),
        ),
    );

    let run_log =
        crate::backend::runs::queries::tests::service_with_sessions(sessions.clone(), &store)
            .log("demo", run.id)
            .await
            .unwrap();

    assert_that!(&(run_log.active)).is_true();
    assert_that!(&(run_log.developer_instructions.as_deref()))
        .is_equal_to(Some("Follow Dispatch policy."));
    assert_that!(&(run_log.user_prompt.as_deref()))
        .is_equal_to(Some("Implement the requested change."));
    assert_that!(&(run_log.output.len())).is_equal_to(1);
    assert_that!(&(run_log.output[0].title)).is_equal_to("active");
    assert_that!(&(run_log.output[0].body)).is_equal_to("active output");
}

#[tokio::test]
async fn mutating_and_read_only_runs_have_independent_admission_limits() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let project = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();

    let mutating = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            project.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::Mutating,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();
    let read_only = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            project.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::ReadOnly,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();

    assert_that!(&(mutating.mutability)).is_equal_to(AutomationRunMutability::Mutating);
    assert_that!(&(read_only.mutability)).is_equal_to(AutomationRunMutability::ReadOnly);
    assert_that!(
        &(!crate::backend::runs::admission::tests::service(&store)
            .can_start_run("demo", AutomationRunMutability::Mutating)
            .await
            .unwrap())
    )
    .is_true();
    let settings = ProjectRepository::new(store.db())
        .settings("demo")
        .await
        .unwrap();
    let err = crate::backend::runs::admission::tests::service(&store)
        .enforce_start_allowed("demo", &settings, AutomationRunMutability::Mutating)
        .await
        .unwrap_err();
    assert_that!(&(err.to_string().contains("mutating"))).is_true();
    assert_that!(&(err.to_string().contains("limit is 1"))).is_true();
    assert_that!(
        &(crate::backend::runs::admission::tests::service(&store)
            .can_start_run("demo", AutomationRunMutability::ReadOnly)
            .await
            .unwrap())
    )
    .is_true();

    let status = crate::backend::runs::queries::tests::service(&store)
        .status("demo")
        .await
        .unwrap();
    assert_that!(&(status.running_runs)).is_equal_to(2);
    assert_that!(&(status.running_mutating_runs)).is_equal_to(1);
    assert_that!(&(status.running_read_only_runs)).is_equal_to(1);
    assert_that!(&(status.allowed_mutating_runs)).is_equal_to(1);
    assert_that!(&(status.settings.max_read_only_agents)).is_equal_to(2);

    crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            project.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::ReadOnly,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();
    assert_that!(
        &(!crate::backend::runs::admission::tests::service(&store)
            .can_start_run("demo", AutomationRunMutability::ReadOnly)
            .await
            .unwrap())
    )
    .is_true();
}

#[tokio::test]
async fn active_project_names_follow_running_runs() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let demo = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    let run = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            demo.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::ReadOnly,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();

    assert_that!(
        &(crate::backend::runs::queries::tests::service(&store)
            .active_project_names()
            .await
            .unwrap())
    )
    .is_equal_to(vec!["demo".to_owned()]);

    crate::backend::runs::tests::service(&store, event_bus.clone())
        .finish(run, AgentRunStatus::Completed, Some(0), "Done".to_owned())
        .await
        .unwrap();

    assert_that!(
        &(crate::backend::runs::queries::tests::service(&store)
            .active_project_names()
            .await
            .unwrap())
    )
    .is_empty();
}

#[tokio::test]
async fn read_only_admission_can_be_disabled_with_zero_limit() {
    let (_temp, store) = test_store().await;
    let settings =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .update_settings(
                "demo",
                UpdateProjectSettings {
                    max_read_only_agents: Some(0),
                    ..Default::default()
                },
            )
            .await
            .unwrap();

    assert_that!(
        &(!crate::backend::runs::admission::tests::service(&store)
            .can_start_run("demo", AutomationRunMutability::ReadOnly)
            .await
            .unwrap())
    )
    .is_true();
    let err = crate::backend::runs::admission::tests::service(&store)
        .enforce_start_allowed("demo", &settings, AutomationRunMutability::ReadOnly)
        .await
        .unwrap_err();
    assert_that!(&(err.to_string().contains("read-only"))).is_true();
    assert_that!(&(err.to_string().contains("limit is 0"))).is_true();
}

#[tokio::test]
async fn rule_caps_and_project_scoped_concurrency_groups_compose() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (_temp, store) = test_store().await;
    let project = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    let origin = AutomationTriggerOrigin {
        trigger_id: 101,
        trigger_name: "group-holder".to_owned(),
        trigger_revision_id: None,
    };
    let run = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            project.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::ReadOnly,
                trigger: Some(&origin),
                personality_revision_id: None,
                effective_timeout_seconds: 90,
                effective_concurrency_group: Some("shared-validation"),
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();
    assert_that!(&(run.effective_timeout_seconds)).is_equal_to(Some(90));
    assert_that!(&(run.effective_concurrency_group.as_deref()))
        .is_equal_to(Some("shared-validation"));
    let settings = ProjectRepository::new(store.db())
        .settings("demo")
        .await
        .unwrap();

    let cap_error = crate::backend::runs::admission::tests::service(&store)
        .enforce_rule_start_allowed(
            "demo",
            &settings,
            AutomationRunMutability::ReadOnly,
            Some(origin.trigger_id),
            &AutomationExecutionPolicy {
                max_concurrent_runs: Some(1),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_that!(&(cap_error.to_string().contains("rule already has 1"))).is_true();

    let group_error = crate::backend::runs::admission::tests::service(&store)
        .enforce_rule_start_allowed(
            "demo",
            &settings,
            AutomationRunMutability::ReadOnly,
            Some(202),
            &AutomationExecutionPolicy {
                concurrency_group: Some("shared-validation".to_owned()),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
    assert_that!(
        &(group_error
            .to_string()
            .contains("concurrency group 'shared-validation'"))
    )
    .is_true();
}

#[tokio::test]
async fn effective_agent_settings_prefer_item_overrides() {
    let (_temp, store) = test_store().await;
    let settings =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .update_settings(
                "demo",
                UpdateProjectSettings {
                    default_agent_model: Some(Some("gpt-5.5".to_owned())),
                    default_agent_reasoning_effort: Some(Some(AgentReasoningEffort::High)),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Configured item".to_owned(),
            description: "Exercise item overrides".to_owned(),
            state: "open".to_owned(),
            agent_model_override: Some("gpt-5.6-terra".to_owned()),
            agent_reasoning_effort_override: Some(AgentReasoningEffort::Medium),
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();

    assert_that!(
        &(effective_agent_model(
            &settings,
            Some(&item),
            &AutomationExecutionPolicy::default()
        )
        .as_deref())
    )
    .is_equal_to(Some("gpt-5.6-terra"));
    assert_that!(
        &(effective_agent_reasoning_effort(
            &settings,
            Some(&item),
            &AutomationExecutionPolicy::default(),
        ))
    )
    .is_equal_to(Some(AgentReasoningEffort::Medium));

    let rule = AutomationExecutionPolicy {
        model: Some("gpt-5.6-rule".to_owned()),
        reasoning_effort: Some(AgentReasoningEffort::Low),
        ..Default::default()
    };
    assert_that!(&(effective_agent_model(&settings, None, &rule).as_deref()))
        .is_equal_to(Some("gpt-5.6-rule"));
    assert_that!(&(effective_agent_reasoning_effort(&settings, None, &rule)))
        .is_equal_to(Some(AgentReasoningEffort::Low));
}

#[tokio::test]
async fn stop_automation_releases_claimed_work_back_to_source_state() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    let item = crate::backend::items::creation::tests::service(
        &store,
        crate::backend::events::UiEventBus::new(),
    )
    .create(
        crate::backend::projects::ProjectReference::Name("demo"),
        CreateWorkItem {
            title: "Cancel me".to_owned(),
            description: "Exercise cancellation release".to_owned(),
            state: "ready".to_owned(),
            agent_model_override: None,
            agent_reasoning_effort_override: None,
            initial_labels: Vec::new(),
        },
        Default::default(),
    )
    .await
    .unwrap();
    let project = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    let run = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            project.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::Mutating,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::next_open("ready").unwrap(),
            },
        )
        .await
        .unwrap();
    let agent_id = agent_ids::dispatch_run_agent_id(run.id);
    crate::backend::items::claims::tests::service(&store, event_bus.clone())
        .resolve_agent_run_target(
            "demo",
            run.id,
            &agent_id,
            &AgentLaunchTargetV1::next_open("ready").unwrap(),
            None,
        )
        .await
        .unwrap()
        .unwrap();
    crate::backend::runs::tests::service(&store, event_bus.clone())
        .launch(
            run,
            LaunchDetails {
                work_item_id: Some(item.id),
                command: "codex app-server".to_owned(),
                workspace: WorkspacePlan {
                    working_dir: temp.path().to_path_buf(),
                    worktree_path: None,
                    branch_name: None,
                },
                developer_instructions_path: None,
                user_prompt_path: None,
                log_path: None,
                agent_model: None,
                agent_reasoning_effort: None,
                commit_required: false,
                pr_requested: false,
                system_prompt_event_id: None,
                effective_input_sha256: String::new(),
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
            },
        )
        .await
        .unwrap();

    let cancelled =
        crate::backend::runs::tests::service(&store, crate::backend::events::UiEventBus::new())
            .cancel_project(project.id)
            .await
            .unwrap();
    let item =
        crate::backend::items::tests::service(&store, crate::backend::events::UiEventBus::new())
            .get("demo", item.id)
            .await
            .unwrap();

    assert_that!(&(cancelled.len())).is_equal_to(1);
    assert_that!(&(cancelled[0].status)).is_equal_to(AgentRunStatus::Cancelled);
    assert_that!(&(item.state.as_deref())).is_equal_to(Some("ready"));
    assert_that!(&(item.claimed_by)).is_equal_to(None);
    assert_that!(
        &(item
            .labels
            .iter()
            .all(|label| label.key != AUTOMATION_BLOCKED_LABEL_KEY))
    )
    .is_true();
}

#[tokio::test]
async fn stop_automation_by_id_does_not_cancel_same_name_replacement() {
    let event_bus = crate::backend::events::UiEventBus::new();

    let (temp, store) = test_store().await;
    let old_project = ProjectRepository::new(store.db())
        .by_name("demo")
        .await
        .unwrap();
    crate::backend::entities::project::Project::delete_by_id(old_project.id)
        .exec(store.db().as_ref())
        .await
        .unwrap();

    let replacement =
        crate::backend::projects::tests::service(&store, crate::backend::events::UiEventBus::new())
            .create(CreateProject {
                name: "demo".to_owned(),
                display_name: None,
                path: temp.path().to_path_buf(),
                default_agent_model: None,
                default_agent_reasoning_effort: None,
                system_prompt: None,
                memory: None,
            })
            .await
            .unwrap();
    let replacement_run = crate::backend::runs::tests::service(&store, event_bus.clone())
        .create(
            replacement.id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::Mutating,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: AGENT_PROCESS_TIMEOUT.as_secs(),
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::Ordinary,
                knowledge_job_id: None,
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
        .unwrap();

    let cancelled =
        crate::backend::runs::tests::service(&store, crate::backend::events::UiEventBus::new())
            .cancel_project(old_project.id)
            .await
            .unwrap();
    let persisted_replacement_run = AgentRun::find_by_id(replacement_run.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap();

    assert_that!(&(replacement.id)).is_not_equal_to(old_project.id);
    assert_that!(&(cancelled.is_empty())).is_true();
    assert_that!(&(persisted_replacement_run.status))
        .is_equal_to(AgentRunStatus::Running.as_storage().to_owned());
}

pub(crate) fn service(store: &Store) -> LaunchService {
    crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    )
    .state
    .launch
    .as_ref()
    .clone()
}
pub(crate) fn service_with_sessions(
    store: &Store,
    sessions: Option<ProcessSessionRegistry>,
) -> LaunchService {
    let mut service = service(store);
    service.sessions = sessions;
    service
}

#[tokio::test]
async fn allocation_and_launch_snapshot_share_one_connection_and_roll_back_together() {
    use crate::backend::projects::ProjectReference;
    use dispatch_types::AutomationPersonalityInput;
    use sea_orm::{ConnectionTrait, DbBackend, Statement};
    let (_temp, app, _, _) = crate::backend::comments::tests::application().await;
    let personality = app
        .state
        .personalities
        .create(
            ProjectReference::Name("demo"),
            AutomationPersonalityInput {
                key: "snapshot".into(),
                name: "Snapshot".into(),
                description: "Original personality".into(),
            },
        )
        .await
        .unwrap();
    let start = StartAutomation {
        tool: None,
        launch_target: AgentLaunchTargetV1::none(),
        work_item_selector: None,
        extra_prompt: None,
        mutability: Some(AutomationRunMutability::ReadOnly),
        personality_id: Some(personality.id),
        trigger: None,
        execution: Default::default(),
        postconditions: None,
    };
    let mut events = app.state.events.subscribe();
    app.state.store.db().execute(Statement::from_string(DbBackend::Sqlite,"CREATE TRIGGER reject_launch BEFORE INSERT ON agent_run_launch_contracts BEGIN SELECT RAISE(FAIL, 'launch contract unavailable'); END".to_owned())).await.unwrap();
    assert_that!(
        &app.state
            .launch
            .begin("demo", start.clone(), None)
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.run_queries.list("demo", None).await.unwrap()).is_empty();
    assert_that!(&events.try_recv().is_err()).is_true();
    app.state
        .store
        .db()
        .execute(Statement::from_string(
            DbBackend::Sqlite,
            "DROP TRIGGER reject_launch".to_owned(),
        ))
        .await
        .unwrap();
    let started = app.state.launch.begin("demo", start, None).await.unwrap();
    assert_that!(&started.run.personality_revision_id).is_equal_to(personality.current_revision_id);
    assert_that!(&started.personality_description.as_deref())
        .is_equal_to(Some("Original personality"));
    assert_that!(&started.run.launch_target).is_equal_to(Some(
        dispatch_types::AgentRunLaunchTargetView::None { schema_version: 1 },
    ));
    app.state
        .personalities
        .update(
            ProjectReference::Name("demo"),
            personality.id,
            AutomationPersonalityInput {
                key: "snapshot".into(),
                name: "Snapshot".into(),
                description: "Updated personality".into(),
            },
        )
        .await
        .unwrap();
    assert_that!(&started.personality_description.as_deref())
        .is_equal_to(Some("Original personality"));
    assert_that!(&started.run.personality_revision_id).is_equal_to(personality.current_revision_id);
}
