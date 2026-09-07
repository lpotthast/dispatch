use rootcause::{Result, prelude::*};
use std::path::{Component, Path};
pub(crate) const DEFAULT_KNOWLEDGE_DIRECTORY: &str = "knowledge";

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
