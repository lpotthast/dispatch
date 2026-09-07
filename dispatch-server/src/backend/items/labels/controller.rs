//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct LabelController {
    pub(super) labels: Arc<crate::backend::items::labels::service::LabelService>,
}
impl LabelController {
    pub(crate) fn new(labels: Arc<crate::backend::items::labels::service::LabelService>) -> Self {
        Self { labels }
    }
}
