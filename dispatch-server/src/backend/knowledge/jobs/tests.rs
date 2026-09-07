use super::*;
use crate::backend::projects::repository::ProjectRepository;
use crate::backend::runs::{launch::model::AgentLaunchTargetV1, model::CreateRunConfig};
use crate::backend::{
    attribution::model::AttributionInput,
    entities::agent_run::{AgentRun, AgentRunActiveModel},
    projects::CreateProject,
    storage::Store,
};
use assertr::prelude::*;
use dispatch_types::{
    AgentRunKind, AgentRunPurposeV1, AgentRunStatus, AgentRunView, AgentToolName,
    AutomationRunMutability,
};
use sea_orm::{
    ActiveModelTrait, ActiveValue::Set, ConnectionTrait, DbBackend, EntityTrait, Statement, Value,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio_util::sync::CancellationToken;
const SOURCE: &str = "Writes are durable after sync.\nOne writer owns the transaction.\nRecovery replays only committed records.\nRoutine helper details stay in source.\n";
fn doc(id: &str, parents: &[&str], body: &str) -> String {
    format!(
        "---\nid: {id}\nrefines: [{}]\n---\n# {id}\n\n{body}\n",
        parents.join(", ")
    )
}
fn write(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, body).unwrap();
}
async fn fixture() -> (TempDir, Store, i64) {
    let event_bus = crate::backend::events::UiEventBus::new();

    let temp = TempDir::new().unwrap();
    let working = temp.path().join("project");
    fs::create_dir_all(&working).unwrap();
    write(&working.join("storage.rs"), SOURCE);
    let store = Store::open_with_max_connections(temp.path().join("state/db.sqlite"), 1)
        .await
        .unwrap();
    let project = crate::backend::projects::tests::service(&store, event_bus.clone())
        .create(CreateProject {
            name: "fixture".into(),
            display_name: None,
            path: working,
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        })
        .await
        .unwrap();
    (temp, store, project.id)
}
fn request(key: &str) -> StartKnowledgeJob {
    StartKnowledgeJob {
        request_id: key.into(),
        budget_seconds: 60,
        ..Default::default()
    }
}
fn attribution(run: i64) -> AttributionInput {
    AttributionInput {
        agent_id: Some(format!("dispatch-run-{run}")),
        agent_run_id: Some(run),
    }
}
struct FakeAgent<'a> {
    store: &'a Store,
    failures: AtomicUsize,
    partial: bool,
}
impl<'a> FakeAgent<'a> {
    fn complete(store: &'a Store) -> Self {
        Self {
            store,
            failures: AtomicUsize::new(0),
            partial: false,
        }
    }
}
#[async_trait::async_trait]
impl execution::PassAgent for FakeAgent<'_> {
    async fn execute(
        &self,
        _cancellation: CancellationToken,
        input: execution::PassInput,
    ) -> Result<()> {
        let store = self.store;
        if self
            .failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            bail!("fixture transport interruption");
        }
        let run_id = input.run.id;
        let mut active: AgentRunActiveModel = AgentRun::find_by_id(input.run.id)
            .one(store.db().as_ref())
            .await
            .unwrap()
            .unwrap()
            .into();
        active.working_dir = Set(input.working.to_string_lossy().into_owned());
        active.update(store.db().as_ref()).await?;
        let a = attribution(run_id);
        let project_id = ProjectRepository::new(store.db()).id("fixture").await?;
        let record = service(store).load(project_id, input.job_id).await?;
        let mut r = JobReport {
            summary: "Persistence, ownership and recovery contracts".into(),
            ..Default::default()
        };
        match record.job().stage {
            JobStage::Discovery => {
                let supplied = service(store)
                    .source_query(
                        "fixture",
                        input.job_id,
                        a.clone(),
                        "read".into(),
                        SourceQuery {
                            path: Some("storage.rs".into()),
                            start: Some(1),
                            end: Some(4),
                            ..Default::default()
                        },
                    )
                    .await?;
                for aspect in ["persistence", "concurrency", "recovery"] {
                    service(store)
                        .assessment(
                            "fixture",
                            input.job_id,
                            a.clone(),
                            AspectAssessment {
                                path: "storage.rs".into(),
                                fingerprint: supplied.excerpts[0].fingerprint.clone(),
                                ranges: vec![LineRange { start: 1, end: 3 }],
                                aspect: aspect.into(),
                                disposition: Disposition::Represented,
                                document_ids: vec!["storage".into()],
                                finding_ids: vec![],
                                explanation:
                                    "Observable durability, ownership and replay boundaries".into(),
                            },
                        )
                        .await?;
                }
                service(store).assessment("fixture", input.job_id, a.clone(), AspectAssessment {path:"storage.rs".into(),fingerprint:supplied.excerpts[0].fingerprint.clone(),ranges:vec![LineRange {start:4,end:4}],aspect:"local mechanics".into(),disposition:Disposition::SourceOnly,document_ids:vec![],finding_ids:vec![],explanation:"Helper mechanics are retrievable locally and add no architectural contract".into()}).await?;
                r.areas = vec![DiscoveryArea {
                    id: "storage".into(),
                    responsibility: "Persistence and recovery".into(),
                    questions: vec!["When are writes durable?".into()],
                    aspects: vec![
                        "persistence".into(),
                        "concurrency".into(),
                        "recovery".into(),
                    ],
                    evidence: vec!["storage.rs".into()],
                    owners: vec!["storage".into()],
                    remaining: if self.partial {
                        vec!["Network boundary remains unexamined".into()]
                    } else {
                        vec![]
                    },
                }];
                r.questions = if record.questions.is_empty() {
                    vec![ReadingQuestion {
                        id: "durability".into(),
                        question:
                            "When are writes durable, and which records does recovery replay?"
                                .into(),
                        requirements: vec![
                            "Durability follows sync".into(),
                            "Recovery replays committed records only".into(),
                        ],
                        evidence: supplied.excerpts,
                    }]
                } else {
                    record.questions
                };
                r.ready_for_synthesis = true;
            }
            JobStage::Synthesis => {
                write(
                    &input.working.join("knowledge/storage.md"),
                    &doc(
                        "storage",
                        &["project"],
                        "Writes are durable after sync. One writer owns a transaction. Recovery replays only committed records; uncommitted records are excluded.",
                    ),
                );
                write(
                    &input.working.join("knowledge/README.md"),
                    &doc(
                        "project",
                        &[],
                        "A durable store coordinates one writer and replays committed records during recovery. [Storage](storage.md) explains persistence and ownership.",
                    ),
                );
                r.reviewed_documents = vec!["storage".into(), "project".into()];
                r.complete = !self.partial;
                if self.partial {
                    r.remaining = vec!["Network boundary remains unexamined".into()];
                }
            }
            JobStage::Reader => {
                assert_that!(&input.prompt.contains("Durability follows sync")).is_false();
                assert_that!(&input.prompt.contains("expected-answer")).is_false();
                let root = crate::backend::application::Application::from_store(
                    store.clone(),
                    "http://127.0.0.1:4000".into(),
                )
                .state
                .knowledge_queries
                .query(
                    "fixture",
                    a.clone(),
                    dispatch_types::knowledge::KnowledgeOperation::Root,
                    Default::default(),
                )
                .await?;
                assert_that!(&root.document.is_some()).is_true();
                r.answers = vec![ReaderAnswer {
                    question_id: "durability".into(),
                    answer: "Durable after sync; recovery excludes uncommitted records.".into(),
                    references: vec!["storage".into()],
                    missing: false,
                }];
            }
            JobStage::Review => {
                r.evaluations = vec![AnswerEvaluation {
                    question_id: "durability".into(),
                    correct: true,
                    issues: vec![],
                }];
            }
            _ => bail!("unexpected fixture stage"),
        }
        service(store)
            .report("fixture", input.job_id, a.clone(), r)
            .await?;
        finish_fake(
            crate::backend::runs::tests::service(store, crate::backend::events::UiEventBus::new()),
            run_id,
        )
        .await?;
        Ok(())
    }
}
async fn drive(store: &Store, project_id: i64, job_id: i64, agent: &dyn execution::PassAgent) {
    let shutdown = CancellationToken::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(120),
        service(store).drive_with_agent(shutdown, project_id, job_id, agent),
    )
    .await
    .unwrap()
    .unwrap();
}

struct WaitingAgent {
    started: tokio::sync::Notify,
}

#[async_trait::async_trait]
impl execution::PassAgent for WaitingAgent {
    async fn execute(
        &self,
        cancellation: CancellationToken,
        _input: execution::PassInput,
    ) -> Result<()> {
        self.started.notify_one();
        cancellation.cancelled().await;
        bail!("fixture execution interrupted")
    }
}

#[tokio::test]
async fn shutdown_retains_a_checkpoint_without_recording_user_cancellation() {
    let (_temp, store, project) = fixture().await;
    let jobs = service(&store);
    let job = jobs.start("fixture", request("shutdown")).await.unwrap();
    let shutdown = CancellationToken::new();
    let agent = WaitingAgent {
        started: tokio::sync::Notify::new(),
    };

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let execution = jobs.drive_with_agent(shutdown.clone(), project, job.id, &agent);
        let stop = async {
            agent.started.notified().await;
            shutdown.cancel();
        };
        let (result, ()) = tokio::join!(execution, stop);
        result.unwrap();
    })
    .await
    .unwrap();

    let record = jobs.load(project, job.id).await.unwrap();
    assert_that!(&record.job().status).is_equal_to(JobStatus::Queued);
    assert_that!(&record.job().cancel_requested).is_false();
    assert_that!(&record.job().active_run_id).is_none();
    assert_that!(&record.job().recovery_attempts).is_equal_to(1);
    assert_that!(&record.job().run_ids.len()).is_equal_to(1);
    assert_that!(&record.draft().exists()).is_true();
    assert_that!(&jobs.sessions.active_run_ids_for_project(project).is_empty()).is_true();
}

#[tokio::test]
async fn user_cancellation_stops_the_pass_without_cancelling_the_worker() {
    let (_temp, store, project) = fixture().await;
    let jobs = service(&store);
    let job = jobs.start("fixture", request("cancel-pass")).await.unwrap();
    let shutdown = CancellationToken::new();
    let agent = WaitingAgent {
        started: tokio::sync::Notify::new(),
    };

    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let execution = jobs.drive_with_agent(shutdown.clone(), project, job.id, &agent);
        let cancel = async {
            agent.started.notified().await;
            jobs.action("fixture", job.id, JobAction::Cancel)
                .await
                .unwrap();
        };
        let (result, ()) = tokio::join!(execution, cancel);
        result.unwrap();
    })
    .await
    .unwrap();

    let record = jobs.load(project, job.id).await.unwrap();
    assert_that!(&record.job().status).is_equal_to(JobStatus::Cancelled);
    assert_that!(&record.job().cancel_requested).is_true();
    assert_that!(&record.job().active_run_id).is_none();
    assert_that!(&record.job().recovery_attempts).is_equal_to(0);
    assert_that!(&record.draft().exists()).is_true();
    assert_that!(&shutdown.is_cancelled()).is_false();
    assert_that!(&jobs.sessions.active_run_ids_for_project(project).is_empty()).is_true();
}

#[tokio::test]
async fn discovery_uses_coherent_owners_aspects_reader_and_stable_continuation() {
    let (temp, store, project) = fixture().await;
    let first = service(&store)
        .start("fixture", request("first"))
        .await
        .unwrap();
    drive(&store, project, first.id, &FakeAgent::complete(&store)).await;
    let first = service(&store).detail("fixture", first.id).await.unwrap();
    assert_that!(&first.job.as_ref().unwrap().status).is_equal_to(JobStatus::Completed);
    assert_that!(&first.job.as_ref().unwrap().run_ids.len()).is_equal_to(4);
    assert_that!(&first.changes.len()).is_equal_to(2);
    assert_that!(&first.changes.last().unwrap().path).is_equal_to("README.md");
    assert_that!(&first.assessments.len()).is_equal_to(4);
    assert_that!(&first.quality.reading_estimated_tokens).is_greater_than(0);
    assert_that!(&first.quality.evaluations[0].correct).is_true();
    let root = fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap();
    let second = service(&store)
        .action(
            "fixture",
            first.job.unwrap().id,
            JobAction::Continue {
                request_id: "second".into(),
            },
        )
        .await
        .unwrap();
    drive(&store, project, second.id, &FakeAgent::complete(&store)).await;
    let second = service(&store).detail("fixture", second.id).await.unwrap();
    assert_that!(&second.changes).is_empty();
    assert_that!(&second.quality.question_set_fingerprint)
        .is_equal_to(first.quality.question_set_fingerprint);
    assert_that!(&fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap())
        .is_equal_to(root);
    let coverage = service(&store)
        .coverage(
            "fixture",
            second.job.unwrap().id,
            Some("recovery".into()),
            None,
        )
        .await
        .unwrap();
    assert_that!(&coverage.considered_lines).is_equal_to(3);
    assert_that!(
        &coverage
            .investigation
            .iter()
            .any(|r| r.path == "storage.rs" && r.ranges.contains(&LineRange { start: 4, end: 4 }))
    )
    .is_true();
}

#[tokio::test]
async fn admission_is_idempotent_across_restarts_and_pending_review_blocks_continuation() {
    let (temp, store, project) = fixture().await;
    let jobs = service(&store);
    let (a, b) = tokio::join!(
        jobs.start("fixture", request("same")),
        jobs.start("fixture", request("same"))
    );
    let a = a.unwrap();
    assert_that!(&b.unwrap().id).is_equal_to(a.id);
    assert_that!(
        &service(&store)
            .start("fixture", request("different"))
            .await
            .is_err()
    )
    .is_true();
    drive(
        &store,
        project,
        a.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    assert_that!(
        &service(&store)
            .detail("fixture", a.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .status
    )
    .is_equal_to(JobStatus::AwaitingReview);
    assert_that!(&temp.path().join("project/knowledge/README.md").exists()).is_false();
    let reopened = Store::open_with_max_connections(store.path().to_path_buf(), 1)
        .await
        .unwrap();
    assert_that!(
        &service(&reopened)
            .start("fixture", request("same"))
            .await
            .unwrap()
            .id
    )
    .is_equal_to(a.id);
    assert_that!(
        &service(&reopened)
            .start("fixture", request("another"))
            .await
            .is_err()
    )
    .is_true();
    let applied = service(&reopened)
        .action("fixture", a.id, JobAction::Apply)
        .await
        .unwrap();
    assert_that!(&applied.status).is_equal_to(JobStatus::Completed);
    assert_that!(
        &service(&reopened)
            .action("fixture", a.id, JobAction::Apply)
            .await
            .unwrap()
            .outcome
    )
    .starts_with("Applied");
}

#[tokio::test]
async fn automatic_recovery_is_bounded_and_user_cancellation_never_resumes() {
    let (_temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("retry"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(3),
            partial: false,
        },
    )
    .await;
    let r = service(&store).detail("fixture", job.id).await.unwrap();
    assert_that!(&r.job.unwrap().recovery_attempts).is_equal_to(3);
    let job = service(&store)
        .start("fixture", request("cancel"))
        .await
        .unwrap();
    service(&store)
        .action("fixture", job.id, JobAction::Cancel)
        .await
        .unwrap();
    service(&store).recover().await.unwrap();
    assert_that!(
        &service(&store)
            .detail("fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .status
    )
    .is_equal_to(JobStatus::Cancelled);
}

#[tokio::test]
async fn source_changes_hunks_exclusions_and_stale_publication_preserve_edits() {
    let (temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("first"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    write(
        &temp.path().join("project/storage.rs"),
        &SOURCE.replace("One writer", "Several writers"),
    );
    let c = service(&store)
        .coverage("fixture", job.id, None, None)
        .await
        .unwrap();
    assert_that!(
        &c.investigation
            .iter()
            .any(|r| r.reason == "added or modified hunk"
                && r.ranges == vec![LineRange { start: 2, end: 2 }])
    )
    .is_true();
    assert_that!(
        &service(&store)
            .action("fixture", job.id, JobAction::Apply)
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&temp.path().join("project/knowledge/README.md").exists()).is_false();
    write(&temp.path().join("project/.dispatchignore"), "storage.rs\n");
    let c = service(&store)
        .coverage("fixture", job.id, None, None)
        .await
        .unwrap();
    assert_that!(&c.files.iter().any(|f| f.path == "storage.rs")).is_false();
    assert_that!(
        &c.investigation
            .iter()
            .any(|r| r.reason.contains("excluded"))
    )
    .is_true();
}

#[tokio::test]
async fn publication_recovery_is_root_last_and_preserves_conflicts() {
    let (temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("partial"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    let mut record = service(&store).load(project, job.id).await.unwrap();
    let changes = record.detail.changes.clone();
    atomic_json(
        &Path::new(&record.artifact_dir).join("publication.json"),
        &serde_json::json!({"changes":changes,"applied":0}),
    )
    .unwrap();
    write(
        &temp.path().join("project/knowledge/storage.md"),
        record.detail.changes[0].after.as_ref().unwrap(),
    );
    write(
        &temp.path().join("project/knowledge/README.md"),
        "External root edit",
    );
    record.job_mut().stage = JobStage::Publication;
    service(&store).persist(&mut record).await.unwrap();
    service(&store).recover().await.unwrap();
    assert_that!(&fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap())
        .is_equal_to("External root edit");
    assert_that!(
        &service(&store)
            .detail("fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .outcome
    )
    .contains("conflict");
    fs::remove_file(temp.path().join("project/knowledge/README.md")).unwrap();
    service(&store).recover().await.unwrap();
    assert_that!(
        &service(&store)
            .detail("fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .outcome
    )
    .starts_with("Applied");
}

#[tokio::test]
async fn shared_refinement_parents_require_review_along_both_paths() {
    let (_temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("parents"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    let mut record = service(&store).load(project, job.id).await.unwrap();
    write(
        &record.draft().join("knowledge/left.md"),
        &doc("left", &["project"], "Left responsibility."),
    );
    write(
        &record.draft().join("knowledge/right.md"),
        &doc("right", &["project"], "Right responsibility."),
    );
    write(
        &record.draft().join("knowledge/storage.md"),
        &doc("storage", &["left", "right"], "Shared storage contract."),
    );
    record.reviewed_documents = vec!["storage".into(), "left".into(), "project".into()];
    publication::runtime::candidate(&mut record).unwrap();
    assert_that!(
        &record
            .detail
            .validation
            .iter()
            .any(|s| s.contains("right") && s.contains("review"))
    )
    .is_true();
    record.reviewed_documents.push("right".into());
    publication::runtime::candidate(&mut record).unwrap();
    assert_that!(&record.detail.validation).is_empty();
}

async fn api_server(store: &Store) -> (String, tokio::task::JoinHandle<()>) {
    let application = crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    );
    let state = application.state.clone();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = crate::backend::api::router::<()>().layer(axum::Extension(state));
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (url, task)
}
#[tokio::test]
async fn http_endpoints_enforce_project_run_scope_and_supplied_evidence() {
    let (_temp, store, project) = fixture().await;
    let (url, server) = api_server(&store).await;
    let client = reqwest::Client::new();
    let endpoint = format!("{url}/api/projects/fixture/knowledge/jobs");
    let job: KnowledgeJob = client
        .post(&endpoint)
        .json(&request("http"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let duplicate: KnowledgeJob = client
        .post(&endpoint)
        .json(&request("http"))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_that!(&duplicate.id).is_equal_to(job.id);
    let mut record = service(&store).load(project, job.id).await.unwrap();
    fs::create_dir_all(record.inputs()).unwrap();
    record.files = sources::capture(
        Path::new(&record.workspace),
        &record.directory,
        &record.inputs(),
    )
    .unwrap();
    let run = allocate_fixture(&store, "fixture", job.id, 60)
        .await
        .unwrap();
    record.job_mut().status = JobStatus::Running;
    record.job_mut().stage = JobStage::Discovery;
    record.job_mut().active_run_id = Some(run.id);
    record.job_mut().run_ids.push(run.id);
    service(&store).persist(&mut record).await.unwrap();
    let source_url = format!(
        "{endpoint}/{}/source/read?path=storage.rs&start=1&end=2",
        job.id
    );
    assert_that!(
        &client
            .get(&source_url)
            .send()
            .await
            .unwrap()
            .status()
            .is_client_error()
    )
    .is_true();
    let agent = client
        .get(&source_url)
        .header("x-dispatch-agent-id", format!("dispatch-run-{}", run.id))
        .header("x-dispatch-agent-run-id", run.id);
    let excerpt: SourceResponse = agent
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_that!(&excerpt.excerpts[0].range).is_equal_to(LineRange { start: 1, end: 2 });
    let a = attribution(run.id);
    let invalid = AspectAssessment {
        path: "storage.rs".into(),
        fingerprint: excerpt.excerpts[0].fingerprint.clone(),
        ranges: vec![LineRange { start: 1, end: 3 }],
        aspect: "recovery".into(),
        disposition: Disposition::SourceOnly,
        document_ids: vec![],
        finding_ids: vec![],
        explanation: "Unjustified read".into(),
    };
    assert_that!(
        &service(&store)
            .assessment("fixture", job.id, a.clone(), invalid)
            .await
            .is_err()
    )
    .is_true();
    let blocked = client
        .get(format!("{endpoint}/{}", job.id))
        .header("x-dispatch-agent-id", format!("dispatch-run-{}", run.id))
        .header("x-dispatch-agent-run-id", run.id)
        .send()
        .await
        .unwrap();
    assert_that!(&blocked.status().is_client_error()).is_true();
    let wrong = client
        .get(source_url)
        .header("x-dispatch-agent-id", "dispatch-run-999")
        .header("x-dispatch-agent-run-id", run.id)
        .send()
        .await
        .unwrap();
    assert_that!(&wrong.status().is_client_error()).is_true();
    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "launches real Codex passes against an isolated fixture; run with just knowledge-smoke-test"]
async fn real_agent_smoke_test() {
    let (temp, store, project) = fixture().await;
    fs::remove_file(temp.path().join("project/storage.rs")).unwrap();
    write(
        &temp.path().join("project/log.py"),
        "import json\nimport os\n\ndef commit(path, value):\n    with open(path, 'a') as stream:\n        stream.write(json.dumps({'committed': True, 'value': value}) + '\\n')\n        stream.flush()\n        os.fsync(stream.fileno())\n\ndef recover(path):\n    with open(path) as stream:\n        for line in stream:\n            try:\n                record = json.loads(line)\n            except ValueError:\n                continue\n            if record.get('committed'):\n                yield record['value']\n",
    );
    store
        .db()
        .execute(statement(
            "UPDATE projects SET default_agent_reasoning_effort='medium' WHERE id=?",
            vec![project.into()],
        ))
        .await
        .unwrap();
    let (url, server) = api_server(&store).await;
    let mut request = request("real-smoke");
    request.budget_seconds = 600;
    request.context="Initialize this tiny file-log fixture. Read its complete source through the scoped interface, retain durability and recovery exceptions without inventing concurrency guarantees. Use one or two documents. Derive one ordinary-plus-exception reading question and synthesize early. Keep this smoke test brief.".into();
    let job = service(&store).start("fixture", request).await.unwrap();
    let shutdown = CancellationToken::new();
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(660),
        crate::backend::application::Application::from_store(store.clone(), url)
            .state
            .jobs
            .drive(shutdown, project, job.id),
    )
    .await;
    let record = service(&store).detail("fixture", job.id).await.unwrap();
    println!(
        "Real knowledge smoke result: {}",
        serde_json::to_string_pretty(&record).unwrap()
    );
    // Retain the real provider evidence even if validation fails.
    let evidence = std::env::temp_dir().join("dispatch-knowledge-real-smoke.json");
    fs::write(&evidence, serde_json::to_vec_pretty(&record).unwrap()).unwrap();
    server.abort();
    assert_that!(&outcome.is_ok()).is_true();
    assert_that!(&outcome.unwrap().is_ok()).is_true();
    assert_that!(&record.job.as_ref().unwrap().run_ids.len()).is_greater_or_equal_to(4);
    assert_that!(&record.assessments.is_empty()).is_false();
    assert_that!(&record.quality.answers.is_empty()).is_false();
    assert_that!(&record.quality.evaluations.is_empty()).is_false();
    assert_that!(
        &record
            .quality
            .evaluations
            .iter()
            .all(|e| e.correct && e.issues.is_empty())
    )
    .is_true();
    assert_that!(&matches!(
        record.job.unwrap().status,
        JobStatus::Completed | JobStatus::AwaitingReview
    ))
    .is_true();
}

#[tokio::test]
async fn admitted_workspace_writers_block_publication_until_their_run_finishes() {
    let (temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("writer"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    let run = allocate_fixture(&store, "fixture", job.id, 60)
        .await
        .unwrap();
    let run_id = run.id;
    let mut active: AgentRunActiveModel = AgentRun::find_by_id(run.id)
        .one(store.db().as_ref())
        .await
        .unwrap()
        .unwrap()
        .into();
    active.mutability = Set("mutating".into());
    active.update(store.db().as_ref()).await.unwrap();
    assert_that!(
        &service(&store)
            .action("fixture", job.id, JobAction::Apply)
            .await
            .unwrap_err()
            .to_string()
    )
    .contains("workspace writers");
    assert_that!(&temp.path().join("project/knowledge/README.md").exists()).is_false();
    finish_fake(
        crate::backend::runs::tests::service(&store, crate::backend::events::UiEventBus::new()),
        run_id,
    )
    .await
    .unwrap();
    assert_that!(
        &service(&store)
            .action("fixture", job.id, JobAction::Apply)
            .await
            .unwrap()
            .outcome
    )
    .starts_with("Applied");
}

#[tokio::test]
async fn interrupted_analysis_resumes_with_charged_budget_and_never_weakens_design() {
    let (temp, store, project) = fixture().await;
    let accepted = doc(
        "project",
        &[],
        "Recovery must preserve every accepted write, including after cancellation.",
    );
    write(&temp.path().join("project/knowledge/README.md"), &accepted);
    let job = service(&store)
        .start("fixture", request("restart"))
        .await
        .unwrap();
    let mut record = service(&store).load(project, job.id).await.unwrap();
    fs::create_dir_all(record.inputs()).unwrap();
    record.files = sources::capture(
        Path::new(&record.workspace),
        &record.directory,
        &record.inputs(),
    )
    .unwrap();
    let run = allocate_fixture(&store, "fixture", job.id, 60)
        .await
        .unwrap();
    record.job_mut().active_run_id = Some(run.id);
    record.job_mut().run_ids.push(run.id);
    record.job_mut().active_millis = 9000;
    record.job_mut().stage = JobStage::Discovery;
    record.job_mut().status = JobStatus::Running;
    record.detail.findings.push(KnowledgeFinding {id:"recovery-gap".into(),explanation:"Code does not preserve uncommitted writes after cancellation; accepted design remains authoritative".into(),references:vec!["project".into(),"storage.rs".into()],consequential:true});
    service(&store).persist(&mut record).await.unwrap();
    service(&store).recover().await.unwrap();
    let r = service(&store).detail("fixture", job.id).await.unwrap();
    assert_that!(&r.job.as_ref().unwrap().status).is_equal_to(JobStatus::Queued);
    assert_that!(&r.job.as_ref().unwrap().recovery_attempts).is_equal_to(1);
    assert_that!(&r.job.unwrap().active_millis).is_equal_to(9000);
    assert_that!(&fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap())
        .is_equal_to(accepted);
    assert_that!(&r.findings[0].consequential).is_true();
}

#[tokio::test]
async fn fixed_question_baseline_and_exception_failures_cannot_be_hidden_by_compression() {
    let (_temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("quality"))
        .await
        .unwrap();
    drive(&store, project, job.id, &FakeAgent::complete(&store)).await;
    let mut record = service(&store).load(project, job.id).await.unwrap();
    let baseline = record.detail.quality.question_set_fingerprint.clone();
    record.detail.quality.reading_estimated_tokens = 1;
    record.detail.quality.evaluations[0].correct = false;
    record.detail.quality.evaluations[0].issues =
        vec!["The shorter answer omits the uncommitted-record exception".into()];
    assert_that!(
        &evaluation::quality_issues(&record)
            .iter()
            .any(|issue| issue.contains("did not pass"))
    )
    .is_true();
    evaluation::freeze_questions(&mut record).unwrap();
    assert_that!(&record.detail.quality.question_set_fingerprint).is_equal_to(baseline);
    assert_that!(&estimated_tokens("é🙂".as_bytes())).is_equal_to(2);
}

#[tokio::test]
async fn application_mode_is_a_project_default_and_does_not_enable_recurring_jobs() {
    let (_temp, store, _project) = fixture().await;
    assert_that!(
        &service(&store)
            .settings("fixture")
            .await
            .unwrap()
            .application_mode
    )
    .is_equal_to(ApplicationMode::Review);
    service(&store)
        .save_settings(
            "fixture",
            KnowledgeSettings {
                application_mode: ApplicationMode::Automatic,
            },
        )
        .await
        .unwrap();
    assert_that!(&service(&store).list("fixture").await.unwrap()).is_empty();
    let job = service(&store)
        .start("fixture", request("mode"))
        .await
        .unwrap();
    assert_that!(&job.application_mode).is_equal_to(ApplicationMode::Automatic);
    service(&store)
        .action("fixture", job.id, JobAction::Cancel)
        .await
        .unwrap();
    let mut request = request("override");
    request.application_mode = Some(ApplicationMode::Review);
    assert_that!(
        &service(&store)
            .start("fixture", request)
            .await
            .unwrap()
            .application_mode
    )
    .is_equal_to(ApplicationMode::Review);
}

#[tokio::test]
async fn stale_and_superseded_assessments_remain_history_without_authorizing_new_knowledge() {
    let (_temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("assessment-history"))
        .await
        .unwrap();
    drive(&store, project, job.id, &FakeAgent::complete(&store)).await;
    let mut record = service(&store).load(project, job.id).await.unwrap();
    let mut assessment = record.detail.assessments[0].clone();
    let old_len = record.detail.assessments.len();
    assessment.assessment.disposition = Disposition::SourceOnly;
    assessment.assessment.document_ids.clear();
    assessment.assessment.explanation =
        "Superseding the same evidence with a justified source-only disposition".into();
    record.detail.assessments.push(assessment.clone());
    let active = sources::current_assessments(&record);
    assert_that!(&active.len()).is_equal_to(old_len);
    assert_that!(&active.contains(&&assessment)).is_true();
    assert_that!(&record.detail.assessments.len()).is_equal_to(old_len + 1);
    record.files[0].fingerprint = "changed".into();
    assert_that!(&sources::current_assessments(&record).is_empty()).is_true();
    assert_that!(
        &evaluation::quality_issues(&record)
            .iter()
            .any(|s| s.contains("No source aspects"))
    )
    .is_true();
}

#[tokio::test]
async fn exhausting_provider_recovery_without_a_checkpoint_is_a_failed_job() {
    let (_temp, store, project) = fixture().await;
    let job = service(&store)
        .start("fixture", request("provider-unavailable"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            store: &store,
            failures: AtomicUsize::new(10),
            partial: false,
        },
    )
    .await;
    let job = service(&store)
        .detail("fixture", job.id)
        .await
        .unwrap()
        .job
        .unwrap();
    assert_that!(&job.status).is_equal_to(JobStatus::Failed);
    assert_that!(&job.recovery_attempts).is_equal_to(3);
    assert_that!(&job.run_ids.len()).is_equal_to(4);
}

#[tokio::test]
async fn linked_bounded_jobs_preserve_large_inventory_and_remaining_discovery() {
    let (temp, store, project) = fixture().await;
    for i in 0..120 {
        write(
            &temp
                .path()
                .join(format!("project/network/component-{i}.rs")),
            "// An unexamined network boundary.\n",
        );
    }
    let mut previous = None;
    let mut baseline: Option<String> = None;
    for pass in 0..3 {
        let mut request = request(&format!("large-{pass}"));
        request.budget_seconds = 300;
        request.previous_job_id = previous;
        let job = service(&store).start("fixture", request).await.unwrap();
        drive(
            &store,
            project,
            job.id,
            &FakeAgent {
                store: &store,
                failures: AtomicUsize::new(0),
                partial: true,
            },
        )
        .await;
        let record = service(&store).load(project, job.id).await.unwrap();
        assert_that!(&record.files.len()).is_greater_or_equal_to(121);
        assert_that!(&record.detail.areas[0].remaining.is_empty()).is_false();
        assert_that!(&record.job().run_ids.len()).is_equal_to(4);
        if let Some(baseline) = &baseline {
            assert_that!(&record.detail.quality.question_set_fingerprint)
                .is_equal_to(baseline.clone());
            assert_that!(&record.detail.changes.is_empty()).is_true();
            assert_that!(&record.detail.assessments.iter().any(|a| a.job_id != job.id)).is_true();
        } else {
            baseline = Some(record.detail.quality.question_set_fingerprint.clone());
            assert_that!(&record.detail.changes.len()).is_equal_to(2);
            service(&store)
                .action("fixture", job.id, JobAction::Apply)
                .await
                .unwrap();
        }
        previous = Some(job.id);
    }
    assert_that!(&service(&store).list("fixture").await.unwrap().len()).is_equal_to(3);
}

pub(crate) fn service(store: &Store) -> std::sync::Arc<super::service::JobService> {
    crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    )
    .state
    .jobs
    .clone()
}
fn statement(sql: &str, values: Vec<Value>) -> Statement {
    Statement::from_sql_and_values(DbBackend::Sqlite, sql, values)
}
async fn allocate_fixture(
    store: &Store,
    project: &str,
    job_id: i64,
    timeout_seconds: u64,
) -> Result<AgentRunView> {
    let app = crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    );
    let project_id = ProjectRepository::new(store.db()).id(project).await?;
    app.state
        .runs
        .create(
            project_id,
            CreateRunConfig {
                tool: AgentToolName::Codex,
                mutability: AutomationRunMutability::ReadOnly,
                trigger: None,
                personality_revision_id: None,
                effective_timeout_seconds: timeout_seconds,
                effective_concurrency_group: None,
                run_kind: AgentRunKind::Task,
                purpose: AgentRunPurposeV1::KnowledgeCycle,
                knowledge_job_id: Some(job_id),
                launch_target: &AgentLaunchTargetV1::none(),
            },
        )
        .await
}
async fn finish_fake(
    runs: std::sync::Arc<crate::backend::runs::service::RunService>,
    id: i64,
) -> Result<()> {
    let run = runs.find(id).await?.unwrap();
    runs.finish(
        run,
        AgentRunStatus::Completed,
        Some(0),
        "Deterministic fixture pass".into(),
    )
    .await?;
    Ok(())
}

#[tokio::test]
async fn job_and_pass_allocation_roll_back_without_runs_or_success_notifications() {
    let (_temp, store, project) = fixture().await;
    let app = crate::backend::application::Application::from_store(
        store.clone(),
        "http://127.0.0.1:4000".into(),
    );
    store.db().execute_unprepared("CREATE TRIGGER reject_job_initialization BEFORE UPDATE ON knowledge_jobs BEGIN SELECT RAISE(ABORT,'fixture initialization failure'); END").await.unwrap();
    assert_that!(
        &app.state
            .jobs
            .start("fixture", request("atomic-job"))
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.jobs.list("fixture").await.unwrap()).is_empty();
    store
        .db()
        .execute_unprepared("DROP TRIGGER reject_job_initialization")
        .await
        .unwrap();
    let job = app
        .state
        .jobs
        .start("fixture", request("atomic-job"))
        .await
        .unwrap();
    app.state.jobs.prepare(project, job.id).await.unwrap();
    let before = app.state.jobs.detail("fixture", job.id).await.unwrap();
    let mut events = app.state.events.subscribe();
    store.db().execute_unprepared("CREATE TRIGGER reject_pass_link BEFORE UPDATE ON knowledge_jobs WHEN json_extract(NEW.payload,'$.detail.job.active_run_id') IS NOT NULL BEGIN SELECT RAISE(ABORT,'fixture pass link failure'); END").await.unwrap();
    assert_that!(
        &app.state
            .jobs
            .begin_pass(project, job.id, 60)
            .await
            .is_err()
    )
    .is_true();
    assert_that!(&app.state.jobs.detail("fixture", job.id).await.unwrap()).is_equal_to(before);
    assert_that!(
        &app.state
            .runs
            .list_for_project(project, false, None)
            .await
            .unwrap()
    )
    .is_empty();
    assert_that!(&events.try_recv().is_err()).is_true();
    store
        .db()
        .execute_unprepared("DROP TRIGGER reject_pass_link")
        .await
        .unwrap();
    let (record, run, _, _) = app
        .state
        .jobs
        .begin_pass(project, job.id, 60)
        .await
        .unwrap()
        .unwrap();
    assert_that!(&record.job().active_run_id).is_equal_to(Some(run.id));
    assert_that!(&run.work_item_id).is_none();
    assert_that!(&run.knowledge_job_id).is_equal_to(Some(job.id));
    assert_that!(
        &app.state
            .jobs
            .detail("fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .run_ids
    )
    .is_equal_to(vec![run.id]);
}

#[tokio::test]
async fn independent_job_coordinators_and_project_deletion_batches_are_isolated() {
    let (_first_temp, first_store, first_project) = fixture().await;
    let (_second_temp, second_store, _) = fixture().await;
    let first = service(&first_store);
    let second = service(&second_store);
    let lock = first.coordination.lock().await;
    let job = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        second.start("fixture", request("independent")),
    )
    .await
    .unwrap()
    .unwrap();
    assert_that!(&job.status).is_equal_to(JobStatus::Queued);
    drop(lock);
    let first_job = first.start("fixture", request("deletion")).await.unwrap();
    first_store.db().execute_unprepared("CREATE TRIGGER reject_job_cancellation BEFORE UPDATE ON knowledge_jobs BEGIN SELECT RAISE(ABORT,'fixture deletion failure'); END").await.unwrap();
    assert_that!(&first.stop_project(first_project).await.is_err()).is_true();
    assert_that!(
        &first
            .detail("fixture", first_job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .status
    )
    .is_equal_to(JobStatus::Queued);
    first_store
        .db()
        .execute_unprepared("DROP TRIGGER reject_job_cancellation")
        .await
        .unwrap();
    first.stop_project(first_project).await.unwrap();
    assert_that!(
        &first
            .detail("fixture", first_job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .status
    )
    .is_equal_to(JobStatus::Cancelled);
    assert_that!(
        &second
            .detail("fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .status
    )
    .is_equal_to(JobStatus::Queued);
}

#[tokio::test]
async fn run_cancellation_form_persists_knowledge_cancellation_before_signalling_execution() {
    use crate::backend::execution::sessions::ProcessSessionStart;
    let (_temp, store, project) = fixture().await;
    let app =
        crate::backend::application::Application::from_store(store, "http://127.0.0.1:4000".into());
    let job = app
        .state
        .jobs
        .start("fixture", request("cancel-form"))
        .await
        .unwrap();
    app.state.jobs.prepare(project, job.id).await.unwrap();
    let (_, run, _, _) = app
        .state
        .jobs
        .begin_pass(project, job.id, 60)
        .await
        .unwrap()
        .unwrap();
    let registration = app.state.sessions.begin(
        ProcessSessionStart {
            run_id: run.id,
            project_id: project,
            project_name: "fixture".into(),
            tool_name: "codex".into(),
            command: "fixture".into(),
            working_dir: "fixture".into(),
        },
        &Default::default(),
    );
    assert_that!(&app.state.run_control.cancel("other", run.id).await.is_err()).is_true();
    assert_that!(&registration.cancellation_requested()).is_false();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "http://{}/projects/fixture/automation/runs/{}/cancel",
        listener.local_addr().unwrap(),
        run.id
    );
    let router = crate::backend::runs::transport::forms::routes::<()>()
        .layer(axum::Extension(app.state.clone()));
    let server = tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap();
    let response = client
        .post(url)
        .form(&[("return_to", "/projects/fixture")])
        .send()
        .await
        .unwrap();
    assert_that!(&response.status().is_redirection()).is_true();
    assert_that!(
        &app.state
            .jobs
            .detail("fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .cancel_requested
    )
    .is_true();
    assert_that!(&registration.cancellation_requested()).is_true();
    app.state.sessions.finish(run.id);
    server.abort();
}
