//! Parse and derive a bounded refinement graph from ordinary Markdown, without canonical sidecars.
use super::discovery::Discovery;
use dispatch_types::knowledge::*;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};
use rootcause::{Result, prelude::*};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

pub(super) struct Index {
    pub documents: BTreeMap<String, KnowledgeDocument>,
    pub diagnostics: Vec<KnowledgeDiagnostic>,
    pub generation: String,
}

fn valid_id(id: &str) -> bool {
    id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
}

fn has_tags(value: &yaml_serde::Value) -> bool {
    match value {
        yaml_serde::Value::Tagged(_) => true,
        yaml_serde::Value::Sequence(values) => values.iter().any(has_tags),
        yaml_serde::Value::Mapping(values) => {
            values.iter().any(|(k, v)| has_tags(k) || has_tags(v))
        }
        _ => false,
    }
}

fn metadata(markdown: &str) -> Result<(KnowledgeMetadata, &str)> {
    let text = markdown.trim_start_matches('\u{feff}');
    let Some(after) = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
    else {
        return Ok((KnowledgeMetadata::default(), text));
    };
    let mut end = 0;
    for line in after.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "---" {
            let value: yaml_serde::Value = yaml_serde::from_str(&after[..end])?;
            if !value.is_mapping() || has_tags(&value) {
                bail!("frontmatter must be a mapping without YAML tags");
            }
            let meta: KnowledgeMetadata = yaml_serde::from_value(value)?;
            for id in meta
                .id
                .iter()
                .chain(&meta.refines)
                .chain(&meta.depends_on)
                .chain(&meta.related_to)
            {
                if !valid_id(id) {
                    bail!("invalid document identity '{id}'");
                }
            }
            for source in &meta.sources {
                super::normalize_knowledge_directory(source)?;
            }
            return Ok((meta, &after[end + line.len()..]));
        }
        end += line.len();
    }
    bail!("frontmatter is missing its closing --- delimiter")
}

/// Render only the first H1 and its opening paragraph as plain routing text.
/// Markdown parsing prevents code examples from becoming document titles.
fn routing_text(body: &str) -> (String, String) {
    let mut title = String::new();
    let mut summary = String::new();
    let mut in_title = false;
    let mut after_title = false;
    let mut in_summary = false;
    for event in Parser::new(body) {
        match event {
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            }) if !after_title => {
                in_title = true;
            }
            Event::End(TagEnd::Heading(HeadingLevel::H1)) if in_title => {
                in_title = false;
                after_title = true;
            }
            Event::Start(Tag::Paragraph) if after_title => in_summary = true,
            Event::End(TagEnd::Paragraph) if in_summary => break,
            Event::Start(_) if after_title && !in_summary => break,
            Event::Text(text) | Event::Code(text) => {
                if in_title {
                    title.push_str(&text);
                } else if in_summary {
                    summary.push_str(&text);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if in_title {
                    title.push(' ');
                }
                if in_summary {
                    summary.push(' ');
                }
            }
            _ => {}
        }
    }
    (title.trim().to_owned(), summary.trim().to_owned())
}

impl Index {
    pub fn read(workspace: &Path, directory: &str) -> Result<Self> {
        let discovered = Discovery::read(workspace, directory);
        let visible_files = discovered.files.clone();
        let controls = discovered.controls.clone();
        let mut index = Self {
            documents: BTreeMap::new(),
            diagnostics: discovered.diagnostics,
            generation: String::new(),
        };
        let mut digest = Sha256::new();
        for (path, hash) in discovered.controls {
            digest.update(path.to_string_lossy().as_bytes());
            digest.update(hash);
        }
        for path in discovered.files {
            let relative = path
                .strip_prefix(workspace.join(directory))
                .expect("discovered path is within knowledge")
                .to_string_lossy()
                .into_owned();
            let read = || -> Result<String> {
                let meta = fs::symlink_metadata(&path)?;
                if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > 4 * 1024 * 1024
                {
                    bail!("document must be a regular UTF-8 file no larger than 4 MiB");
                }
                let mut text = String::new();
                fs::File::open(&path)?
                    .take(4 * 1024 * 1024 + 1)
                    .read_to_string(&mut text)?;
                if text.len() > 4 * 1024 * 1024 {
                    bail!("document grew beyond the 4 MiB read limit");
                }
                Ok(text)
            };
            let markdown = match read() {
                Ok(text) => text,
                Err(error) => {
                    index.problem(&relative, "read_failed", error.to_string());
                    continue;
                }
            };
            let fingerprint = format!("{:x}", Sha256::digest(markdown.as_bytes()));
            digest.update(relative.as_bytes());
            digest.update(&fingerprint);
            let (meta, body) = match metadata(&markdown) {
                Ok(parsed) => parsed,
                Err(error) => {
                    index.problem(&relative, "invalid_metadata", error.to_string());
                    (KnowledgeMetadata::default(), markdown.as_str())
                }
            };
            let (mut title, paragraph) = routing_text(body);
            if title.is_empty() {
                index.problem(&relative, "missing_title", "Add a level-one heading".into());
                title.clone_from(&relative);
            }
            if paragraph.is_empty() {
                index.problem(
                    &relative,
                    "missing_summary",
                    "Add a short opening paragraph".into(),
                );
            }
            if meta.id.is_none() {
                index.problem(
                    &relative,
                    "unorganized",
                    "Add a stable id and a refinement route to README.md".into(),
                );
            }
            let words = body.split_whitespace().count();
            if words > if relative == "README.md" { 500 } else { 1500 } {
                index.problem(
                    &relative,
                    "size_review",
                    format!("{words} words: review whether a split would improve reading"),
                );
            }
            index.documents.insert(
                relative.clone(),
                KnowledgeDocument {
                    summary: KnowledgeSummary {
                        path: relative,
                        id: meta.id.clone(),
                        title,
                        summary: if paragraph.chars().count() > 600 {
                            format!("{}…", paragraph.chars().take(600).collect::<String>())
                        } else {
                            paragraph
                        },
                        fingerprint,
                    },
                    metadata: meta,
                    markdown,
                    body_offset: 0,
                    next_body_offset: None,
                    parents: vec![],
                    children: vec![],
                    dependents: vec![],
                    related: vec![],
                },
            );
        }
        index.generation = format!("{:x}", digest.finalize());
        let current = Discovery::read(workspace, directory);
        if current.files != visible_files || current.controls != controls {
            bail!(
                "knowledge participation changed while reading; retry with current ignore controls"
            );
        }
        index.derive();
        Ok(index)
    }

    fn problem(&mut self, path: &str, code: &str, message: String) {
        self.diagnostics.push(KnowledgeDiagnostic {
            path: path.into(),
            code: code.into(),
            message,
        });
    }

    fn derive(&mut self) {
        let mut ids: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (path, doc) in &self.documents {
            if let Some(id) = &doc.metadata.id {
                ids.entry(id.clone()).or_default().push(path.clone());
            }
        }
        for (id, paths) in &ids {
            if paths.len() > 1 {
                for path in paths {
                    self.problem(
                        path,
                        "duplicate_id",
                        format!("Identity '{id}' occurs at {}", paths.join(", ")),
                    );
                }
            }
        }
        if !self.documents.contains_key("README.md") {
            self.problem(
                "README.md",
                "missing_root",
                "Add the project overview at README.md; other documents remain readable".into(),
            );
        }
        let mut edges = Vec::new();
        let metadata: Vec<_> = self
            .documents
            .iter()
            .map(|(path, doc)| (path.clone(), doc.metadata.clone()))
            .collect();
        for (path, meta) in metadata {
            if path == "README.md" && !meta.refines.is_empty() {
                self.problem(
                    &path,
                    "root_parent",
                    "The project root cannot refine another document".into(),
                );
            }
            let Some(id) = &meta.id else {
                continue;
            };
            if ids[id].len() != 1 {
                continue;
            }
            for (kind, targets) in [
                ("refines", meta.refines),
                ("depends_on", meta.depends_on),
                ("related_to", meta.related_to),
            ] {
                let mut seen = BTreeSet::new();
                for target in targets {
                    if !seen.insert(target.clone()) {
                        self.problem(
                            &path,
                            "duplicate_relation",
                            format!("Duplicate {kind} relation to {target}"),
                        );
                        continue;
                    }
                    match ids.get(&target) {
                        Some(paths) if paths.len() == 1 && paths[0] != path => {
                            if kind != "refines" || path != "README.md" {
                                edges.push((path.clone(), paths[0].clone(), kind));
                            }
                        }
                        _ => self.problem(
                            &path,
                            "unavailable_relation",
                            format!("{kind} target '{target}' is missing, ambiguous, or self-referential"),
                        ),
                    }
                }
            }
        }
        let parents: BTreeMap<String, Vec<String>> = self
            .documents
            .keys()
            .map(|p| {
                (
                    p.clone(),
                    edges
                        .iter()
                        .filter(|(from, _, kind)| from == p && *kind == "refines")
                        .map(|(_, to, _)| to.clone())
                        .collect(),
                )
            })
            .collect();
        for (from, to, kind) in edges {
            if kind == "refines" && reachable(&parents, &to, &from) {
                self.problem(
                    &from,
                    "refinement_cycle",
                    format!("Refinement cycle through {from} and {to}"),
                );
                continue;
            }
            match kind {
                "refines" => {
                    self.documents
                        .get_mut(&from)
                        .expect("known document")
                        .parents
                        .push(to.clone());
                    self.documents
                        .get_mut(&to)
                        .expect("known document")
                        .children
                        .push(from);
                }
                "depends_on" => self
                    .documents
                    .get_mut(&to)
                    .expect("known document")
                    .dependents
                    .push(from),
                _ => {
                    self.documents
                        .get_mut(&from)
                        .expect("known document")
                        .related
                        .push(to.clone());
                    self.documents
                        .get_mut(&to)
                        .expect("known document")
                        .related
                        .push(from);
                }
            }
        }
        let valid_parents = self
            .documents
            .iter()
            .map(|(path, doc)| (path.clone(), doc.parents.clone()))
            .collect();
        for path in self.documents.keys().cloned().collect::<Vec<_>>() {
            if path != "README.md" && !reachable(&valid_parents, &path, "README.md") {
                self.problem(
                    &path,
                    "unorganized",
                    "No valid refinement route to README.md".into(),
                );
            }
        }
        for doc in self.documents.values_mut() {
            doc.related.sort();
            doc.related.dedup();
        }
    }

    pub fn select(&self, query: &KnowledgeQuery) -> Result<&KnowledgeDocument> {
        if query
            .selector
            .iter()
            .chain(query.id.iter())
            .chain(query.path.iter())
            .count()
            != 1
        {
            bail!("Select exactly one document using its id, path, or positional selector");
        }
        let mut paths = BTreeSet::new();
        for (path, doc) in &self.documents {
            if query.path.as_ref() == Some(path)
                || query
                    .id
                    .as_ref()
                    .is_some_and(|id| doc.metadata.id.as_ref() == Some(id))
                || query
                    .selector
                    .as_ref()
                    .is_some_and(|s| s == path || doc.metadata.id.as_ref() == Some(s))
            {
                paths.insert(path);
            }
        }
        match paths.len() {
            1 => Ok(&self.documents[*paths.first().expect("one path")]),
            0 => bail!("Document is not visible in this working copy"),
            _ => bail!("Ambiguous document selector; use an explicit unique path"),
        }
    }
}

fn reachable(parents: &BTreeMap<String, Vec<String>>, from: &str, to: &str) -> bool {
    let mut pending = vec![from];
    let mut visited = BTreeSet::new();
    while let Some(path) = pending.pop() {
        if path == to {
            return true;
        }
        if visited.insert(path)
            && let Some(next) = parents.get(path)
        {
            pending.extend(next.iter().map(String::as_str));
        }
    }
    false
}
