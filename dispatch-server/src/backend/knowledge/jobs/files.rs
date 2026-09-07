use super::{
    evaluation,
    model::Record,
    publication::runtime::{candidate, publish_files},
    sources,
};
use crate::backend::knowledge::editing::DocumentWriter;
use dispatch_types::knowledge::jobs::*;
use rootcause::Result;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
pub(crate) struct JobFiles {
    root: PathBuf,
    writer: Arc<DocumentWriter>,
}
impl JobFiles {
    pub(crate) fn new(root: PathBuf, writer: Arc<DocumentWriter>) -> Self {
        Self { root, writer }
    }
    pub(crate) fn project_artifacts(&self, project_id: i64) -> PathBuf {
        self.root.join(project_id.to_string())
    }
    pub(crate) fn root_is_absent(&self, workspace: &str, directory: &str) -> bool {
        !Path::new(workspace)
            .join(directory)
            .join("README.md")
            .exists()
    }
    pub(super) async fn coverage(
        &self,
        record: Record,
        aspect: Option<String>,
    ) -> Result<CoverageView> {
        tokio::task::spawn_blocking(move || sources::coverage(&record, aspect.as_deref())).await?
    }
    pub(super) async fn source_query(
        &self,
        mut record: Record,
        run: i64,
        operation: String,
        query: SourceQuery,
    ) -> Result<(Record, SourceResponse)> {
        tokio::task::spawn_blocking(move || {
            let response = sources::query(&mut record, run, &operation, query)?;
            Ok((record, response))
        })
        .await?
    }
    pub(super) async fn assessment(
        &self,
        mut record: Record,
        run: i64,
        assessment: AspectAssessment,
    ) -> Result<Record> {
        tokio::task::spawn_blocking(move || {
            sources::assess(&mut record, run, assessment)?;
            Ok(record)
        })
        .await?
    }
    pub(super) async fn report(
        &self,
        mut record: Record,
        run: i64,
        report: JobReport,
    ) -> Result<Record> {
        tokio::task::spawn_blocking(move || {
            evaluation::accept_report(&mut record, run, report)?;
            Ok(record)
        })
        .await?
    }
    pub(super) async fn candidate(&self, mut record: Record) -> Result<Record> {
        tokio::task::spawn_blocking(move || {
            candidate(&mut record)?;
            Ok(record)
        })
        .await?
    }
    pub(super) async fn check_fresh(&self, record: Record) -> Result<()> {
        tokio::task::spawn_blocking(move || {
            sources::check_fresh(
                Path::new(&record.workspace),
                &record.directory,
                &record.inputs(),
                &record.files,
            )
        })
        .await?
    }
    pub(super) async fn publish(&self, record: Record) -> Result<()> {
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || writer.with_write(|| publish_files(&record))).await?
    }
    pub(super) async fn recover_processes(&self, record: &Record) -> Result<()> {
        for id in &record.job().run_ids {
            crate::backend::execution::process_identity::cleanup(
                &Path::new(&record.artifact_dir).join(format!("process-{id}.json")),
            )
            .await?;
        }
        Ok(())
    }
    pub(super) async fn prepare_inputs(
        &self,
        record: Record,
        previous: Option<Record>,
    ) -> Result<Record> {
        tokio::task::spawn_blocking(move || prepare_inputs(record, previous)).await?
    }
    pub(super) async fn pass_workspace(&self, record: Record, run_id: i64) -> Result<PathBuf> {
        tokio::task::spawn_blocking(move || {
            if !matches!(record.job().stage, JobStage::Reader | JobStage::Review) {
                return Ok(record.draft());
            }
            let working = Path::new(&record.artifact_dir).join(format!("reader-{run_id}"));
            let inventory = crate::backend::knowledge::discovery::Discovery::read(
                &record.draft(),
                &record.directory,
            );
            for path in inventory.files {
                let relative = path.strip_prefix(record.draft())?;
                let target = working.join(relative);
                fs::create_dir_all(target.parent().unwrap())?;
                fs::copy(path, target)?;
            }
            fs::create_dir_all(&working)?;
            Ok(working)
        })
        .await?
    }
}
fn prepare_inputs(mut record: Record, previous: Option<Record>) -> Result<Record> {
    let inputs = Path::new(&record.artifact_dir).join(format!("capture-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&inputs)?;
    record.files = match sources::capture(Path::new(&record.workspace), &record.directory, &inputs)
    {
        Ok(files) => {
            if record.inputs().exists() {
                fs::remove_dir_all(record.inputs())?;
            }
            fs::rename(inputs, record.inputs())?;
            files
        }
        Err(error) => {
            let _ = fs::remove_dir_all(inputs);
            return Err(error);
        }
    };
    let omissions: Vec<String> = serde_json::from_slice(&fs::read(
        Path::new(&record.artifact_dir).join("source-omissions.json"),
    )?)?;
    if !omissions.is_empty() {
        record
            .detail
            .findings
            .retain(|f| f.id != "source-input-omissions");
        record.detail.findings.push(KnowledgeFinding{id:"source-input-omissions".into(),explanation:"Source input excludes binary, non-UTF-8, or oversized files; assess whether these omissions leave consequential gaps".into(),references:omissions,consequential:true});
    }
    for file in &record.files {
        if let Some(relative) = file.path.strip_prefix(&format!("{}/", record.directory)) {
            let target = record.draft().join(&record.directory).join(relative);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::copy(record.inputs().join(&file.path), target)?;
        }
    }
    if let Some(previous) = previous
        && matches!(
            previous.job().status,
            JobStatus::Failed | JobStatus::Cancelled
        )
    {
        let index = crate::backend::knowledge::documents::Index::read(
            &previous.draft(),
            &previous.directory,
        )?;
        for document in index.documents.values() {
            let relative = format!("{}/{}", record.directory, document.summary.path);
            let before = fs::read(previous.inputs().join(&relative)).ok();
            let current = fs::read(record.inputs().join(&relative)).ok();
            if before == current {
                let target = record.draft().join(relative);
                fs::create_dir_all(target.parent().unwrap())?;
                fs::write(target, &document.markdown)?;
            } else {
                record.detail.remaining.push(format!(
                    "Retained draft for '{}' has changed evidence; reconsider it",
                    document.summary.path
                ));
            }
        }
    }
    fs::create_dir_all(record.draft().join(&record.directory))?;
    Ok(record)
}

pub(super) fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    fs::create_dir_all(path.parent().unwrap())?;
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(value)?)?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    Ok(())
}
