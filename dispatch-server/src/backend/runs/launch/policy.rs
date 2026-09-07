use rootcause::{Result, prelude::*};
use serde::Serialize;
use sha2::{Digest, Sha256};
const MAX_TEXT: usize = 512;
pub(crate) fn canonical_value_sha256<T: Serialize>(domain: &str, value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value).context("failed to encode launch selector")?;
    Ok(domain_sha256(domain, &bytes))
}

pub(crate) fn domain_sha256(domain: &str, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub(crate) fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

pub(crate) fn validate_sha256(name: &str, value: &str) -> Result<()> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        bail!("{name} must be lowercase SHA-256 hex");
    }
    Ok(())
}

pub(crate) fn validate_text(name: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_TEXT
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        bail!("{name} must be bounded, trimmed, non-control text");
    }
    Ok(())
}
