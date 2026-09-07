use super::rules::policy::validate_stable_key;
use rootcause::Result;
#[derive(Clone, Debug)]
pub(crate) struct ManagedObjectKey {
    bundle_key: String,
    object_key: String,
}
impl ManagedObjectKey {
    pub(crate) fn new(bundle_key: &str, object_key: &str) -> Result<Self> {
        validate_stable_key("bundle key", bundle_key)?;
        validate_stable_key("object key", object_key)?;
        Ok(Self {
            bundle_key: bundle_key.into(),
            object_key: object_key.into(),
        })
    }
    pub(crate) fn bundle_key(&self) -> &str {
        &self.bundle_key
    }
    pub(crate) fn object_key(&self) -> &str {
        &self.object_key
    }
    pub(crate) fn matches(&self, bundle_key: Option<&str>, object_key: Option<&str>) -> bool {
        bundle_key == Some(self.bundle_key.as_str()) && object_key == Some(self.object_key.as_str())
    }
}
#[derive(Clone, Copy)]
pub(crate) enum ManagedDeletion {
    Reconcile,
    RemoveBundle,
}
