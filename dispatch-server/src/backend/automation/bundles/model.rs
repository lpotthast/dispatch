use dispatch_types::AutomationBundleManifest;
use rootcause::{Result, prelude::*};
#[derive(Debug)]
pub(crate) struct ValidatedBundle {
    pub(crate) manifest: AutomationBundleManifest,
    pub(crate) manifest_hash: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BundleStatus {
    Applied,
    Removed,
}
impl BundleStatus {
    pub(crate) fn as_storage(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Removed => "removed",
        }
    }
    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "applied" => Ok(Self::Applied),
            "removed" => Ok(Self::Removed),
            _ => bail!("invalid stored bundle status '{value}'"),
        }
    }
}
#[derive(Clone, Debug)]
pub(crate) struct BundleRecord {
    pub(crate) id: i64,
    pub(crate) bundle_key: String,
    pub(crate) display_name: String,
    pub(crate) manifest_hash: String,
    pub(crate) status: BundleStatus,
    pub(crate) created_at: String,
}
