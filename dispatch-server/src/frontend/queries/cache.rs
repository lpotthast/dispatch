use std::{
    collections::HashMap,
    hash::Hash,
    sync::{Arc, Mutex},
};

use codee::string::JsonSerdeCodec;
use futures_util::{
    FutureExt,
    future::{BoxFuture, WeakShared},
};
use leptos::prelude::*;
use leptos_use::storage::{UseStorageOptions, use_local_storage_with_options};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Clone, Deserialize, PartialEq, Serialize)]
struct Entries<K, V> {
    // JSON object keys cannot represent tuples. Decode entries into typed keys immediately.
    values: Vec<(K, V)>,
}
impl<K, V> Default for Entries<K, V> {
    fn default() -> Self {
        Self { values: Vec::new() }
    }
}

type Pending<V> = WeakShared<BoxFuture<'static, Result<V, ServerFnError>>>;
struct Requests<K, V> {
    sequence: u64,
    versions: HashMap<K, u64>,
    pending: HashMap<K, (u64, Pending<V>)>,
}
impl<K, V> Default for Requests<K, V> {
    fn default() -> Self {
        Self {
            sequence: 0,
            versions: HashMap::new(),
            pending: HashMap::new(),
        }
    }
}

#[derive(Clone)]
pub(crate) struct Ticket<K> {
    key: K,
    version: u64,
    lifecycle: u64,
}

/// Domain-store persistence and shared request ordering. Leptos resources own consumers.
pub(crate) struct QueryCache<K: Send + Sync + 'static, V: Send + Sync + 'static> {
    entries: Signal<Entries<K, V>>,
    set_entries: WriteSignal<Entries<K, V>>,
    lifecycle: RwSignal<u64>,
    requests: Arc<Mutex<Requests<K, V>>>,
    selectors: Arc<Mutex<HashMap<K, ArcMemo<Option<V>>>>>,
}
impl<K: Send + Sync + 'static, V: Send + Sync + 'static> Clone for QueryCache<K, V> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries,
            set_entries: self.set_entries,
            lifecycle: self.lifecycle,
            requests: Arc::clone(&self.requests),
            selectors: Arc::clone(&self.selectors),
        }
    }
}

impl<K, V> QueryCache<K, V>
where
    K: Clone + Eq + Hash + Send + Sync + 'static,
    V: Clone + PartialEq + Send + Sync + 'static,
{
    pub(crate) fn in_memory() -> Self {
        let (entries, set_entries) = signal(Entries::default());
        Self::new(entries.into(), set_entries)
    }
    fn new(entries: Signal<Entries<K, V>>, set_entries: WriteSignal<Entries<K, V>>) -> Self {
        let requests = Arc::new(Mutex::new(Requests::<K, V>::default()));
        let cleanup_requests = Arc::clone(&requests);
        on_cleanup(move || {
            let mut requests = cleanup_requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            requests.versions.clear();
            requests.pending.clear();
        });
        Self {
            entries,
            set_entries,
            lifecycle: crate::frontend::app::context::project_lifecycle_epoch_signal(),
            requests,
            selectors: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    pub(crate) fn get(&self, key: &K) -> Option<V> {
        let selector = {
            let mut selectors = self
                .selectors
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            selectors
                .entry(key.clone())
                .or_insert_with(|| {
                    let entries = self.entries;
                    let key = key.clone();
                    ArcMemo::new(move |_| {
                        entries.with(|entries| {
                            entries
                                .values
                                .iter()
                                .find(|(candidate, _)| candidate == &key)
                                .map(|(_, value)| value.clone())
                        })
                    })
                })
                .clone()
        };
        selector.get()
    }
    pub(crate) fn get_untracked(&self, key: &K) -> Option<V> {
        self.entries.with_untracked(|entries| {
            entries
                .values
                .iter()
                .find(|(candidate, _)| candidate == key)
                .map(|(_, value)| value.clone())
        })
    }
    pub(crate) fn seed(&self, key: K, value: V) {
        // An explicitly supplied initial value supersedes disk state, but never a
        // response from a newer request.
        let requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !requests.versions.contains_key(&key) {
            self.store(key, value);
        }
    }
    fn store(&self, key: K, value: V) {
        if self.get_untracked(&key).as_ref() == Some(&value) {
            return;
        }
        self.set_entries.try_update(|entries| {
            if let Some((_, existing)) = entries
                .values
                .iter_mut()
                .find(|(candidate, _)| candidate == &key)
            {
                *existing = value;
            } else {
                entries.values.push((key, value));
            }
        });
    }
    pub(crate) fn begin(&self, key: K) -> Ticket<K> {
        let mut requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        requests.sequence = requests.sequence.wrapping_add(1);
        let version = requests.sequence;
        requests.versions.insert(key.clone(), version);
        Ticket {
            key,
            version,
            lifecycle: self.lifecycle.get_untracked(),
        }
    }
    pub(crate) fn commit(&self, ticket: Ticket<K>, value: V) -> bool {
        self.commit_if(ticket, value, |_| true)
    }
    fn commit_if(&self, ticket: Ticket<K>, value: V, retain: impl FnOnce(&V) -> bool) -> bool {
        let requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let current = self.lifecycle.try_get_untracked() == Some(ticket.lifecycle)
            && requests.versions.get(&ticket.key) == Some(&ticket.version);
        if current && retain(&value) {
            self.store(ticket.key, value);
        }
        current
    }
    pub(crate) async fn load<F, Fut>(&self, key: K, fetch: F) -> Result<V, ServerFnError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<V, ServerFnError>> + Send + 'static,
    {
        self.load_retained(key, fetch, |_| true).await
    }
    pub(crate) async fn load_retained<F, Fut, R>(
        &self,
        key: K,
        fetch: F,
        retain: R,
    ) -> Result<V, ServerFnError>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: std::future::Future<Output = Result<V, ServerFnError>> + Send + 'static,
        R: FnOnce(&V) -> bool + Send + 'static,
    {
        let request = {
            let mut requests = self
                .requests
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(request) = requests
                .pending
                .get(&key)
                .and_then(|(_, request)| request.upgrade())
            {
                request
            } else {
                requests.sequence = requests.sequence.wrapping_add(1);
                let version = requests.sequence;
                requests.versions.insert(key.clone(), version);
                let ticket = Ticket {
                    key: key.clone(),
                    version,
                    lifecycle: self.lifecycle.get_untracked(),
                };
                let cache = self.clone();
                let completed_key = key.clone();
                let request = async move {
                    let result = fetch().await;
                    let result = match result {
                        Ok(value) if cache.commit_if(ticket, value.clone(), retain) => Ok(value),
                        Ok(_) => Err(ServerFnError::new(
                            "The selected data changed while the request was running.",
                        )),
                        Err(error) => Err(error),
                    };
                    let mut requests = cache
                        .requests
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    if requests
                        .pending
                        .get(&completed_key)
                        .is_some_and(|(current, _)| *current == version)
                    {
                        requests.pending.remove(&completed_key);
                    }
                    result
                }
                .boxed()
                .shared();
                // The cache does not keep an abandoned consumer's future alive.
                requests.pending.insert(
                    key,
                    (
                        version,
                        request.downgrade().expect("new request is pending"),
                    ),
                );
                request
            }
        };
        request.await
    }
    pub(crate) fn retain_keys(&self, keep: impl Fn(&K) -> bool) {
        let mut requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        requests.versions.retain(|key, _| keep(key));
        requests.pending.retain(|key, _| keep(key));
        self.set_entries
            .try_update(|entries| entries.values.retain(|(key, _)| keep(key)));
    }

    pub(crate) fn invalidate_key(&self, key: &K) {
        let mut requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        requests.versions.remove(key);
        requests.pending.remove(key);
    }

    pub(crate) fn invalidate(&self) {
        let mut requests = self
            .requests
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        requests.versions.clear();
        requests.pending.clear();
    }
    pub(crate) fn clear(&self) {
        self.invalidate();
        self.set_entries.try_set(Entries::default());
    }
}

impl<K, V> QueryCache<K, V>
where
    K: Clone + Eq + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    V: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub(crate) fn persistent(key: &'static str) -> Self {
        let (entries, set_entries, _) =
            use_local_storage_with_options::<Entries<K, V>, JsonSerdeCodec>(
                key,
                UseStorageOptions::default().delay_during_hydration(true),
            );
        Self::new(entries, set_entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn cache<K, V>() -> (Owner, QueryCache<K, V>)
    where
        K: Clone + Eq + Hash + Send + Sync + 'static,
        V: Clone + PartialEq + Send + Sync + 'static,
    {
        let owner = Owner::new();
        let cache = owner.with(|| {
            provide_context(crate::frontend::app::context::ProjectLifecycleEpoch(
                RwSignal::new(0),
            ));
            QueryCache::in_memory()
        });
        (owner, cache)
    }

    #[test]
    fn typed_tuple_keys_round_trip_without_collisions() {
        let entries = Entries {
            values: vec![
                (("a:b".to_owned(), 7), "first".to_owned()),
                (("a".to_owned(), 7), "second".to_owned()),
            ],
        };
        let json = serde_json::to_string(&entries).unwrap();
        let decoded: Entries<(String, i64), String> = serde_json::from_str(&json).unwrap();
        assert_that!(&decoded.values).is_equal_to(entries.values);
    }

    #[test]
    fn late_response_cannot_replace_a_newer_response_for_the_same_key() {
        let (_owner, cache) = cache::<String, i32>();
        let older = cache.begin("demo".into());
        let newer = cache.begin("demo".into());
        assert_that!(cache.commit(newer, 2)).is_true();
        assert_that!(cache.commit(older, 1)).is_false();
        assert_that!(cache.get_untracked(&"demo".into())).is_equal_to(Some(2));
    }

    #[test]
    fn invalidation_and_project_lifecycle_reject_inflight_responses() {
        let (owner, cache) = cache::<String, i32>();
        let before_clear = cache.begin("demo".into());
        cache.clear();
        assert_that!(cache.commit(before_clear, 1)).is_false();
        let before_deletion = cache.begin("demo".into());
        owner.with(|| {
            crate::frontend::app::context::project_lifecycle_epoch_signal()
                .update(|epoch| *epoch += 1)
        });
        assert_that!(cache.commit(before_deletion, 2)).is_false();
        assert_that!(cache.get_untracked(&"demo".into())).is_none();
    }

    #[tokio::test]
    async fn abandoned_requests_are_not_reused_on_later_navigation() {
        let (_owner, cache) = cache::<String, i32>();
        let (release, response) = tokio::sync::oneshot::channel();
        {
            let abandoned =
                cache.load(
                    "demo".into(),
                    move || async move { Ok(response.await.unwrap()) },
                );
            tokio::pin!(abandoned);
            assert_that!(futures_util::poll!(&mut abandoned).is_pending()).is_true();
        }
        assert_that!(release.send(1).is_err()).is_true();
        let fresh = cache.load("demo".into(), || async { Ok(2) }).await.unwrap();
        assert_that!(fresh).is_equal_to(2);
    }

    #[tokio::test]
    async fn refresh_starts_a_new_request_and_rejects_the_old_response() {
        let (_owner, cache) = cache::<String, i32>();
        cache.seed("demo".into(), 0);
        let (release, response) = tokio::sync::oneshot::channel();
        let old = cache.load(
            "demo".into(),
            move || async move { Ok(response.await.unwrap()) },
        );
        tokio::pin!(old);
        assert_that!(futures_util::poll!(&mut old).is_pending()).is_true();
        cache.invalidate_key(&"demo".into());
        assert_that!(cache.get_untracked(&"demo".into())).is_equal_to(Some(0));
        let fresh = cache.load("demo".into(), || async { Ok(2) }).await.unwrap();
        assert_that!(fresh).is_equal_to(2);
        release.send(1).unwrap();
        assert_that!(old.await.is_err()).is_true();
        assert_that!(cache.get_untracked(&"demo".into())).is_equal_to(Some(2));
    }

    #[test]
    fn initial_value_replaces_disk_state_but_never_a_newer_response() {
        let (_owner, cache) = cache::<(), i32>();
        cache.seed((), 1);
        cache.seed((), 2);
        assert_that!(cache.get_untracked(&())).is_equal_to(Some(2));
        let ticket = cache.begin(());
        cache.commit(ticket, 3);
        cache.seed((), 2);
        assert_that!(cache.get_untracked(&())).is_equal_to(Some(3));
    }

    #[tokio::test]
    async fn concurrent_consumers_share_one_request() {
        let (_owner, cache) = cache::<String, i32>();
        let calls = Arc::new(AtomicUsize::new(0));
        let calls_first = Arc::clone(&calls);
        let calls_second = Arc::clone(&calls);
        let first = cache.load("demo".into(), move || async move {
            calls_first.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(42)
        });
        let second = cache.load("demo".into(), move || async move {
            calls_second.fetch_add(1, Ordering::SeqCst);
            Ok(99)
        });
        let (first, second) = futures_util::future::join(first, second).await;
        assert_that!(first.unwrap()).is_equal_to(42);
        assert_that!(second.unwrap()).is_equal_to(42);
        assert_that!(calls.load(Ordering::SeqCst)).is_equal_to(1);
        assert_that!(cache.get_untracked(&"demo".into())).is_equal_to(Some(42));
    }

    #[tokio::test]
    async fn errors_preserve_saved_data_and_transient_results_are_not_retained() {
        let (_owner, cache) = cache::<(), i32>();
        cache.seed((), 7);
        let failure = cache
            .load((), || async { Err(ServerFnError::new("offline")) })
            .await;
        assert_that!(failure.is_err()).is_true();
        assert_that!(cache.get_untracked(&())).is_equal_to(Some(7));
        cache.clear();
        let transient = cache.load_retained((), || async { Ok(8) }, |_| false).await;
        assert_that!(transient.unwrap()).is_equal_to(8);
        assert_that!(cache.get_untracked(&())).is_none();
    }
}
