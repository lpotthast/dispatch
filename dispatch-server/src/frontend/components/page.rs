use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    sync::{Arc, Mutex},
};

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};

pub(crate) fn selected_project_signal() -> Memo<Option<String>> {
    let query = use_query_map();
    let params = use_params_map();
    Memo::new(move |_| {
        query
            .read()
            .get("project")
            .or_else(|| params.read().get("project"))
    })
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
    T: PartialEq + Send + Sync + 'static,
    InputFn: Fn() -> Input + Clone + 'static,
    CachedFn: Fn(&Input) -> Option<T> + Clone + 'static,
    LoadFn: Fn(Input) -> LoadFuture + Clone + 'static,
    LoadFuture: Future<Output = Result<T, ServerFnError>> + 'static,
{
    let (value, set_value) = signal(initial);
    let (revision, set_revision) = signal(0_u64);
    let generation = Rc::new(Cell::new(0_u64));
    let refresh_coalescer = Arc::new(RefreshCoalescer::default());

    let input_for_cache = input.clone();
    Effect::new(move |_| {
        let input = input_for_cache();
        if let Some(cached) = cached(&input) {
            set_query_value_if_changed(value, set_value, cached);
        }
    });

    let refresh_coalescer_for_load = Arc::clone(&refresh_coalescer);
    Effect::new(move |_| {
        revision.get();
        let input = input();

        let next_generation = generation.get().wrapping_add(1);
        generation.set(next_generation);
        let generation = Rc::clone(&generation);
        let refresh_coalescer = Arc::clone(&refresh_coalescer_for_load);
        refresh_coalescer.start_request();
        let load = load.clone();
        leptos::task::spawn_local(async move {
            if let Ok(next) = load(input).await
                && generation.get() == next_generation
            {
                set_query_value_if_changed(value, set_value, next);
            }
            if refresh_coalescer.finish_request() {
                set_revision.update(|revision| *revision = revision.wrapping_add(1));
            }
        });
    });

    let refresh_coalescer_for_callback = Arc::clone(&refresh_coalescer);
    CachedQuery {
        value,
        refresh: Callback::new(move |()| {
            if refresh_coalescer_for_callback.request_refresh() {
                set_revision.update(|revision| *revision = revision.wrapping_add(1));
            }
        }),
    }
}

fn set_query_value_if_changed<T>(
    value: ReadSignal<Option<T>>,
    set_value: WriteSignal<Option<T>>,
    next: T,
) where
    T: PartialEq + Send + Sync + 'static,
{
    let unchanged = value.with_untracked(|current| current.as_ref() == Some(&next));
    if !unchanged {
        set_value.set(Some(next));
    }
}

#[derive(Default)]
struct RefreshCoalescer {
    state: Mutex<RefreshState>,
}

#[derive(Default)]
struct RefreshState {
    in_flight: usize,
    pending: bool,
}

impl RefreshCoalescer {
    fn start_request(&self) {
        let mut state = self.state();
        state.in_flight = state.in_flight.saturating_add(1);
    }

    fn request_refresh(&self) -> bool {
        let mut state = self.state();
        if state.in_flight == 0 {
            true
        } else {
            state.pending = true;
            false
        }
    }

    fn finish_request(&self) -> bool {
        let mut state = self.state();
        state.in_flight = state.in_flight.saturating_sub(1);
        if state.in_flight == 0 && state.pending {
            state.pending = false;
            true
        } else {
            false
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, RefreshState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::RefreshCoalescer;
    use assertr::prelude::*;

    #[test]
    fn refresh_runs_immediately_while_idle() {
        assert_that!(&(RefreshCoalescer::default().request_refresh())).is_true();
    }

    #[test]
    fn refreshes_during_a_request_collapse_into_one_trailing_request() {
        let coalescer = RefreshCoalescer::default();
        coalescer.start_request();

        assert_that!(&(!coalescer.request_refresh())).is_true();
        assert_that!(&(!coalescer.request_refresh())).is_true();
        assert_that!(&(coalescer.finish_request())).is_true();
        assert_that!(&(!coalescer.finish_request())).is_true();
    }

    #[test]
    fn trailing_refresh_waits_for_all_concurrent_requests() {
        let coalescer = RefreshCoalescer::default();
        coalescer.start_request();
        coalescer.start_request();
        assert_that!(&(!coalescer.request_refresh())).is_true();

        assert_that!(&(!coalescer.finish_request())).is_true();
        assert_that!(&(coalescer.finish_request())).is_true();
    }
}
