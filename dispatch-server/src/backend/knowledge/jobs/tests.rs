use super::*;
use crate::backend::{
    automation::knowledge_execution, entities::agent_run::AgentRunActiveModel,
    projects::CreateProject,
};
use assertr::prelude::*;
use sea_orm::{ActiveModelTrait, ActiveValue::Set};
use std::sync::atomic::{AtomicUsize, Ordering};
use tempfile::TempDir;
use tokio::sync::watch;
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
    let temp = TempDir::new().unwrap();
    let working = temp.path().join("project");
    fs::create_dir_all(&working).unwrap();
    write(&working.join("storage.rs"), SOURCE);
    let store = Store::open_with_max_connections(temp.path().join("state/db.sqlite"), 1)
        .await
        .unwrap();
    let project = projects::create_project(
        &store,
        CreateProject {
            name: "fixture".into(),
            display_name: None,
            path: working,
            default_agent_model: None,
            default_agent_reasoning_effort: None,
            system_prompt: None,
            memory: None,
        },
    )
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
async fn attribution(store: &Store, run: i64) -> RequestAttribution {
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        "x-dispatch-agent-id",
        format!("dispatch-run-{run}").parse().unwrap(),
    );
    headers.insert("x-dispatch-agent-run-id", run.to_string().parse().unwrap());
    RequestAttribution::from_knowledge_headers(store, "fixture", &headers)
        .await
        .unwrap()
}
struct FakeAgent {
    failures: AtomicUsize,
    partial: bool,
}
impl FakeAgent {
    fn complete() -> Self {
        Self {
            failures: AtomicUsize::new(0),
            partial: false,
        }
    }
}
#[async_trait::async_trait]
impl runtime::PassAgent for FakeAgent {
    async fn execute(
        &self,
        store: &Store,
        _sessions: ProcessSessionRegistry,
        _shutdown: watch::Receiver<bool>,
        input: knowledge_execution::PassInput,
    ) -> Result<()> {
        if self
            .failures
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok()
        {
            bail!("fixture transport interruption");
        }
        let run_id = input.run.id;
        let mut active: AgentRunActiveModel = input.run.into();
        active.working_dir = Set(input.working.to_string_lossy().into_owned());
        active.update(store.db().as_ref()).await?;
        let a = attribution(store, run_id).await;
        let project_id = projects::project_id(store, "fixture").await?;
        let record = load(store, project_id, input.job_id).await?;
        let mut r = JobReport {
            summary: "Persistence, ownership and recovery contracts".into(),
            ..Default::default()
        };
        match record.job().stage {
            JobStage::Discovery => {
                let supplied = source_query(
                    store,
                    "fixture",
                    input.job_id,
                    &a,
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
                    assessment(
                        store,
                        "fixture",
                        input.job_id,
                        &a,
                        AspectAssessment {
                            path: "storage.rs".into(),
                            fingerprint: supplied.excerpts[0].fingerprint.clone(),
                            ranges: vec![LineRange { start: 1, end: 3 }],
                            aspect: aspect.into(),
                            disposition: Disposition::Represented,
                            document_ids: vec!["storage".into()],
                            finding_ids: vec![],
                            explanation: "Observable durability, ownership and replay boundaries"
                                .into(),
                        },
                    )
                    .await?;
                }
                assessment(store,"fixture",input.job_id,&a,AspectAssessment {path:"storage.rs".into(),fingerprint:supplied.excerpts[0].fingerprint.clone(),ranges:vec![LineRange {start:4,end:4}],aspect:"local mechanics".into(),disposition:Disposition::SourceOnly,document_ids:vec![],finding_ids:vec![],explanation:"Helper mechanics are retrievable locally and add no architectural contract".into()}).await?;
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
                let root = crate::backend::knowledge::query(
                    store,
                    "fixture",
                    &a,
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
        report(store, "fixture", input.job_id, &a, r).await?;
        knowledge_execution::finish_fake(store, run_id).await?;
        Ok(())
    }
}
async fn drive(store: &Store, project_id: i64, job_id: i64, agent: &dyn runtime::PassAgent) {
    let (_tx, rx) = watch::channel(false);
    tokio::time::timeout(
        std::time::Duration::from_secs(120),
        runtime::drive_with_agent(
            store,
            &ProcessSessionRegistry::new(),
            rx,
            project_id,
            job_id,
            agent,
        ),
    )
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test]
async fn discovery_uses_coherent_owners_aspects_reader_and_stable_continuation() {
    let (temp, store, project) = fixture().await;
    let first = start(&store, "fixture", request("first")).await.unwrap();
    drive(&store, project, first.id, &FakeAgent::complete()).await;
    let first = detail(&store, "fixture", first.id).await.unwrap();
    assert_that!(&first.job.as_ref().unwrap().status).is_equal_to(JobStatus::Completed);
    assert_that!(&first.job.as_ref().unwrap().run_ids.len()).is_equal_to(4);
    assert_that!(&first.changes.len()).is_equal_to(2);
    assert_that!(&first.changes.last().unwrap().path).is_equal_to("README.md");
    assert_that!(&first.assessments.len()).is_equal_to(4);
    assert_that!(&first.quality.reading_estimated_tokens).is_greater_than(0);
    assert_that!(&first.quality.evaluations[0].correct).is_true();
    let root = fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap();
    let second = action(
        &store,
        &ProcessSessionRegistry::new(),
        "fixture",
        first.job.unwrap().id,
        JobAction::Continue {
            request_id: "second".into(),
        },
    )
    .await
    .unwrap();
    drive(&store, project, second.id, &FakeAgent::complete()).await;
    let second = detail(&store, "fixture", second.id).await.unwrap();
    assert_that!(&second.changes).is_empty();
    assert_that!(&second.quality.question_set_fingerprint)
        .is_equal_to(first.quality.question_set_fingerprint);
    assert_that!(&fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap())
        .is_equal_to(root);
    let coverage = coverage(
        &store,
        "fixture",
        second.job.unwrap().id,
        Some("recovery".into()),
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
    let (a, b) = tokio::join!(
        start(&store, "fixture", request("same")),
        start(&store, "fixture", request("same"))
    );
    let a = a.unwrap();
    assert_that!(&b.unwrap().id).is_equal_to(a.id);
    assert_that!(
        &start(&store, "fixture", request("different"))
            .await
            .is_err()
    )
    .is_true();
    drive(
        &store,
        project,
        a.id,
        &FakeAgent {
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    assert_that!(
        &detail(&store, "fixture", a.id)
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
        &start(&reopened, "fixture", request("same"))
            .await
            .unwrap()
            .id
    )
    .is_equal_to(a.id);
    assert_that!(
        &start(&reopened, "fixture", request("another"))
            .await
            .is_err()
    )
    .is_true();
    let applied = action(
        &reopened,
        &ProcessSessionRegistry::new(),
        "fixture",
        a.id,
        JobAction::Apply,
    )
    .await
    .unwrap();
    assert_that!(&applied.status).is_equal_to(JobStatus::Completed);
    assert_that!(
        &action(
            &reopened,
            &ProcessSessionRegistry::new(),
            "fixture",
            a.id,
            JobAction::Apply
        )
        .await
        .unwrap()
        .outcome
    )
    .starts_with("Applied");
}

#[tokio::test]
async fn automatic_recovery_is_bounded_and_user_cancellation_never_resumes() {
    let (_temp, store, project) = fixture().await;
    let job = start(&store, "fixture", request("retry")).await.unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            failures: AtomicUsize::new(3),
            partial: false,
        },
    )
    .await;
    let r = detail(&store, "fixture", job.id).await.unwrap();
    assert_that!(&r.job.unwrap().recovery_attempts).is_equal_to(3);
    let job = start(&store, "fixture", request("cancel")).await.unwrap();
    action(
        &store,
        &ProcessSessionRegistry::new(),
        "fixture",
        job.id,
        JobAction::Cancel,
    )
    .await
    .unwrap();
    runtime::recover(&store).await.unwrap();
    assert_that!(
        &detail(&store, "fixture", job.id)
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
    let job = start(&store, "fixture", request("first")).await.unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    write(
        &temp.path().join("project/storage.rs"),
        &SOURCE.replace("One writer", "Several writers"),
    );
    let c = coverage(&store, "fixture", job.id, None).await.unwrap();
    assert_that!(
        &c.investigation
            .iter()
            .any(|r| r.reason == "added or modified hunk"
                && r.ranges == vec![LineRange { start: 2, end: 2 }])
    )
    .is_true();
    assert_that!(
        &action(
            &store,
            &ProcessSessionRegistry::new(),
            "fixture",
            job.id,
            JobAction::Apply
        )
        .await
        .is_err()
    )
    .is_true();
    assert_that!(&temp.path().join("project/knowledge/README.md").exists()).is_false();
    write(&temp.path().join("project/.dispatchignore"), "storage.rs\n");
    let c = coverage(&store, "fixture", job.id, None).await.unwrap();
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
    let job = start(&store, "fixture", request("partial")).await.unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    let mut record = load(&store, project, job.id).await.unwrap();
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
    persist(&store, &mut record).await.unwrap();
    runtime::recover(&store).await.unwrap();
    assert_that!(&fs::read_to_string(temp.path().join("project/knowledge/README.md")).unwrap())
        .is_equal_to("External root edit");
    assert_that!(
        &detail(&store, "fixture", job.id)
            .await
            .unwrap()
            .job
            .unwrap()
            .outcome
    )
    .contains("conflict");
    fs::remove_file(temp.path().join("project/knowledge/README.md")).unwrap();
    runtime::recover(&store).await.unwrap();
    assert_that!(
        &detail(&store, "fixture", job.id)
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
    let job = start(&store, "fixture", request("parents")).await.unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    let mut record = load(&store, project, job.id).await.unwrap();
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
    publication::candidate(&mut record).unwrap();
    assert_that!(
        &record
            .detail
            .validation
            .iter()
            .any(|s| s.contains("right") && s.contains("review"))
    )
    .is_true();
    record.reviewed_documents.push("right".into());
    publication::candidate(&mut record).unwrap();
    assert_that!(&record.detail.validation).is_empty();
}

async fn api_server(store: &Store) -> (String, tokio::task::JoinHandle<()>) {
    use crate::backend::{
        app_state::AppState, automation_controller::AutomationController,
        project_deletion::ProjectDeletionService,
    };
    let sessions = ProcessSessionRegistry::new();
    let controller = AutomationController::new();
    let state = AppState {
        store: store.clone(),
        sessions: sessions.clone(),
        automation_controller: controller.clone(),
        project_deletion: ProjectDeletionService::new(store.clone(), controller, sessions),
        codex_status: std::sync::Arc::new(tokio::sync::RwLock::new(Default::default())),
        codex_status_refresh: Default::default(),
    };
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
    let mut record = load(&store, project, job.id).await.unwrap();
    fs::create_dir_all(record.inputs()).unwrap();
    record.files = sources::capture(
        Path::new(&record.workspace),
        &record.directory,
        &record.inputs(),
    )
    .unwrap();
    let run = knowledge_execution::allocate(&store, "fixture", job.id, 60)
        .await
        .unwrap();
    record.job_mut().status = JobStatus::Running;
    record.job_mut().stage = JobStage::Discovery;
    record.job_mut().active_run_id = Some(run.id);
    record.job_mut().run_ids.push(run.id);
    persist(&store, &mut record).await.unwrap();
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
    let a = attribution(&store, run.id).await;
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
        &assessment(&store, "fixture", job.id, &a, invalid)
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
    crate::backend::automation::set_server_api_url(url);
    let mut request = request("real-smoke");
    request.budget_seconds = 600;
    request.context="Initialize this tiny file-log fixture. Read its complete source through the scoped interface, retain durability and recovery exceptions without inventing concurrency guarantees. Use one or two documents. Derive one ordinary-plus-exception reading question and synthesize early. Keep this smoke test brief.".into();
    let job = start(&store, "fixture", request).await.unwrap();
    let (_tx, rx) = watch::channel(false);
    let outcome = tokio::time::timeout(
        std::time::Duration::from_secs(660),
        runtime::drive_with_agent(
            &store,
            &ProcessSessionRegistry::new(),
            rx,
            project,
            job.id,
            &runtime::RealAgent,
        ),
    )
    .await;
    let record = detail(&store, "fixture", job.id).await.unwrap();
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
    let job = start(&store, "fixture", request("writer")).await.unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            failures: AtomicUsize::new(0),
            partial: true,
        },
    )
    .await;
    let run = knowledge_execution::allocate(&store, "fixture", job.id, 60)
        .await
        .unwrap();
    let run_id = run.id;
    let mut active: AgentRunActiveModel = run.into();
    active.mutability = Set("mutating".into());
    active.update(store.db().as_ref()).await.unwrap();
    assert_that!(
        &action(
            &store,
            &ProcessSessionRegistry::new(),
            "fixture",
            job.id,
            JobAction::Apply
        )
        .await
        .unwrap_err()
        .to_string()
    )
    .contains("workspace writers");
    assert_that!(&temp.path().join("project/knowledge/README.md").exists()).is_false();
    knowledge_execution::finish_fake(&store, run_id)
        .await
        .unwrap();
    assert_that!(
        &action(
            &store,
            &ProcessSessionRegistry::new(),
            "fixture",
            job.id,
            JobAction::Apply
        )
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
    let job = start(&store, "fixture", request("restart")).await.unwrap();
    let mut record = load(&store, project, job.id).await.unwrap();
    fs::create_dir_all(record.inputs()).unwrap();
    record.files = sources::capture(
        Path::new(&record.workspace),
        &record.directory,
        &record.inputs(),
    )
    .unwrap();
    let run = knowledge_execution::allocate(&store, "fixture", job.id, 60)
        .await
        .unwrap();
    record.job_mut().active_run_id = Some(run.id);
    record.job_mut().run_ids.push(run.id);
    record.job_mut().active_millis = 9000;
    record.job_mut().stage = JobStage::Discovery;
    record.job_mut().status = JobStatus::Running;
    record.detail.findings.push(KnowledgeFinding {id:"recovery-gap".into(),explanation:"Code does not preserve uncommitted writes after cancellation; accepted design remains authoritative".into(),references:vec!["project".into(),"storage.rs".into()],consequential:true});
    persist(&store, &mut record).await.unwrap();
    runtime::recover(&store).await.unwrap();
    let r = detail(&store, "fixture", job.id).await.unwrap();
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
    let job = start(&store, "fixture", request("quality")).await.unwrap();
    drive(&store, project, job.id, &FakeAgent::complete()).await;
    let mut record = load(&store, project, job.id).await.unwrap();
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
    assert_that!(&settings(&store, "fixture").await.unwrap().application_mode)
        .is_equal_to(ApplicationMode::Review);
    save_settings(
        &store,
        "fixture",
        KnowledgeSettings {
            application_mode: ApplicationMode::Automatic,
        },
    )
    .await
    .unwrap();
    assert_that!(&list(&store, "fixture").await.unwrap()).is_empty();
    let job = start(&store, "fixture", request("mode")).await.unwrap();
    assert_that!(&job.application_mode).is_equal_to(ApplicationMode::Automatic);
    action(
        &store,
        &ProcessSessionRegistry::new(),
        "fixture",
        job.id,
        JobAction::Cancel,
    )
    .await
    .unwrap();
    let mut request = request("override");
    request.application_mode = Some(ApplicationMode::Review);
    assert_that!(
        &start(&store, "fixture", request)
            .await
            .unwrap()
            .application_mode
    )
    .is_equal_to(ApplicationMode::Review);
}

#[tokio::test]
async fn stale_and_superseded_assessments_remain_history_without_authorizing_new_knowledge() {
    let (_temp, store, project) = fixture().await;
    let job = start(&store, "fixture", request("assessment-history"))
        .await
        .unwrap();
    drive(&store, project, job.id, &FakeAgent::complete()).await;
    let mut record = load(&store, project, job.id).await.unwrap();
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
    let job = start(&store, "fixture", request("provider-unavailable"))
        .await
        .unwrap();
    drive(
        &store,
        project,
        job.id,
        &FakeAgent {
            failures: AtomicUsize::new(10),
            partial: false,
        },
    )
    .await;
    let job = detail(&store, "fixture", job.id)
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
    let sessions = ProcessSessionRegistry::new();
    let mut previous = None;
    let mut baseline: Option<String> = None;
    for pass in 0..3 {
        let mut request = request(&format!("large-{pass}"));
        request.budget_seconds = 300;
        request.previous_job_id = previous;
        let job = start(&store, "fixture", request).await.unwrap();
        drive(
            &store,
            project,
            job.id,
            &FakeAgent {
                failures: AtomicUsize::new(0),
                partial: true,
            },
        )
        .await;
        let record = load(&store, project, job.id).await.unwrap();
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
            action(&store, &sessions, "fixture", job.id, JobAction::Apply)
                .await
                .unwrap();
        }
        previous = Some(job.id);
    }
    assert_that!(&list(&store, "fixture").await.unwrap().len()).is_equal_to(3);
}
