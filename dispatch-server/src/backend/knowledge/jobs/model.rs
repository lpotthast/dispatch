use dispatch_types::knowledge::jobs::*;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Supplied {
    pub(crate) run_id: i64,
    pub(crate) path: String,
    pub(crate) fingerprint: String,
    pub(crate) range: LineRange,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct Record {
    pub(crate) detail: KnowledgeJobDetail,
    pub(crate) workspace: String,
    pub(crate) directory: String,
    pub(crate) artifact_dir: String,
    pub(crate) files: Vec<SourceFile>,
    pub(crate) supplied: Vec<Supplied>,
    pub(crate) questions: Vec<ReadingQuestion>,
    pub(crate) reports: Vec<(i64, JobReport)>,
    pub(crate) reviewed_documents: Vec<String>,
    pub(crate) initial: bool,
    pub(crate) complete: bool,
    pub(crate) version: i64,
    pub(crate) pass_started_at: Option<i64>,
}
impl Record {
    pub(crate) fn job(&self) -> &KnowledgeJob {
        self.detail.job.as_ref().expect("persisted job")
    }
    pub(crate) fn job_mut(&mut self) -> &mut KnowledgeJob {
        self.detail.job.as_mut().expect("persisted job")
    }
    pub(crate) fn draft(&self) -> PathBuf {
        Path::new(&self.artifact_dir).join("draft")
    }
    pub(crate) fn inputs(&self) -> PathBuf {
        Path::new(&self.artifact_dir).join("inputs")
    }
}
