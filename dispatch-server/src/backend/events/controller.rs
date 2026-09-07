use super::UiEventBus;
pub(crate) struct EventController {
    pub(super) events: UiEventBus,
}
impl EventController {
    pub(crate) fn new(events: UiEventBus) -> Self {
        Self { events }
    }
}
