//! REST controller with concrete, constructor-injected domain collaborators.
use std::sync::Arc;
pub(crate) struct RoutingController {
    pub(super) attribution: Arc<crate::backend::attribution::service::AttributionService>,
    pub(super) routing: Arc<crate::backend::automation::routing::service::RoutingService>,
}
impl RoutingController {
    pub(crate) fn new(
        attribution: Arc<crate::backend::attribution::service::AttributionService>,
        routing: Arc<crate::backend::automation::routing::service::RoutingService>,
    ) -> Self {
        Self {
            attribution,
            routing,
        }
    }
}
