use crate::frontend::codex::store::codex_store;
use crate::frontend::live_events::refetch_on_live_event;
use crate::frontend::queries::query_resource;
use leptos::prelude::*;

/// These subscriptions belong to the layout, so changing routes does not restart them.
#[component]
pub(crate) fn SharedStatusProvider() -> impl IntoView {
    let codex = codex_store();
    let store_for_refresh = codex.clone();
    let store_for_seed = codex.clone();
    let codex_query = query_resource(
        None,
        || (),
        |_| None,
        move |()| {
            let store = codex.clone();
            async move { store.load_status().await }
        },
        move |(), value| store_for_seed.seed_status(value),
        move |()| store_for_refresh.invalidate_status(),
    );
    refetch_on_live_event(codex_query.refresh, |event| {
        matches!(event, dispatch_types::UiEvent::CodexStatusChanged { .. })
    });
    let selected_project = crate::frontend::components::selected_project_signal();
    let knowledge = crate::frontend::knowledge::store::knowledge_store();
    let jobs = LocalResource::new(move || {
        let project = selected_project.get();
        let store = knowledge.clone();
        async move {
            match project {
                Some(project) => store.list_jobs(project).await,
                None => Ok(Vec::new()),
            }
        }
    });
    let _poll = leptos_use::use_interval_fn(move || jobs.refetch(), 3_000);
}
