//! Default storage implementations using standard library types.
//!
//! [`DefaultStore<V>`] is a generic `HashMap<Mutex>` store with optional
//! TTL-based eviction. [`DefaultTree`] and [`DefaultCache`] are type aliases.
//!
//! # Example
//!
//! ```rust
//! use proc_tree::{DefaultTree, DefaultCache, snapshot};
//!
//! let tree = DefaultTree::new(65536, 600);
//! let cache = DefaultCache::new(65536, 600);
//! snapshot(&tree, &cache);
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::{CacheStore, PidNode, ProcInfo, TreeStore};

// ---- Internal entry with optional TTL ----

struct Entry<V> {
    value: V,
    inserted_at: Instant,
}

impl<V: Clone> Clone for Entry<V> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
            inserted_at: self.inserted_at,
        }
    }
}

// ---- Shared inner ----

type Inner<V> = Arc<Mutex<HashMap<u32, Entry<V>>>>;

fn get_inner<V: Clone>(inner: &Inner<V>, pid: u32, ttl: Duration) -> Option<V> {
    let mut map = inner.lock().unwrap();
    let entry = map.get(&pid)?;
    if !ttl.is_zero() && entry.inserted_at.elapsed() >= ttl {
        map.remove(&pid);
        return None;
    }
    Some(entry.value.clone())
}

fn insert_inner<V: Clone>(inner: &Inner<V>, pid: u32, value: V) {
    let mut map = inner.lock().unwrap();
    map.insert(
        pid,
        Entry {
            value,
            inserted_at: Instant::now(),
        },
    );
}

fn len_inner<V>(inner: &Inner<V>) -> usize {
    inner.lock().unwrap().len()
}

// ---- DefaultStore<V> ----

/// Generic store backed by `HashMap<Mutex>` with optional TTL eviction.
///
/// Thread-safe via `Arc<Mutex<...>>`. Cloning shares the same data.
pub struct DefaultStore<V> {
    inner: Inner<V>,
    ttl: Duration,
}

/// Process tree store. See [`DefaultStore`].
pub type DefaultTree = DefaultStore<PidNode>;

/// Process info cache. See [`DefaultStore`].
pub type DefaultCache = DefaultStore<ProcInfo>;

impl<V: Clone> DefaultStore<V> {
    /// Create a new store with the given capacity hint and TTL in seconds.
    /// `ttl_secs = 0` means no expiration.
    pub fn new(_capacity: u64, ttl_secs: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl: Duration::from_secs(ttl_secs),
        }
    }

    /// Number of entries (including possibly-expired ones not yet evicted).
    pub fn len(&self) -> usize {
        len_inner(&self.inner)
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

impl<V: Clone> Clone for DefaultStore<V> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            ttl: self.ttl,
        }
    }
}

impl<V: Clone> Default for DefaultStore<V> {
    /// Creates a store with capacity 100 and no TTL.
    fn default() -> Self {
        Self::new(100, 0)
    }
}

impl TreeStore for DefaultTree {
    fn get_node(&self, pid: u32) -> Option<PidNode> {
        get_inner(&self.inner, pid, self.ttl)
    }

    fn insert_node(&self, pid: u32, node: PidNode) {
        insert_inner(&self.inner, pid, node);
    }

    fn all_pids(&self) -> Vec<u32> {
        self.inner.lock().unwrap().keys().copied().collect()
    }
}

impl CacheStore for DefaultCache {
    fn get_info(&self, pid: u32) -> Option<ProcInfo> {
        get_inner(&self.inner, pid, self.ttl)
    }

    fn insert_info(&self, pid: u32, info: ProcInfo) {
        insert_inner(&self.inner, pid, info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tree_insert_get() {
        let tree = DefaultTree::new(100, 0);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        let node = tree.get_node(1).unwrap();
        assert_eq!(node.ppid, 0);
        assert_eq!(node.cmd, "init");
    }

    #[test]
    fn default_tree_ttl_expired() {
        let tree = DefaultTree::new(100, 0); // ttl=0 means no expiry
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        assert!(tree.get_node(1).is_some());

        // With ttl=1, entry expires after 1 second
        let tree = DefaultTree::new(100, 1);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        assert!(tree.get_node(1).is_some());
        std::thread::sleep(Duration::from_millis(1100));
        assert!(tree.get_node(1).is_none());
    }

    #[test]
    fn default_cache_insert_get() {
        let cache = DefaultCache::new(100, 0);
        cache.insert_info(
            42,
            ProcInfo {
                cmd: "bash".into(),
                user: "root".into(),
                ppid: 1,
                tgid: 42,
                start_time_ns: 0,
            },
        );
        let info = cache.get_info(42).unwrap();
        assert_eq!(info.cmd, "bash");
        assert_eq!(info.ppid, 1);
    }

    #[test]
    fn clone_shares_data() {
        let tree = DefaultTree::new(100, 0);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        let tree2 = tree.clone();
        assert!(tree2.get_node(1).is_some());
        tree2.insert_node(
            2,
            PidNode {
                ppid: 1,
                cmd: "bash".into(),
            },
        );
        assert!(tree.get_node(2).is_some());
    }

    #[test]
    fn len_and_contains() {
        let cache = DefaultCache::new(100, 0);
        assert_eq!(cache.len(), 0);
        cache.insert_info(
            1,
            ProcInfo {
                cmd: "a".into(),
                user: "u".into(),
                ppid: 0,
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert_eq!(cache.len(), 1);
        assert!(cache.contains_key(1));
        assert!(!cache.contains_key(999));
    }

    #[test]
    fn default_cache_ttl_expired() {
        let cache = DefaultCache::new(100, 0);
        cache.insert_info(
            1,
            ProcInfo {
                cmd: "a".into(),
                user: "u".into(),
                ppid: 0,
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(cache.get_info(1).is_some());

        let cache = DefaultCache::new(100, 1);
        cache.insert_info(
            1,
            ProcInfo {
                cmd: "a".into(),
                user: "u".into(),
                ppid: 0,
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(cache.get_info(1).is_some());
        std::thread::sleep(Duration::from_millis(1100));
        assert!(cache.get_info(1).is_none());
    }

    #[test]
    fn is_empty_default() {
        let tree = DefaultTree::new(100, 0);
        assert!(tree.is_empty());
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        assert!(!tree.is_empty());

        let cache = DefaultCache::new(100, 0);
        assert!(cache.is_empty());
        cache.insert_info(
            1,
            ProcInfo {
                cmd: "a".into(),
                user: "u".into(),
                ppid: 0,
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert!(!cache.is_empty());
    }

    #[test]
    fn all_pids_returns_keys() {
        let tree = DefaultTree::new(100, 0);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "a".into(),
            },
        );
        tree.insert_node(
            2,
            PidNode {
                ppid: 1,
                cmd: "b".into(),
            },
        );
        tree.insert_node(
            3,
            PidNode {
                ppid: 1,
                cmd: "c".into(),
            },
        );
        let mut pids = tree.all_pids();
        pids.sort();
        assert_eq!(pids, vec![1, 2, 3]);
    }

    #[test]
    fn tree_ttl_contains_key_expires() {
        let tree = DefaultTree::new(100, 1);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "a".into(),
            },
        );
        assert!(tree.contains_key(1));
        std::thread::sleep(Duration::from_millis(1100));
        assert!(!tree.contains_key(1));
    }
}
