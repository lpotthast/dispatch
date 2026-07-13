use std::{cell::Cell, future::Future, rc::Rc};

use leptos::prelude::*;
use leptos_router::hooks::use_query_map;
use serde::Serialize;

pub(crate) fn selected_project_signal() -> Memo<Option<String>> {
    let query = use_query_map();
    Memo::new(move |_| query.read().get("project"))
}

#[derive(Clone, Copy)]
pub(crate) struct CachedQuery<T: 'static> {
    pub(crate) value: ReadSignal<Option<T>>,
    pub(crate) refresh: Callback<()>,
}

pub(crate) fn cached_query<Input, T, InputFn, CachedFn, LoadFn, LoadFuture>(
    initial: Option<T>,
    input: InputFn,
    cached: CachedFn,
    load: LoadFn,
) -> CachedQuery<T>
where
    Input: Clone + 'static,
    T: Serialize + Send + Sync + 'static,
    InputFn: Fn() -> Input + Clone + 'static,
    CachedFn: Fn(&Input) -> Option<T> + Clone + 'static,
    LoadFn: Fn(Input) -> LoadFuture + Clone + 'static,
    LoadFuture: Future<Output = Result<T, ServerFnError>> + 'static,
{
    let (value, set_value) = signal(initial);
    let (revision, set_revision) = signal(0_u64);
    let generation = Rc::new(Cell::new(0_u64));

    let input_for_cache = input.clone();
    Effect::new(move |_| {
        let input = input_for_cache();
        if let Some(cached) = cached(&input) {
            set_query_value_if_changed(value, set_value, cached);
        }
    });

    Effect::new(move |_| {
        revision.get();
        let input = input();

        let next_generation = generation.get().wrapping_add(1);
        generation.set(next_generation);
        let generation = Rc::clone(&generation);
        let load = load.clone();
        leptos::task::spawn_local(async move {
            if let Ok(next) = load(input).await
                && generation.get() == next_generation
            {
                set_query_value_if_changed(value, set_value, next);
            }
        });
    });

    CachedQuery {
        value,
        refresh: Callback::new(move |()| set_revision.update(|revision| *revision += 1)),
    }
}

fn set_query_value_if_changed<T>(
    value: ReadSignal<Option<T>>,
    set_value: WriteSignal<Option<T>>,
    next: T,
) where
    T: Serialize + Send + Sync + 'static,
{
    let unchanged = value.with_untracked(|current| {
        current.as_ref().is_some_and(|current| {
            serde_json::to_string(current).ok() == serde_json::to_string(&next).ok()
        })
    });
    if !unchanged {
        set_value.set(Some(next));
    }
}
