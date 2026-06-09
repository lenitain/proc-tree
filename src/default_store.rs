//! Default storage implementation using standard library types.
//!
//! [`DefaultStore`] is a `HashMap<Mutex>` store with optional TTL-based eviction.
//!
//! # Example
//!
//! ```rust
//! use proc_tree::{DefaultStore, snapshot};
//!
//! let store = DefaultStore::new(600);
//! snapshot(&store);
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::traits::ProcessStore;
use crate::types::ProcessInfo;

// ---- Internal entry with optional TTL ----

struct Entry {
    value: ProcessInfo,
    inserted_at: Instant,
}

impl Clone for Entry {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            inserted_at: self.inserted_at,
        }
    }
}

// ---- Shared inner ----

type Inner = Arc<Mutex<HashMap<u32, Entry>>>;

fn get_inner(inner: &Inner, pid: u32, ttl: Duration) -> Option<ProcessInfo> {
    let mut map = inner.lock().unwrap();
    let entry = map.get(&pid)?;
    if !ttl.is_zero() && entry.inserted_at.elapsed() >= ttl {
        map.remove(&pid);
        return None;
    }
    Some(entry.value.clone())
}

fn insert_inner(inner: &Inner, pid: u32, value: ProcessInfo) {
    let mut map = inner.lock().unwrap();
    map.insert(
        pid,
        Entry {
            value,
            inserted_at: Instant::now(),
        },
    );
}

// ---- DefaultStore ----

/// Process tree store backed by `HashMap<Mutex>` with optional TTL eviction.
///
/// Thread-safe via `Arc<Mutex<...>>`. Cloning shares the same data.
pub struct DefaultStore {
    inner: Inner,
    children_index: Arc<Mutex<HashMap<u32, Vec<u32>>>>,
    ttl: Duration,
}

impl DefaultStore {
    /// Create a new store with the given TTL in seconds.
    /// `ttl_secs = 0` means no expiration.
    pub fn new(ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            children_index: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Number of entries (including possibly-expired ones not yet evicted).
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    /// Returns `true` if the store contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a PID exists and is not expired.
    pub fn contains_key(&self, pid: u32) -> bool {
        get_inner(&self.inner, pid, self.ttl).is_some()
    }
}

impl Clone for DefaultStore {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            children_index: Arc::clone(&self.children_index),
            ttl: self.ttl,
        }
    }
}

impl Default for DefaultStore {
    /// Creates a store with no TTL.
    fn default() -> Self {
        Self::new(0)
    }
}

impl ProcessStore for DefaultStore {
    fn get_process(&self, pid: u32) -> Option<ProcessInfo> {
        get_inner(&self.inner, pid, self.ttl)
    }

    fn insert_process(&self, pid: u32, info: ProcessInfo) {
        let ppid = info.ppid;

        // Insert the process
        insert_inner(&self.inner, pid, info);

        // Update children index
        let mut index = self.children_index.lock().unwrap();
        index.entry(ppid).or_default().push(pid);
    }

    fn remove_process(&self, pid: u32) -> Option<ProcessInfo> {
        // Remove from inner
        let info = {
            let mut map = self.inner.lock().unwrap();
            map.remove(&pid).map(|e| e.value)
        };

        // Remove from children index
        if let Some(ref p) = info {
            let mut index = self.children_index.lock().unwrap();
            if let Some(children) = index.get_mut(&p.ppid) {
                children.retain(|&c| c != pid);
            }
        }

        info
    }

    fn all_pids(&self) -> Vec<u32> {
        self.inner.lock().unwrap().keys().copied().collect()
    }

    fn children_of(&self, pid: u32) -> Vec<u32> {
        // O(1) lookup from index
        self.children_index
            .lock()
            .unwrap()
            .get(&pid)
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_store_insert_get() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        let info = store.get_process(1).unwrap();
        assert_eq!(info.ppid, 0);
        assert_eq!(info.cmd, "init");
    }

    #[test]
    fn default_store_ttl_expired() {
        let store = DefaultStore::new(0); // ttl=0 means no expiry
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(store.get_process(1).is_some());

        // With ttl=1, entry expires after 1 second
        let store = DefaultStore::new(1);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(store.get_process(1).is_some());
        std::thread::sleep(Duration::from_millis(1100));
        assert!(store.get_process(1).is_none());
    }

    #[test]
    fn clone_shares_data() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        let store2 = store.clone();
        assert!(store2.get_process(1).is_some());
        store2.insert_process(
            2,
            ProcessInfo {
                ppid: 1,
                cmd: "bash".into(),
                user: "root".into(),
                tgid: 2,
                start_time_ns: 0,
            },
        );
        assert!(store.get_process(2).is_some());
    }

    #[test]
    fn len_and_contains() {
        let store = DefaultStore::new(0);
        assert_eq!(store.len(), 0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "a".into(),
                user: "u".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert_eq!(store.len(), 1);
        assert!(store.contains_key(1));
        assert!(!store.contains_key(999));
    }

    #[test]
    fn is_empty_default() {
        let store = DefaultStore::new(0);
        assert!(store.is_empty());
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "a".into(),
                user: "u".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(!store.is_empty());
    }

    #[test]
    fn all_pids_returns_keys() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "a".into(),
                user: "u".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            2,
            ProcessInfo {
                ppid: 1,
                cmd: "b".into(),
                user: "u".into(),
                tgid: 2,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            3,
            ProcessInfo {
                ppid: 1,
                cmd: "c".into(),
                user: "u".into(),
                tgid: 3,
                start_time_ns: 0,
            },
        );
        let mut pids = store.all_pids();
        pids.sort();
        assert_eq!(pids, vec![1, 2, 3]);
    }

    #[test]
    fn ttl_contains_key_expires() {
        let store = DefaultStore::new(1);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "a".into(),
                user: "u".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(store.contains_key(1));
        std::thread::sleep(Duration::from_millis(1100));
        assert!(!store.contains_key(1));
    }

    #[test]
    fn remove_process() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            2,
            ProcessInfo {
                ppid: 1,
                cmd: "bash".into(),
                user: "root".into(),
                tgid: 2,
                start_time_ns: 0,
            },
        );

        assert_eq!(store.len(), 2);
        assert!(store.contains_key(2));

        let removed = store.remove_process(2);
        assert!(removed.is_some());
        assert_eq!(store.len(), 1);
        assert!(!store.contains_key(2));
    }

    #[test]
    fn children_of() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            100,
            ProcessInfo {
                ppid: 1,
                cmd: "a".into(),
                user: "root".into(),
                tgid: 100,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            200,
            ProcessInfo {
                ppid: 1,
                cmd: "b".into(),
                user: "root".into(),
                tgid: 200,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            300,
            ProcessInfo {
                ppid: 100,
                cmd: "c".into(),
                user: "root".into(),
                tgid: 300,
                start_time_ns: 0,
            },
        );

        let mut kids = store.children_of(1);
        kids.sort();
        assert_eq!(kids, vec![100, 200]);

        let kids_100 = store.children_of(100);
        assert_eq!(kids_100, vec![300]);

        assert!(store.children_of(999).is_empty());
    }
}
