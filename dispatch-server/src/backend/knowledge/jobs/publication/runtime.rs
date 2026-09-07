use super::super::files::atomic_json;
use crate::backend::knowledge::{
    discovery::Discovery,
    documents::Index,
    editing,
    jobs::{model::Record, policy::hash, sources},
};
use dispatch_types::knowledge::{KnowledgeSaveRequest, jobs::*};
use rootcause::{Result, prelude::*};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, fs, path::Path};
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Journal {
    changes: Vec<CandidateChange>,
    applied: usize,
}

pub(crate) fn candidate(record: &mut Record) -> Result<()> {
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

pub(crate) fn publish_files(record: &Record) -> Result<()> {
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
        crate::backend::knowledge::policy::normalize_knowledge_directory(&change.path)?;
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
