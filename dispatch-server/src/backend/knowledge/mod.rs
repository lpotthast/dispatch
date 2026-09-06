//! Deterministic, current-working-copy knowledge reading. No signing, sidecar authority, or AI.
mod discovery;
mod documents;
mod editing;
pub(crate) use editing::save;
#[cfg(test)]
mod tests;

use crate::backend::{
    entities::agent_run::AgentRun, projects, request_attribution::RequestAttribution,
    storage::Store,
};
use dispatch_types::knowledge::*;
use rootcause::{Result, prelude::*};
use sea_orm::EntityTrait;
use std::path::{Component, Path};

pub(crate) const DEFAULT_KNOWLEDGE_DIRECTORY: &str = "knowledge";
const BODY_CHARACTERS: usize = 32_000;

pub(crate) fn normalize_knowledge_directory(value: &str) -> Result<String> {
    if value.is_empty()
        || value.contains('\\')
        || Path::new(value)
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
        || value
            .split('/')
            .any(|p| p.is_empty() || matches!(p, "." | ".." | ".git" | ".dispatch" | ".knowledge"))
    {
        bail!("knowledge paths must be project-relative and cannot traverse runtime directories");
    }
    Ok(value.to_owned())
}

/// Resolve the actual registered run directory; client filesystem paths never select a workspace.
pub(crate) async fn query(
    store: &Store,
    project_name: &str,
    attribution: &RequestAttribution,
    operation: KnowledgeOperation,
    query: KnowledgeQuery,
) -> Result<KnowledgeView> {
    let project = projects::find_project_by_name(store, project_name).await?;
    let working = if let Some(run_id) = attribution.agent_run_id {
        let run = AgentRun::find_by_id(run_id)
            .one(store.db().as_ref())
            .await?
            .ok_or_else(|| report!("agent run is missing"))?;
        if run.project_id != project.id || run.working_dir.is_empty() {
            bail!("agent run has no registered working copy in this project");
        }
        run.working_dir
    } else {
        project
            .path
            .ok_or_else(|| report!("project has no working directory"))?
    };
    let directory = normalize_knowledge_directory(&project.knowledge_directory)?;
    tokio::task::spawn_blocking(move || read(project.id, &working, &directory, operation, &query))
        .await?
}

pub(crate) fn read(
    project_id: i64,
    working: &str,
    directory: &str,
    operation: KnowledgeOperation,
    query: &KnowledgeQuery,
) -> Result<KnowledgeView> {
    normalize_knowledge_directory(directory)?;
    let workspace = Path::new(working)
        .canonicalize()
        .context("cannot open registered working copy")?;
    let index = documents::Index::read(&workspace, directory)?;
    let selected = match operation {
        KnowledgeOperation::Root => index.documents.get("README.md"),
        KnowledgeOperation::Node => Some(index.select(query)?),
        KnowledgeOperation::Check
            if query.selector.is_some() || query.id.is_some() || query.path.is_some() =>
        {
            Some(index.select(query)?)
        }
        _ => None,
    };
    let mut documents: Vec<_> = match operation {
        KnowledgeOperation::Root | KnowledgeOperation::Node => selected
            .into_iter()
            .flat_map(|root| root.children.iter())
            .filter_map(|path| index.documents.get(path))
            .map(|d| d.summary.clone())
            .collect(),
        KnowledgeOperation::Check => vec![],
        KnowledgeOperation::List | KnowledgeOperation::Graph => index
            .documents
            .values()
            .map(|d| d.summary.clone())
            .collect(),
        KnowledgeOperation::Search => {
            let text = query.text.as_deref().unwrap_or("").trim().to_lowercase();
            if text.is_empty() {
                bail!("search requires non-empty text");
            }
            let terms: Vec<_> = text.split_whitespace().collect();
            let mut hits: Vec<_> = index
                .documents
                .values()
                .filter_map(|doc| {
                    let body = doc.markdown.to_lowercase();
                    if !terms.iter().all(|term| body.contains(term)) {
                        return None;
                    }
                    let title = doc.summary.title.to_lowercase();
                    let score = terms
                        .iter()
                        .map(|term| {
                            body.matches(term).count().min(20)
                                + if title.contains(term) { 50 } else { 0 }
                        })
                        .sum::<usize>();
                    Some((score, doc.summary.clone()))
                })
                .collect();
            hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.path.cmp(&b.1.path)));
            hits.into_iter().map(|(_, summary)| summary).collect()
        }
    };
    let limit = query.limit.unwrap_or(20).clamp(1, 100);
    let total = documents.len();
    documents = documents
        .into_iter()
        .skip(query.offset)
        .take(limit)
        .collect();
    // Only graph pages carry edges, owned by their source document. Dependencies use the
    // derived backlinks so invalid/ambiguous authored targets cannot become usable edges.
    let mut relations = Vec::new();
    if operation == KnowledgeOperation::Graph {
        for summary in &documents {
            let doc = &index.documents[&summary.path];
            for parent in &doc.parents {
                relations.push(KnowledgeRelation {
                    from: summary.path.clone(),
                    to: parent.clone(),
                    kind: KnowledgeRelationKind::Refines,
                });
            }
            for target in index
                .documents
                .values()
                .filter(|target| target.dependents.contains(&summary.path))
            {
                relations.push(KnowledgeRelation {
                    from: summary.path.clone(),
                    to: target.summary.path.clone(),
                    kind: KnowledgeRelationKind::DependsOn,
                });
            }
            for related in doc.related.iter().filter(|path| *path > &summary.path) {
                relations.push(KnowledgeRelation {
                    from: summary.path.clone(),
                    to: related.clone(),
                    kind: KnowledgeRelationKind::RelatedTo,
                });
            }
        }
    }
    let document = selected.cloned().map(|mut doc| {
        let len = doc.markdown.chars().count();
        doc.markdown = doc
            .markdown
            .chars()
            .skip(query.body_offset)
            .take(BODY_CHARACTERS)
            .collect();
        doc.body_offset = query.body_offset;
        doc.next_body_offset = (len > query.body_offset.saturating_add(BODY_CHARACTERS))
            .then_some(query.body_offset.saturating_add(BODY_CHARACTERS));
        doc
    });
    Ok(KnowledgeView {
        project_id,
        working_directory: working.into(),
        knowledge_directory: directory.into(),
        index_generation: index.generation,
        document,
        documents,
        next_offset: (query.offset.saturating_add(limit) < total)
            .then_some(query.offset.saturating_add(limit)),
        diagnostics: index.diagnostics,
        relations,
    })
}

/// Compact launch input. An absent or invalid root is a reported gap, never a code-work gate.
pub(crate) async fn launch_context(project_id: i64, working: String, directory: String) -> String {
    match tokio::task::spawn_blocking(move || {
        read(
            project_id,
            &working,
            &directory,
            KnowledgeOperation::Root,
            &KnowledgeQuery::default(),
        )
    })
    .await
    {
        Ok(Ok(view)) => {
            let mut text = format!(
                "Knowledge directory: {} (in the assigned working copy).\n",
                view.knowledge_directory
            );
            match view.document {
                Some(root) => {
                    text.push_str(&root.markdown);
                    if let Some(offset) = root.next_body_offset {
                        text.push_str(&format!(
                            "\nRoot truncated; continue with dispatch knowledge node show --path README.md --body-offset {offset}.\n"
                        ));
                    }
                }
                None => text.push_str(
                    "No readable README.md knowledge root. Inspect visible documents and source as needed; report the gap.\n"
                ),
            }
            for doc in view.documents {
                text.push_str(&format!(
                    "\n- {} [{}]: {}",
                    doc.title, doc.path, doc.summary
                ));
            }
            if view.next_offset.is_some() {
                text.push_str("\nMore child summaries are available with knowledge root --offset.");
            }
            if !view.diagnostics.is_empty() {
                text.push_str(
                    "\nKnowledge has local diagnostics; use knowledge check to inspect them.",
                );
            }
            text
        }
        Ok(Err(error)) => format!(
            "Knowledge could not be read: {error}. Continue permitted work and report the gap."
        ),
        Err(error) => {
            format!("Knowledge reader failed: {error}. Continue permitted work and report the gap.")
        }
    }
}
