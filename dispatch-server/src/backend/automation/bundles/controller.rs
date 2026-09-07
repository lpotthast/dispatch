//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct BundleController {
    pub(super) bundles: Arc<crate::backend::automation::bundles::service::BundleService>,
}
impl BundleController {
    pub(crate) fn new(
        bundles: Arc<crate::backend::automation::bundles::service::BundleService>,
    ) -> Self {
        Self { bundles }
    }
}
