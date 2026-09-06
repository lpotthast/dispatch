//! Current-file knowledge queries. Markdown owns content; these values are disposable projections.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeMetadata {
    pub id: Option<String>,
    #[serde(default)]
    pub refines: Vec<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub related_to: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeDiagnostic {
    pub path: String,
    pub code: String,
    pub message: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeSummary {
    pub path: String,
    pub id: Option<String>,
    pub title: String,
    pub summary: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeDocument {
    pub summary: KnowledgeSummary,
    pub metadata: KnowledgeMetadata,
    pub markdown: String,
    pub body_offset: usize,
    pub next_body_offset: Option<usize>,
    pub parents: Vec<String>,
    pub children: Vec<String>,
    pub dependents: Vec<String>,
    pub related: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeQuery {
    pub selector: Option<String>,
    pub id: Option<String>,
    pub path: Option<String>,
    pub text: Option<String>,
    #[serde(default)]
    pub offset: usize,
    pub limit: Option<usize>,
    #[serde(default)]
    pub body_offset: usize,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeOperation {
    Root,
    Node,
    Search,
    Check,
    List,
    Graph,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeView {
    pub project_id: i64,
    pub working_directory: String,
    pub knowledge_directory: String,
    pub index_generation: String,
    pub document: Option<KnowledgeDocument>,
    pub documents: Vec<KnowledgeSummary>,
    pub next_offset: Option<usize>,
    pub diagnostics: Vec<KnowledgeDiagnostic>,
    #[serde(default)]
    pub relations: Vec<KnowledgeRelation>,
}

/// A derived edge between visible document paths; direction follows authored frontmatter.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct KnowledgeRelation {
    pub from: String,
    pub to: String,
    pub kind: KnowledgeRelationKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeRelationKind {
    Refines,
    DependsOn,
    RelatedTo,
}

/// None creates a new path; Some requires the exact content opened by the editor.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KnowledgeSaveRequest {
    pub path: String,
    pub expected_fingerprint: Option<String>,
    pub markdown: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KnowledgeSaveResult {
    pub fingerprint: String,
}
