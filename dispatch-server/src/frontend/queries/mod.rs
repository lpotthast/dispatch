pub(crate) mod cache;
use std::future::Future;

use leptos::prelude::*;

/// A route-owned resource projected over a domain store's cached values.
/// The response carries its input so data from a previous selection cannot render.
#[derive(Clone, Copy)]
pub(crate) struct Query<T: Send + Sync + 'static> {
    pub(crate) value: Signal<Option<T>>,
    pub(crate) pending: Signal<bool>,
    pub(crate) error: Signal<Option<String>>,
    pub(crate) refresh: Callback<()>,
}

pub(crate) fn query_resource<Input, T, F, C, L, Fut, R, I>(
    initial: Option<T>,
    input: F,
    cached: C,
    load: L,
    remember: R,
    invalidate: I,
) -> Query<T>
where
    Input: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + PartialEq + Send + Sync + 'static,
    R: Fn(Input, T) + Send + Sync + 'static,
    I: Fn(Input) + Send + Sync + 'static,
    F: Fn() -> Input + Clone + Send + Sync + 'static,
    C: Fn(&Input) -> Option<T> + Clone + Send + Sync + 'static,
    L: Fn(Input) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, ServerFnError>> + Send + 'static,
{
    resource(initial, input, cached, load, remember, invalidate, false)
}

pub(crate) fn seeded_query_resource<Input, T, F, C, L, Fut, R, I>(
    initial: Option<T>,
    input: F,
    cached: C,
    load: L,
    remember: R,
    invalidate: I,
) -> Query<T>
where
    Input: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + PartialEq + Send + Sync + 'static,
    R: Fn(Input, T) + Send + Sync + 'static,
    I: Fn(Input) + Send + Sync + 'static,
    F: Fn() -> Input + Clone + Send + Sync + 'static,
    C: Fn(&Input) -> Option<T> + Clone + Send + Sync + 'static,
    L: Fn(Input) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, ServerFnError>> + Send + 'static,
{
    resource(initial, input, cached, load, remember, invalidate, true)
}

fn resource<Input, T, F, C, L, Fut, R, I>(
    initial: Option<T>,
    input: F,
    cached: C,
    load: L,
    remember: R,
    invalidate: I,
    seeded: bool,
) -> Query<T>
where
    Input: Clone + PartialEq + Send + Sync + 'static,
    T: Clone + PartialEq + Send + Sync + 'static,
    R: Fn(Input, T) + Send + Sync + 'static,
    I: Fn(Input) + Send + Sync + 'static,
    F: Fn() -> Input + Clone + Send + Sync + 'static,
    C: Fn(&Input) -> Option<T> + Clone + Send + Sync + 'static,
    L: Fn(Input) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Result<T, ServerFnError>> + Send + 'static,
{
    let input = Memo::new(move |_| input());
    let initial_input = input.get_untracked();
    let initial_for_load = initial.clone();
    let seed_input = initial_input.clone();
    // A source revision makes pending state and seeded first loads explicit. Resource owns
    // scheduling, dependency tracking, and disposal.
    let revision = RwSignal::new(0_u64);
    let fetch = move |(key, revision): (Input, u64)| {
        let initial = (seeded && revision == 0 && key == seed_input)
            .then(|| initial_for_load.clone())
            .flatten();
        let load = load.clone();
        async move {
            let result = match initial {
                Some(value) => Ok(value),
                None => load(key.clone()).await.map_err(|error| error.to_string()),
            };
            (key, revision, result)
        }
    };
    // Browser caches and subscriptions initialize during hydration. LocalResource
    // keeps request execution with those stores while the shell renders on the server.
    let resource = LocalResource::new(move || fetch((input.get(), revision.get())));
    let response = Signal::derive(move || resource.get());
    // Share successful initial values with the store as well as fetched values.
    Effect::new(move |_| {
        response.with(|response| {
            if let Some((key, _, Ok(value))) = response
                && *key == input.get_untracked()
            {
                remember(key.clone(), value.clone());
            }
        });
    });
    let snapshot = Memo::new(move |previous: Option<&(Input, Option<T>)>| {
        let key = input.get();
        let fresh = response.with(|response| {
            response.as_ref().and_then(|(loaded_key, _, result)| {
                (loaded_key == &key)
                    .then(|| result.as_ref().ok().cloned())
                    .flatten()
            })
        });
        let value = cached(&key)
            .or(fresh)
            .or_else(|| {
                previous
                    .filter(|(old_key, _)| old_key == &key)
                    .and_then(|(_, value)| value.clone())
            })
            .or_else(|| (key == initial_input).then(|| initial.clone()).flatten());
        (key, value)
    });
    Query {
        value: Signal::derive(move || snapshot.with(|(_, value)| value.clone())),
        pending: Signal::derive(move || {
            response.with(|response| {
                response.as_ref().is_none_or(|(key, loaded_revision, _)| {
                    *key != input.get() || *loaded_revision != revision.get()
                })
            })
        }),
        error: Signal::derive(move || {
            response.with(|response| {
                response.as_ref().and_then(|(key, _, result)| {
                    (*key == input.get())
                        .then(|| result.as_ref().err().cloned())
                        .flatten()
                })
            })
        }),
        refresh: Callback::new(move |()| {
            invalidate(input.get_untracked());
            revision.update(|revision| *revision = revision.wrapping_add(1))
        }),
    }
}
