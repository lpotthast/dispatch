use rootcause::{Result, prelude::*};
pub fn normalize_identifier(identifier: impl Into<String>) -> Result<String> {
    let identifier = identifier.into().trim().to_owned();
    if identifier.is_empty() {
        bail!("work item state identifier cannot be empty");
    }
    if identifier.contains('=') {
        bail!("work item state identifier cannot contain '='");
    }
    Ok(identifier)
}

pub fn normalize_name(name: impl Into<String>) -> Result<String> {
    let name = name.into().trim().to_owned();
    if name.is_empty() {
        bail!("work item state name cannot be empty");
    }
    Ok(name)
}
