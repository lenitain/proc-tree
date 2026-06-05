//! Default storage implementations using standard library types.
//!
//! `DefaultTree` and `DefaultCache` use `HashMap` behind `Mutex` with
//! optional TTL-based eviction. No external dependencies.
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

fn contains_inner<V: Clone>(inner: &Inner<V>, pid: u32, ttl: Duration) -> bool {
    get_inner(inner, pid, ttl).is_some()
}

// ---- DefaultTree ----

/// Default process tree backed by `HashMap<Mutex>`.
///
/// Thread-safe via `Arc<Mutex<...>>`. Cloning shares the same data.
pub struct DefaultTree {
    inner: Inner<PidNode>,
    ttl: Duration,
}

impl DefaultTree {
    /// Create a new tree with the given capacity hint and TTL in seconds.
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

    /// Returns `true` if the tree contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a PID exists and is not expired.
    pub fn contains_key(&self, pid: u32) -> bool {
        contains_inner(&self.inner, pid, self.ttl)
    }
}

impl Clone for DefaultTree {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            ttl: self.ttl,
        }
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

// ---- DefaultCache ----

/// Default process info cache backed by `HashMap<Mutex>`.
///
/// Thread-safe via `Arc<Mutex<...>>`. Cloning shares the same data.
pub struct DefaultCache {
    inner: Inner<ProcInfo>,
    ttl: Duration,
}

impl DefaultCache {
    /// Create a new cache with the given capacity hint and TTL in seconds.
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

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a PID exists and is not expired.
    pub fn contains_key(&self, pid: u32) -> bool {
        contains_inner(&self.inner, pid, self.ttl)
    }
}

impl Clone for DefaultCache {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            ttl: self.ttl,
        }
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
}
