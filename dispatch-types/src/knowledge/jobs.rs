//! Durable discovery jobs and their operational evidence. Markdown remains the design authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationMode {
    #[default]
    Review,
    Automatic,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct StartKnowledgeJob {
    pub request_id: String,
    #[serde(default)]
    pub previous_job_id: Option<i64>,
    #[serde(default)]
    pub context: String,
    #[serde(default = "default_budget")]
    pub budget_seconds: u64,
    #[serde(default)]
    pub token_budget: Option<u64>,
    #[serde(default)]
    pub application_mode: Option<ApplicationMode>,
}
pub const fn default_budget() -> u64 {
    3600
}
impl Default for StartKnowledgeJob {
    fn default() -> Self {
        Self {
            request_id: String::new(),
            previous_job_id: None,
            context: String::new(),
            budget_seconds: default_budget(),
            token_budget: None,
            application_mode: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    Queued,
    Running,
    AwaitingReview,
    Completed,
    Failed,
    Cancelled,
}
impl JobStatus {
    pub fn unresolved(self) -> bool {
        matches!(self, Self::Queued | Self::Running | Self::AwaitingReview)
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JobStage {
    #[default]
    Inventory,
    Discovery,
    Synthesis,
    Reader,
    Review,
    Publication,
    Finished,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeJob {
    pub id: i64,
    pub project_id: i64,
    pub request: StartKnowledgeJob,
    #[serde(default)]
    pub application_mode: ApplicationMode,
    pub status: JobStatus,
    pub stage: JobStage,
    pub progress: String,
    pub current_area: Option<String>,
    pub current_aspect: Option<String>,
    pub active_millis: u64,
    pub recovery_attempts: u32,
    pub run_ids: Vec<i64>,
    pub active_run_id: Option<i64>,
    pub cancel_requested: bool,
    pub outcome: String,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct SourceFile {
    pub path: String,
    pub fingerprint: String,
    pub lines: usize,
    pub bytes: usize,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SourceQuery {
    pub path: Option<String>,
    pub text: Option<String>,
    pub start: Option<usize>,
    pub end: Option<usize>,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct SourceExcerpt {
    pub path: String,
    pub fingerprint: String,
    pub range: LineRange,
    pub content: String,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct SourceResponse {
    pub files: Vec<SourceFile>,
    pub excerpts: Vec<SourceExcerpt>,
    pub next_offset: Option<usize>,
    pub next_line: Option<usize>,
}
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Represented,
    SourceOnly,
    Unresolved,
    FurtherAnalysis,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AspectAssessment {
    pub path: String,
    pub fingerprint: String,
    pub ranges: Vec<LineRange>,
    pub aspect: String,
    pub disposition: Disposition,
    #[serde(default)]
    pub document_ids: Vec<String>,
    #[serde(default)]
    pub finding_ids: Vec<String>,
    pub explanation: String,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RecordedAssessment {
    pub job_id: i64,
    pub run_id: i64,
    pub assessment: AspectAssessment,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct DiscoveryArea {
    pub id: String,
    pub responsibility: String,
    pub questions: Vec<String>,
    pub evidence: Vec<String>,
    pub aspects: Vec<String>,
    pub owners: Vec<String>,
    pub remaining: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeFinding {
    pub id: String,
    pub explanation: String,
    pub references: Vec<String>,
    pub consequential: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReadingQuestion {
    pub id: String,
    pub question: String,
    pub requirements: Vec<String>,
    pub evidence: Vec<SourceExcerpt>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReaderAnswer {
    pub question_id: String,
    pub answer: String,
    pub references: Vec<String>,
    pub missing: bool,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AnswerEvaluation {
    pub question_id: String,
    pub correct: bool,
    pub issues: Vec<String>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct QualityResults {
    pub question_set_fingerprint: String,
    pub evidence_estimated_tokens: u64,
    pub reading_estimated_tokens: u64,
    pub source_fallback_estimated_tokens: u64,
    pub answers: Vec<ReaderAnswer>,
    pub evaluations: Vec<AnswerEvaluation>,
    pub review_issues: Vec<String>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct JobReport {
    pub summary: String,
    #[serde(default)]
    pub areas: Vec<DiscoveryArea>,
    #[serde(default)]
    pub findings: Vec<KnowledgeFinding>,
    #[serde(default)]
    pub questions: Vec<ReadingQuestion>,
    #[serde(default)]
    pub answers: Vec<ReaderAnswer>,
    #[serde(default)]
    pub evaluations: Vec<AnswerEvaluation>,
    #[serde(default)]
    pub review_issues: Vec<String>,
    #[serde(default)]
    pub reviewed_documents: Vec<String>,
    #[serde(default)]
    pub remaining: Vec<String>,
    #[serde(default)]
    pub ready_for_synthesis: bool,
    #[serde(default)]
    pub complete: bool,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct JobProgress {
    pub body: String,
    pub area: Option<String>,
    pub aspect: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CandidateChange {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeJobDetail {
    pub job: Option<KnowledgeJob>,
    pub areas: Vec<DiscoveryArea>,
    pub findings: Vec<KnowledgeFinding>,
    pub remaining: Vec<String>,
    pub assessments: Vec<RecordedAssessment>,
    pub quality: QualityResults,
    pub changes: Vec<CandidateChange>,
    pub validation: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CoverageRegion {
    pub path: String,
    pub ranges: Vec<LineRange>,
    pub aspect: Option<String>,
    pub reason: String,
    pub document_ids: Vec<String>,
}
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct CoverageView {
    pub files: Vec<SourceFile>,
    pub supplied_lines: usize,
    pub considered_lines: usize,
    pub accounted_lines: usize,
    pub total_lines: usize,
    pub aspects: Vec<String>,
    pub assessments: Vec<RecordedAssessment>,
    pub investigation: Vec<CoverageRegion>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum JobAction {
    Cancel,
    Retry { request_id: String },
    Continue { request_id: String },
    Apply,
    Reject,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeSettings {
    pub application_mode: ApplicationMode,
}
