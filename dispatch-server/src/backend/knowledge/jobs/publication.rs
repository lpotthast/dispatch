//! Checked multi-file publication, root last, with a durable recovery journal.
use super::*;
use crate::backend::knowledge::{discovery::Discovery, documents::Index, editing};
use dispatch_types::knowledge::KnowledgeSaveRequest;
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Journal {
    changes: Vec<CandidateChange>,
    applied: usize,
}

pub(super) fn candidate(record: &mut Record) -> Result<()> {
    let draft = record.draft();
    let index = Index::read(&draft, &record.directory)?;
    record.detail.validation = index
        .diagnostics
        .iter()
        .filter(|d| d.code != "size_review")
        .map(|d| format!("{}: {}: {}", d.path, d.code, d.message))
        .collect();
    let prefix = format!("{}/", record.directory);
    let mut paths: BTreeSet<_> = record
        .files
        .iter()
        .filter_map(|f| {
            f.path
                .strip_prefix(&prefix)
                .filter(|p| p.ends_with(".md"))
                .map(str::to_owned)
        })
        .collect();
    paths.extend(index.documents.keys().cloned());
    let mut changes = Vec::new();
    for path in paths {
        let before = read_optional(&record.inputs().join(&record.directory).join(&path))?;
        let after = index.documents.get(&path).map(|d| d.markdown.clone());
        if before != after {
            changes.push(CandidateChange {
                path,
                before,
                after,
            });
        }
    }
    changes.sort_by(|a, b| {
        (a.path == "README.md")
            .cmp(&(b.path == "README.md"))
            .then(a.path.cmp(&b.path))
    });
    // Inspect all affected ancestor paths, including parents shared by several branches.
    let old_index = Index::read(&record.inputs(), &record.directory)?;
    let mut required = BTreeSet::new();
    for index in [&old_index, &index] {
        let mut queue: Vec<_> = changes.iter().map(|c| c.path.clone()).collect();
        let mut seen = BTreeSet::new();
        while let Some(path) = queue.pop() {
            if !seen.insert(path.clone()) {
                continue;
            }
            if let Some(doc) = index.documents.get(&path) {
                required.insert(doc.metadata.id.clone().unwrap_or(path));
                queue.extend(doc.parents.clone());
                for dependent in &doc.dependents {
                    if let Some(d) = index.documents.get(dependent) {
                        required.insert(d.metadata.id.clone().unwrap_or(dependent.clone()));
                    }
                }
            }
        }
    }
    for id in required {
        if !record.reviewed_documents.contains(&id) {
            record.detail.validation.push(format!(
                "Affected document '{id}' lacks ancestor/dependency review"
            ));
        }
    }
    for a in sources::current_assessments(record)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>()
    {
        if a.assessment.disposition == Disposition::Represented {
            for id in &a.assessment.document_ids {
                if !index
                    .documents
                    .values()
                    .any(|d| d.metadata.id.as_ref() == Some(id))
                {
                    record
                        .detail
                        .validation
                        .push(format!("Assessment refers to missing document '{id}'"));
                }
            }
        }
        for id in &a.assessment.finding_ids {
            if !record.detail.findings.iter().any(|f| &f.id == id) {
                record
                    .detail
                    .validation
                    .push(format!("Assessment refers to missing finding '{id}'"));
            }
        }
    }
    record.detail.changes = changes;
    Ok(())
}
fn read_optional(path: &Path) -> Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(report!(e).into_dynamic()),
    }
}

pub(super) async fn apply(store: &Store, project_id: i64, id: i64) -> Result<KnowledgeJob> {
    // Admission is shared with coding launches. A writer cannot enter between the count and write.
    let _admission = store.lock_runtime_admission().await;
    let _lock = store.lock_knowledge_jobs().await;
    let mut record = load(store, project_id, id).await?;
    if record.job().status == JobStatus::Completed && record.job().outcome.starts_with("Applied") {
        return Ok(record.job().clone());
    }
    if record.job().stage != JobStage::Publication
        && (record.job().status != JobStatus::AwaitingReview || record.job().cancel_requested)
    {
        bail!("only an active pending proposal can be applied");
    }
    if crate::backend::automation_admission::running_counts_for_project_id(store, project_id)
        .await?
        .mutating
        > 0
    {
        bail!("publication is waiting for workspace writers");
    }
    let project = crate::backend::entities::project::Project::find_by_id(project_id)
        .one(store.db().as_ref())
        .await?
        .ok_or_else(|| report!("project was deleted"))?;
    if project.path.as_deref() != Some(&record.workspace)
        || project.knowledge_directory != record.directory
    {
        bail!("project working copy or knowledge directory changed; refresh this proposal");
    }
    if record.job().stage == JobStage::Publication {
        recover(store, &mut record).await?;
        return Ok(record.job().clone());
    }
    let copy = record.clone();
    tokio::task::spawn_blocking(move || {
        sources::check_fresh(
            Path::new(&copy.workspace),
            &copy.directory,
            &copy.inputs(),
            &copy.files,
        )
    })
    .await??;
    let copy = record.clone();
    let checked = tokio::task::spawn_blocking(move || {
        let mut copy = copy;
        candidate(&mut copy)?;
        Ok::<_, rootcause::Report>(copy)
    })
    .await??;
    if !checked.detail.validation.is_empty() {
        bail!(
            "candidate has structural or propagation defects: {}",
            checked.detail.validation.join("; ")
        );
    }
    if checked.detail.changes != record.detail.changes {
        bail!("candidate changed after review; refresh the job before applying");
    }
    record.job_mut().stage = JobStage::Publication;
    persist(store, &mut record).await?;
    // A cancellation arriving here persists while the bounded publication finishes or recovers.
    drop(_lock);
    let copy = record.clone();
    let result = tokio::task::spawn_blocking(move || publish_files(&copy)).await?;
    let _lock = store.lock_knowledge_jobs().await;
    record = load(store, project_id, id).await?;
    match result {
        Ok(()) => {
            record.job_mut().status = JobStatus::Completed;
            record.job_mut().stage = JobStage::Finished;
            record.job_mut().outcome = if record.job().cancel_requested {
                "Applied changes; cancellation arrived during publication"
            } else {
                "Applied changes"
            }
            .into();
        }
        Err(error) => {
            record.job_mut().status = JobStatus::AwaitingReview;
            record.job_mut().outcome = format!("Publication requires recovery: {error}");
        }
    }
    persist(store, &mut record).await?;
    Ok(record.job().clone())
}
use sea_orm::EntityTrait;
pub(super) fn publish_files(record: &Record) -> Result<()> {
    let _writer = editing::WRITES
        .lock()
        .map_err(|_| report!("knowledge writer is unavailable"))?;
    let path = Path::new(&record.artifact_dir).join("publication.json");
    let mut journal = if path.exists() {
        serde_json::from_slice::<Journal>(&fs::read(&path)?)?
    } else {
        let journal = Journal {
            changes: record.detail.changes.clone(),
            applied: 0,
        };
        atomic_json(&path, &journal)?;
        journal
    };
    let workspace = Path::new(&record.workspace);
    for i in journal.applied..journal.changes.len() {
        let change = &journal.changes[i];
        super::super::normalize_knowledge_directory(&change.path)?;
        Discovery::check_file(workspace, &record.directory, &change.path)?;
        let destination = workspace.join(&record.directory).join(&change.path);
        let current = read_optional(&destination)?;
        if current != change.after {
            if current != change.before {
                bail!(
                    "external edit at '{}' is preserved; resolve the publication conflict",
                    change.path
                );
            }
            if let Some(after) = &change.after {
                editing::save_file_locked(
                    workspace,
                    &record.directory,
                    &KnowledgeSaveRequest {
                        path: change.path.clone(),
                        expected_fingerprint: change.before.as_ref().map(|s| hash(s.as_bytes())),
                        markdown: after.clone(),
                    },
                )?;
            } else {
                // Byte comparison above prevents deleting an independently changed document.
                fs::remove_file(&destination)?;
            }
            fs::File::open(destination.parent().unwrap())?.sync_all()?;
        }
        journal.applied = i + 1;
        atomic_json(&path, &journal)?;
    }
    Ok(())
}

pub(super) async fn recover(store: &Store, record: &mut Record) -> Result<()> {
    let copy = record.clone();
    match tokio::task::spawn_blocking(move || publish_files(&copy)).await? {
        Ok(()) => {
            record.job_mut().status = JobStatus::Completed;
            record.job_mut().stage = JobStage::Finished;
            record.job_mut().outcome = "Applied changes; publication recovered".into();
        }
        Err(e) => {
            record.job_mut().status = JobStatus::AwaitingReview;
            record.job_mut().outcome = format!("Publication conflict preserved: {e}");
        }
    }
    persist(store, record).await
}
