use std::collections::HashMap;

use codee::string::JsonSerdeCodec;
use leptos::prelude::*;
use leptos_use::storage::{UseStorageOptions, use_local_storage_with_options};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Clone, Deserialize, PartialEq, Serialize)]
struct CacheEntries<T> {
    values: HashMap<String, T>,
}

impl<T> Default for CacheEntries<T> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

impl<T> CacheEntries<T>
where
    T: PartialEq,
{
    fn insert(&mut self, key: String, value: T) -> bool {
        if self.values.get(&key) == Some(&value) {
            return false;
        }
        self.values.insert(key, value);
        true
    }
}

pub(super) struct QueryCache<T>
where
    T: Send + Sync + 'static,
{
    entries: Signal<CacheEntries<T>>,
    set_entries: WriteSignal<CacheEntries<T>>,
    project_lifecycle_epoch: RwSignal<u64>,
}

impl<T> Clone for QueryCache<T>
where
    T: Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for QueryCache<T> where T: Send + Sync + 'static {}

impl<T> QueryCache<T>
where
    T: Clone + PartialEq + Send + Sync + 'static,
{
    pub(super) fn in_memory() -> Self {
        let (entries, set_entries) = signal(CacheEntries::default());
        Self {
            entries: entries.into(),
            set_entries,
            project_lifecycle_epoch: super::project_lifecycle_epoch_signal(),
        }
    }

    pub(super) fn get<K>(self, key: &K) -> Option<T>
    where
        K: Serialize,
    {
        let key = serde_json::to_string(key).ok()?;
        self.entries
            .with(|entries| entries.values.get(&key).cloned())
    }

    pub(super) fn get_untracked<K>(self, key: &K) -> Option<T>
    where
        K: Serialize,
    {
        let key = serde_json::to_string(key).ok()?;
        self.entries
            .with_untracked(|entries| entries.values.get(&key).cloned())
    }

    pub(super) fn store<K>(self, key: &K, value: &T)
    where
        K: Serialize,
    {
        self.store_owned(key, value.clone());
    }

    pub(super) fn store_owned<K>(self, key: &K, value: T)
    where
        K: Serialize,
    {
        let Ok(key) = serde_json::to_string(key) else {
            return;
        };
        let unchanged = self
            .entries
            .with_untracked(|entries| entries.values.get(&key) == Some(&value));
        if unchanged {
            return;
        }
        self.set_entries.update(move |entries| {
            entries.insert(key, value);
        });
    }

    #[cfg(not(feature = "ssr"))]
    pub(super) fn clear(self) {
        self.set_entries.set(CacheEntries::default());
    }

    pub(super) fn capture_lifecycle_epoch(self) -> u64 {
        self.project_lifecycle_epoch.get_untracked()
    }

    pub(super) fn lifecycle_epoch_is(self, expected: u64) -> bool {
        self.project_lifecycle_epoch.get_untracked() == expected
    }
}

impl<T> QueryCache<T>
where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub(super) fn persistent(storage_key: &'static str) -> Self {
        let (entries, set_entries, _) =
            use_local_storage_with_options::<CacheEntries<T>, JsonSerdeCodec>(
                storage_key,
                UseStorageOptions::default().delay_during_hydration(true),
            );
        Self {
            entries,
            set_entries,
            project_lifecycle_epoch: super::project_lifecycle_epoch_signal(),
        }
    }
}

pub(super) type LocalStorageCache<T> = QueryCache<T>;

#[cfg(test)]
mod tests {
    use super::{CacheEntries, QueryCache};
    use crate::frontend::services::ProjectLifecycleEpoch;
    use assertr::prelude::*;
    use leptos::prelude::*;

    #[test]
    fn cache_entries_ignore_unchanged_values() {
        let mut entries = CacheEntries::default();

        assert_that!(&(entries.insert("project".to_owned(), 1))).is_true();
        assert_that!(&(!entries.insert("project".to_owned(), 1))).is_true();
        assert_that!(&(entries.values.get("project"))).is_equal_to(Some(&1));
    }

    #[test]
    fn in_memory_cache_stores_typed_values_without_browser_storage() {
        Owner::new().with(|| {
            provide_context(ProjectLifecycleEpoch(RwSignal::new(0)));
            let cache = QueryCache::<String>::in_memory();

            cache.store(&("demo", 7), &"first".to_owned());
            cache.store(&("demo", 8), &"second".to_owned());

            assert_that!(&(cache.get_untracked(&("demo", 7))))
                .is_equal_to(Some("first".to_owned()));
            assert_that!(&(cache.get_untracked(&("demo", 8))))
                .is_equal_to(Some("second".to_owned()));
        });
    }
}
