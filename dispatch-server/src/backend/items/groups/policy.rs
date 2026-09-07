use rootcause::{Result, prelude::*};
pub(crate) fn normalize_group_key(value: String) -> Result<String> {
    let value = value.trim().to_owned();
    if value.is_empty()
        || value.len() > 128
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte)
        })
    {
        bail!("work-group key must use lowercase letters, digits, '.', '_' or '-'");
    }
    Ok(value)
}

pub(crate) fn normalize_group_name(value: String) -> Result<String> {
    let value = value.trim().to_owned();
    if value.is_empty() {
        bail!("work-group name cannot be empty");
    }
    if value.len() > 200 {
        bail!("work-group name cannot exceed 200 bytes");
    }
    Ok(value)
}
