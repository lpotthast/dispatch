//! Checked ordinary-file saves. No metadata sidecars or alternative content authority.
use super::{discovery::Discovery, normalize_knowledge_directory};
use crate::backend::{projects, storage::Store};
use dispatch_types::knowledge::{KnowledgeSaveRequest, KnowledgeSaveResult};
use rootcause::{Result, prelude::*};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
};

// Serialize Dispatch's short file publications; external editors are checked by fingerprint.
static WRITES: Mutex<()> = Mutex::new(());

pub(crate) async fn save(
    store: &Store,
    project: &str,
    request: KnowledgeSaveRequest,
) -> Result<KnowledgeSaveResult> {
    let project = projects::find_project_by_name(store, project).await?;
    let working = project
        .path
        .ok_or_else(|| report!("project has no working directory"))?;
    tokio::task::spawn_blocking(move || {
        save_file(Path::new(&working), &project.knowledge_directory, &request)
    })
    .await?
}

fn fingerprint(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn check_current(path: &Path, expected: &Option<String>) -> Result<()> {
    match (fs::read(path), expected) {
        (Ok(bytes), Some(expected)) if fingerprint(&bytes) == *expected => Ok(()),
        (Err(error), None) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        (Ok(_), None) => bail!("a document already exists at this path; choose another path"),
        (Ok(_), Some(_)) => bail!(
            "document changed on disk; your draft is preserved. Compare it with current content before saving"
        ),
        (Err(error), _) => Err(report!(error)
            .context("cannot read the document originally opened")
            .into_dynamic()),
    }
}

struct TemporaryFile(PathBuf);
impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

pub(super) fn save_file(
    workspace: &Path,
    directory: &str,
    request: &KnowledgeSaveRequest,
) -> Result<KnowledgeSaveResult> {
    normalize_knowledge_directory(directory)?;
    normalize_knowledge_directory(&request.path)?;
    if !request.path.ends_with(".md") || request.markdown.len() > 4 * 1024 * 1024 {
        bail!("save requires a Markdown path and at most 4 MiB of UTF-8 content");
    }
    let workspace = workspace
        .canonicalize()
        .context("cannot open registered working copy")?;
    let _write = WRITES
        .lock()
        .map_err(|_| report!("knowledge writer is unavailable"))?;
    Discovery::check_file(&workspace, directory, &request.path)?;
    let destination = workspace.join(directory).join(&request.path);
    check_current(&destination, &request.expected_fingerprint)?;
    let parent = destination.parent().expect("validated document parent");
    fs::create_dir_all(parent)?;
    Discovery::check_file(&workspace, directory, &request.path)?;
    let temporary =
        TemporaryFile(parent.join(format!(".dispatch-edit-{}.tmp", uuid::Uuid::new_v4())));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary.0)?;
    if let Ok(metadata) = fs::metadata(&destination) {
        file.set_permissions(metadata.permissions())?;
    }
    file.write_all(request.markdown.as_bytes())?;
    file.sync_all()?;
    // Ignore edits and file edits made during preparation invalidate the write, too.
    Discovery::check_file(&workspace, directory, &request.path)?;
    check_current(&destination, &request.expected_fingerprint)?;
    if request.expected_fingerprint.is_none() {
        // A new path must not clobber another creator between the check and publication.
        fs::hard_link(&temporary.0, &destination)?;
    } else {
        fs::rename(&temporary.0, &destination)?;
    }
    Ok(KnowledgeSaveResult {
        fingerprint: fingerprint(request.markdown.as_bytes()),
    })
}
