use std::sync::{
    Arc, LazyLock, RwLock,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::broadcast;

use crate::{
    backend::storage::utc_now,
    shared::view_models::{UiEvent, UiEventKind},
};

const EVENT_BUFFER_SIZE: usize = 1024;

static EVENT_BUS: LazyLock<RwLock<Option<Arc<UiEventBus>>>> = LazyLock::new(|| RwLock::new(None));

#[derive(Debug)]
struct UiEventBus {
    sender: broadcast::Sender<UiEvent>,
    next_sequence: AtomicU64,
}

pub(crate) fn install() {
    let (sender, _) = broadcast::channel(EVENT_BUFFER_SIZE);
    let bus = UiEventBus {
        sender,
        next_sequence: AtomicU64::new(1),
    };
    *EVENT_BUS
        .write()
        .expect("Patchbay UI event bus lock is poisoned") = Some(Arc::new(bus));
}

pub(crate) fn subscribe() -> broadcast::Receiver<UiEvent> {
    event_bus().sender.subscribe()
}

pub(crate) fn publish(
    kind: UiEventKind,
    project: Option<impl Into<String>>,
    item_id: Option<i64>,
    run_id: Option<i64>,
) {
    let Some(bus) = EVENT_BUS
        .read()
        .expect("Patchbay UI event bus lock is poisoned")
        .clone()
    else {
        return;
    };
    let event = UiEvent {
        sequence: bus.next_sequence.fetch_add(1, Ordering::Relaxed),
        kind,
        project: project.map(Into::into),
        item_id,
        run_id,
        timestamp: utc_now(),
    };
    let _ = bus.sender.send(event);
}

pub(crate) fn publish_project(kind: UiEventKind, project: &str) {
    publish(kind, Some(project), None, None);
}

pub(crate) fn publish_item(kind: UiEventKind, project: &str, item_id: i64) {
    publish(kind, Some(project), Some(item_id), None);
}

pub(crate) fn publish_run(kind: UiEventKind, project: &str, run_id: i64, item_id: Option<i64>) {
    publish(kind, Some(project), item_id, Some(run_id));
}

pub(crate) fn publish_global(kind: UiEventKind) {
    publish(kind, None::<String>, None, None);
}

fn event_bus() -> Arc<UiEventBus> {
    let Some(bus) = EVENT_BUS
        .read()
        .expect("Patchbay UI event bus lock is poisoned")
        .clone()
    else {
        install();
        return EVENT_BUS
            .read()
            .expect("Patchbay UI event bus lock is poisoned")
            .as_ref()
            .expect("Patchbay UI event bus was just installed")
            .clone();
    };
    bus
}
