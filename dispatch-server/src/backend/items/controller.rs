//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct ItemController {
    pub(super) attribution: Arc<crate::backend::attribution::service::AttributionService>,
    pub(super) item_creation: Arc<crate::backend::items::creation::service::ItemCreationService>,
    pub(super) items: Arc<crate::backend::items::service::ItemService>,
}
impl ItemController {
    pub(crate) fn new(
        attribution: Arc<crate::backend::attribution::service::AttributionService>,
        item_creation: Arc<crate::backend::items::creation::service::ItemCreationService>,
        items: Arc<crate::backend::items::service::ItemService>,
    ) -> Self {
        Self {
            attribution,
            item_creation,
            items,
        }
    }
}
