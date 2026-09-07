#[derive(Clone, Debug)]
pub(crate) struct LabelKeyRecord {
    pub(crate) id: i64,
    pub(crate) project_id: i64,
    pub(crate) key: String,
    pub(crate) accent_color: Option<String>,
    pub(crate) persistent: bool,
    pub(crate) built_in: bool,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}
pub(crate) struct CreateLabelKey {
    pub(crate) key: String,
    pub(crate) accent_color: Option<String>,
    pub(crate) persistent: bool,
}
pub(crate) struct UpdateLabelKey {
    pub(crate) accent_color: Option<String>,
    pub(crate) persistent: bool,
}
impl CreateLabelKey {
    pub(crate) fn validate(mut self) -> rootcause::Result<Self> {
        self.key = crate::backend::items::labels::policy::normalize_key(self.key)?;
        self.accent_color = super::policy::normalize_accent_color(self.accent_color)?;
        if !self.persistent {
            rootcause::bail!("an unused label key must be created as persistent");
        }
        Ok(self)
    }
}
impl UpdateLabelKey {
    pub(crate) fn validate(mut self, built_in: bool, key: &str) -> rootcause::Result<Self> {
        self.accent_color = super::policy::normalize_accent_color(self.accent_color)?;
        if built_in && !self.persistent {
            rootcause::bail!("built-in label key '{key}' must remain persistent");
        }
        Ok(self)
    }
}
