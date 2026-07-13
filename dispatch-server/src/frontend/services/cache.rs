use std::collections::HashMap;

use codee::string::JsonSerdeCodec;
use leptos::prelude::*;
use leptos_use::storage::{UseStorageOptions, use_local_storage_with_options};
use serde::{Deserialize, Serialize, de::DeserializeOwned};

#[derive(Clone, Deserialize, PartialEq, Serialize)]
struct LocalStorageEntries<T> {
    values: HashMap<String, T>,
}

impl<T> Default for LocalStorageEntries<T> {
    fn default() -> Self {
        Self {
            values: HashMap::new(),
        }
    }
}

impl<T> LocalStorageEntries<T>
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

pub(super) struct LocalStorageCache<T>
where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    entries: Signal<LocalStorageEntries<T>>,
    set_entries: WriteSignal<LocalStorageEntries<T>>,
}

impl<T> Clone for LocalStorageCache<T>
where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for LocalStorageCache<T> where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static
{
}

impl<T> LocalStorageCache<T>
where
    T: Clone + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static,
{
    pub(super) fn persistent(storage_key: &'static str) -> Self {
        let (entries, set_entries, _) =
            use_local_storage_with_options::<LocalStorageEntries<T>, JsonSerdeCodec>(
                storage_key,
                UseStorageOptions::default().delay_during_hydration(true),
            );
        Self {
            entries,
            set_entries,
        }
    }

    pub(super) fn get<K>(self, key: &K) -> Option<T>
    where
        K: Serialize,
    {
        let key = serde_json::to_string(key).ok()?;
        self.entries.get().values.get(&key).cloned()
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
        let Ok(key) = serde_json::to_string(key) else {
            return;
        };
        let unchanged = self
            .entries
            .with_untracked(|entries| entries.values.get(&key) == Some(value));
        if unchanged {
            return;
        }
        self.set_entries.update(|entries| {
            entries.insert(key, value.clone());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::LocalStorageEntries;

    #[test]
    fn cache_entries_ignore_unchanged_values() {
        let mut entries = LocalStorageEntries::default();

        assert!(entries.insert("project".to_owned(), 1));
        assert!(!entries.insert("project".to_owned(), 1));
        assert_eq!(entries.values.get("project"), Some(&1));
    }
}
