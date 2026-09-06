use std::{
    cell::Cell,
    future::Future,
    rc::Rc,
    sync::{Arc, Mutex},
};

use leptos::prelude::*;
use leptos_router::hooks::{use_location, use_query_map};

pub(crate) fn selected_project_signal() -> Memo<Option<String>> {
    let query = use_query_map();
    let location = use_location();
    Memo::new(move |_| {
        query
            .read()
            .get("project")
            .or_else(|| project_from_path(&location.pathname.get()))
    })
}

// Layouts cannot read a child route's named parameters. Resolve the project
// from the same canonical URL for the layout, dock, subscriptions, and pages.
fn project_from_path(path: &str) -> Option<String> {
    let segments = path.trim_end_matches('/').split('/').collect::<Vec<_>>();
    let project = match segments.as_slice() {
        ["", "projects", project, "items", _]
        | ["", "projects", project, "automation", "runs", _, "log"] => *project,
        _ => return None,
    };
    if project.is_empty() {
        return None;
    }
    urlencoding::decode(project)
        .ok()
        .map(|project| project.into_owned())
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
    Input: Clone + PartialEq + 'static,
    T: PartialEq + Send + Sync + 'static,
    InputFn: Fn() -> Input + Clone + 'static,
    CachedFn: Fn(&Input) -> Option<T> + Clone + 'static,
    LoadFn: Fn(Input) -> LoadFuture + Clone + 'static,
    LoadFuture: Future<Output = Result<T, ServerFnError>> + 'static,
{
    cached_query_inner(initial, input, cached, load, true)
}

pub(crate) fn seeded_cached_query<Input, T, InputFn, CachedFn, LoadFn, LoadFuture>(
    initial: Option<T>,
    input: InputFn,
    cached: CachedFn,
    load: LoadFn,
) -> CachedQuery<T>
where
    Input: Clone + PartialEq + 'static,
    T: PartialEq + Send + Sync + 'static,
    InputFn: Fn() -> Input + Clone + 'static,
    CachedFn: Fn(&Input) -> Option<T> + Clone + 'static,
    LoadFn: Fn(Input) -> LoadFuture + Clone + 'static,
    LoadFuture: Future<Output = Result<T, ServerFnError>> + 'static,
{
    cached_query_inner(initial, input, cached, load, false)
}

fn cached_query_inner<Input, T, InputFn, CachedFn, LoadFn, LoadFuture>(
    initial: Option<T>,
    input: InputFn,
    cached: CachedFn,
    load: LoadFn,
    load_initially: bool,
) -> CachedQuery<T>
where
    Input: Clone + PartialEq + 'static,
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
    let mut previous_input = untrack(&input);
    Effect::new(move |_| {
        let input = input_for_cache();
        if input != previous_input {
            previous_input = input.clone();
            set_value.set(None);
        }
        if let Some(cached) = cached(&input) {
            set_query_value_if_changed(value, set_value, cached);
        }
    });

    let refresh_coalescer_for_load = Arc::clone(&refresh_coalescer);
    let first_load = Rc::new(Cell::new(true));
    Effect::new(move |_| {
        revision.get();
        let input = input();
        if first_load.replace(false) && !load_initially {
            return;
        }

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
                set_revision.try_update(|revision| *revision = revision.wrapping_add(1));
            }
        });
    });

    let refresh_coalescer_for_callback = Arc::clone(&refresh_coalescer);
    CachedQuery {
        value,
        refresh: Callback::new(move |()| {
            if refresh_coalescer_for_callback.request_refresh() {
                set_revision.try_update(|revision| *revision = revision.wrapping_add(1));
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
    if value.try_with_untracked(|current| current.as_ref() == Some(&next)) == Some(false) {
        set_value.try_set(Some(next));
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
    use super::{RefreshCoalescer, project_from_path};
    use assertr::prelude::*;

    #[test]
    fn layout_project_context_recognizes_only_project_scoped_routes() {
        for path in [
            "/projects/demo/items/42",
            "/projects/demo/automation/runs/7/log/",
        ] {
            assert_that!(project_from_path(path)).is_equal_to(Some("demo".to_owned()));
        }
        assert_that!(project_from_path("/projects/space%20and%2Bplus/items/42"))
            .is_equal_to(Some("space and+plus".to_owned()));
        for path in [
            "/",
            "/projects",
            "/projects/demo",
            "/projects//items/42",
            "/projects/demo/settings",
        ] {
            assert_that!(project_from_path(path)).is_none();
        }
    }

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
