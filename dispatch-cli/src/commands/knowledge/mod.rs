use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum KnowledgeCommand {
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
