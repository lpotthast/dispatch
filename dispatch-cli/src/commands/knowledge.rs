use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum KnowledgeCommand {
    /// Retrieve retained source evidence for the active knowledge job.
    Source {
        #[command(subcommand)]
        command: KnowledgeSourceCommand,
    },
    /// Inspect and control discovery jobs or submit an active pass checkpoint.
    Job {
        #[command(subcommand)]
        command: KnowledgeJobCommand,
    },
    /// Read the project overview and immediate child summaries.
    Root(KnowledgePageArgs),
    /// Read a current Markdown document in the assigned working copy.
    Node {
        #[command(subcommand)]
        command: KnowledgeNodeCommand,
    },
    /// Search current Markdown without launching an agent.
    Search(KnowledgeSearchArgs),
    /// Check frontmatter, identities, and refinement relationships.
    Check(KnowledgeSelectArgs),
}
#[derive(Debug, Subcommand)]
pub(crate) enum KnowledgeNodeCommand {
    /// Read one document and its immediate relationships.
    Show(KnowledgeSelectArgs),
}
#[derive(Debug, Args)]
pub(crate) struct KnowledgePageArgs {
    /// Result offset for the next page of summaries.
    #[arg(long, default_value_t = 0)]
    pub offset: usize,
    /// Maximum summaries per page, from 1 through 100.
    #[arg(long,default_value_t=20,value_parser=clap::value_parser!(u16).range(1..=100))]
    pub limit: u16,
    /// Unicode character offset when continuing a large document.
    #[arg(long, default_value_t = 0)]
    pub body_offset: usize,
}
#[derive(Debug, Args)]
pub(crate) struct KnowledgeSelectArgs {
    /// Document ID or knowledge-directory-relative path.
    #[arg(conflicts_with_all=["id","path"])]
    pub selector: Option<String>,
    /// Select by a unique frontmatter identity.
    #[arg(long, conflicts_with = "path")]
    pub id: Option<String>,
    /// Select by a path relative to the knowledge directory.
    #[arg(long)]
    pub path: Option<String>,
    #[command(flatten)]
    pub page: KnowledgePageArgs,
}
#[derive(Debug, Args)]
pub(crate) struct KnowledgeSearchArgs {
    /// Words to find in current Markdown documents.
    #[arg(long)]
    pub text: String,
    #[command(flatten)]
    pub page: KnowledgePageArgs,
}

#[derive(Debug, Subcommand)]
pub(crate) enum KnowledgeSourceCommand {
    /// List eligible retained source paths without marking their content supplied.
    List {
        /// Project-relative source path or prefix.
        #[arg(long)]
        path: Option<String>,
        /// Result offset for the next page.
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum results per page; the server bounds this to 1 through 100.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Find literal text in retained inputs and record the returned line ranges.
    Search {
        /// Literal text to find in retained source.
        #[arg(long)]
        text: String,
        /// Project-relative source path or prefix.
        #[arg(long)]
        path: Option<String>,
        /// Result offset for the next page.
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// Maximum results per page; the server bounds this to 1 through 100.
        #[arg(long, default_value_t = 20)]
        limit: usize,
    },
    /// Read an inclusive source line range and record exactly what was supplied.
    Read {
        /// Exact project-relative path from the retained source inventory.
        #[arg(long)]
        path: String,
        /// First line to read, 1-based and inclusive.
        #[arg(long, default_value_t = 1)]
        start: usize,
        /// Last line to read, inclusive; defaults to a 200-line window.
        #[arg(long)]
        end: Option<usize>,
    },
}
#[derive(Debug, Subcommand)]
pub(crate) enum KnowledgeJobCommand {
    /// Checkpoint a short status for the active knowledge pass.
    Progress {
        /// Short progress message for the active pass.
        #[arg(long)]
        body: String,
        /// Stable discovery-area identifier currently being investigated.
        #[arg(long)]
        area: Option<String>,
        /// Exact aspect identifier used in the discovery map and assessments.
        #[arg(long)]
        aspect: Option<String>,
    },
    /// Submit a JSON pass report; '-' reads stdin.
    Report {
        /// JSON file to submit, or '-' to read one complete object from stdin.
        #[arg(long)]
        file: String,
    },
    /// Record one aspect-specific source assessment; '-' reads stdin.
    Assess {
        /// JSON file to submit, or '-' to read one complete object from stdin.
        #[arg(long)]
        file: String,
    },
    /// Inspect a user-selected job (unavailable to launched knowledge readers).
    Inspect {
        /// Knowledge job ID in the selected project.
        id: i64,
    },
    /// List project knowledge jobs, newest first.
    List,
    /// Start one bounded user-requested discovery job with a stable idempotency key.
    Start {
        /// Stable idempotency key; reuse it when retrying the same HTTP request.
        #[arg(long)]
        request_id: String,
        /// Completed predecessor whose discoveries and remaining scope are retained.
        #[arg(long)]
        previous_job: Option<i64>,
        /// Additional project context or documentation URLs.
        #[arg(long, default_value = "")]
        context: String,
        /// Shared active-work limit in seconds, including automatic recovery (60–86400).
        #[arg(long, default_value_t = 3600)]
        budget_seconds: u64,
        /// Stop further passes when reported token usage reaches this amount.
        #[arg(long)]
        token_budget: Option<u64>,
    },
    /// Authorize a linked bounded discovery job from current evidence.
    Continue {
        /// Knowledge job ID in the selected project.
        id: i64,
        /// Stable idempotency key; reuse it when retrying the same HTTP request.
        #[arg(long)]
        request_id: String,
    },
    /// Authorize another bounded attempt using retained drafts and checkpoints.
    Retry {
        /// Knowledge job ID in the selected project.
        id: i64,
        /// Stable idempotency key; reuse it when retrying the same HTTP request.
        #[arg(long)]
        request_id: String,
    },
    /// Persist cancellation before stopping the active job process.
    Cancel {
        /// Knowledge job ID in the selected project.
        id: i64,
    },
    /// Apply a reviewed candidate after freshness and publication checks.
    Apply {
        /// Knowledge job ID in the selected project.
        id: i64,
    },
    /// Resolve a pending proposal while retaining its draft and history.
    Reject {
        /// Knowledge job ID in the selected project.
        id: i64,
    },
    /// Inspect current source consideration and outstanding aspects.
    Coverage {
        /// Knowledge job ID; otherwise use --knowledge-job or DISPATCH_KNOWLEDGE_JOB_ID.
        id: Option<i64>,
        /// Exact aspect identifier used in the discovery map and assessments.
        #[arg(long)]
        aspect: Option<String>,
    },
}
