use rootcause::{Result, prelude::*};
pub(crate) fn normalize_accent_color(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(None);
    }
    let valid = value.len() == 7
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit());
    if !valid {
        bail!("accent color must use #RRGGBB hexadecimal notation");
    }
    Ok(Some(value.to_ascii_lowercase()))
}
