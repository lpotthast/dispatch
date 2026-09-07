//! A PID alone never authorizes cleanup: compare its start identity, command and process group.
use rootcause::Result;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path, time::Duration};
#[derive(Serialize, Deserialize)]
struct Identity {
    pid: u32,
    signature: String,
}
#[cfg(unix)]
fn signature(pid: u32) -> Result<Option<String>> {
    let output = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", "pgid=,lstart=,command="])
        .output()?;
    if !output.status.success() {
        return Ok(None);
    }
    let text = String::from_utf8(output.stdout)?.trim().to_owned();
    if text.is_empty() {
        return Ok(None);
    }
    if text
        .split_whitespace()
        .next()
        .and_then(|p| p.parse::<u32>().ok())
        != Some(pid)
    {
        return Ok(None);
    }
    Ok(Some(text))
}
#[cfg(not(unix))]
fn signature(_pid: u32) -> Result<Option<String>> {
    Ok(None)
}
pub(crate) fn record(path: &Path, pid: u32) -> Result<()> {
    if let Some(signature) = signature(pid)? {
        use std::io::Write;
        let parent = path
            .parent()
            .ok_or_else(|| rootcause::report!("process identity path has no parent"))?;
        fs::create_dir_all(parent)?;
        let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)?;
        file.write_all(&serde_json::to_vec(&Identity { pid, signature })?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}
pub(crate) async fn cleanup(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let identity: Identity = serde_json::from_slice(&fs::read(path)?)?;
    if signature(identity.pid)?.as_deref() != Some(&identity.signature) {
        fs::remove_file(path)?;
        return Ok(());
    }
    #[cfg(unix)]
    {
        // The launch runtime starts a fresh process group; its leader and start time still match.
        unsafe {
            libc::kill(-(identity.pid as i32), libc::SIGTERM);
        }
        for _ in 0..20 {
            if signature(identity.pid)?.as_deref() != Some(&identity.signature) {
                fs::remove_file(path)?;
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        if signature(identity.pid)?.as_deref() == Some(&identity.signature) {
            unsafe {
                libc::kill(-(identity.pid as i32), libc::SIGKILL);
            }
        }
    }
    fs::remove_file(path)?;
    Ok(())
}
