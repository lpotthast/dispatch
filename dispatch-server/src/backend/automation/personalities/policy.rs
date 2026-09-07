use rootcause::{Result, prelude::*};
pub(crate) const DEFAULT_PERSONALITY_NAME: &str = "Default";
pub(crate) fn normalize_name(name: String) -> Result<String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        bail!("personality name cannot be empty");
    }
    Ok(name)
}
