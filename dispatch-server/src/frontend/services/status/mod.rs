use super::codex_service;
use crate::frontend::{components::cached_query, live_events::refetch_on_live_event};
use leptos::prelude::*;

/// These subscriptions belong to the layout, so changing routes does not restart them.
#[component]
pub(crate) fn SharedStatusProvider() -> impl IntoView {
    let codex = codex_service();
    let codex_query = cached_query(
        None,
        || (),
        |_| None,
        move |()| {
            let service = codex.clone();
            async move { service.load_status().await }
        },
    );
    refetch_on_live_event(codex_query.refresh, |event| {
        matches!(event, dispatch_types::UiEvent::CodexStatusChanged { .. })
    });
}
