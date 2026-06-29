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
//! snapshot(&store).expect("failed to read /proc");
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

fn get_inner(
    inner: &Inner,
    pid: u32,
    ttl: Duration,
    children_index: &Arc<Mutex<HashMap<u32, Vec<u32>>>>,
) -> Option<ProcessInfo> {
    let mut map = inner.lock().expect("lock poisoned");
    let entry = map.get(&pid)?;
    if !ttl.is_zero() && entry.inserted_at.elapsed() >= ttl {
        let info = map.remove(&pid).expect("entry should exist").value;
        // Update children index
        let mut index = children_index.lock().expect("lock poisoned");
        if let Some(children) = index.get_mut(&info.ppid()) {
            children.retain(|&c| c != pid);
        }
        // Clean up this process's own children index entry
        index.remove(&pid);
        return None;
    }
    Some(entry.value.clone())
}

fn insert_inner(inner: &Inner, pid: u32, value: ProcessInfo) {
    let mut map = inner.lock().expect("lock poisoned");
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
        self.inner.lock().expect("lock poisoned").len()
    }

    /// Returns `true` if the store contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Check if a PID exists in the store.
    ///
    /// Does not trigger TTL eviction — use [`get_process`](ProcessStore::get_process)
    /// to check existence with TTL enforcement.
    pub fn contains_key(&self, pid: u32) -> bool {
        self.inner.lock().expect("lock poisoned").contains_key(&pid)
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

impl std::fmt::Debug for DefaultStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DefaultStore")
            .field("len", &self.len())
            .field("ttl", &self.ttl)
            .finish()
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
        get_inner(&self.inner, pid, self.ttl, &self.children_index)
    }

    fn insert_process(&self, pid: u32, info: ProcessInfo) {
        let new_ppid = info.ppid();

        // Check if process already exists with different ppid
        let old_ppid = {
            let map = self.inner.lock().expect("lock poisoned");
            map.get(&pid).map(|e| e.value.ppid())
        };

        // Insert the process
        insert_inner(&self.inner, pid, info);

        // Update children index
        let mut index = self.children_index.lock().expect("lock poisoned");

        // Remove from old parent's index if ppid changed
        if let Some(old_ppid) = old_ppid
            && old_ppid != new_ppid
            && let Some(children) = index.get_mut(&old_ppid)
        {
            children.retain(|&c| c != pid);
        }

        // Add to new parent's index (avoid duplicates)
        let children = index.entry(new_ppid).or_default();
        if !children.contains(&pid) {
            children.push(pid);
        }
    }

    fn remove_process(&self, pid: u32) -> Option<ProcessInfo> {
        // Remove from inner
        let info = {
            let mut map = self.inner.lock().expect("lock poisoned");
            map.remove(&pid).map(|e| e.value)
        };

        // Remove from children index
        if let Some(ref p) = info {
            let mut index = self.children_index.lock().expect("lock poisoned");
            // Remove from parent's index
            if let Some(children) = index.get_mut(&p.ppid()) {
                children.retain(|&c| c != pid);
            }
            // Clean up this process's own children index entry
            index.remove(&pid);
        }

        info
    }

    fn all_pids(&self) -> Vec<u32> {
        self.inner
            .lock()
            .expect("lock poisoned")
            .keys()
            .copied()
            .collect()
    }

    fn for_each_child(&self, pid: u32, f: &mut dyn FnMut(u32)) {
        // Collect children first, then release lock before calling f.
        // f may call insert_process which also locks children_index.
        let children: Vec<u32> = {
            let index = self.children_index.lock().expect("lock poisoned");
            index.get(&pid).cloned().unwrap_or_default()
        };
        for child in children {
            f(child);
        }
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
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        let info = store.get_process(1).unwrap();
        assert_eq!(info.ppid(), 0);
        assert_eq!(info.cmd(), "init");
    }

    #[test]
    fn default_store_ttl_expired() {
        let store = DefaultStore::new(0); // ttl=0 means no expiry
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        assert!(store.get_process(1).is_some());

        // With ttl=1, entry expires after 1 second
        let store = DefaultStore::new(1);
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
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
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        let store2 = store.clone();
        assert!(store2.get_process(1).is_some());
        store2.insert_process(
            2,
            ProcessInfo::new("bash".into(), "bash".into(), "root".into(), 1, 2, 0),
        );
        assert!(store.get_process(2).is_some());
    }

    #[test]
    fn len_and_contains() {
        let store = DefaultStore::new(0);
        assert_eq!(store.len(), 0);
        store.insert_process(
            1,
            ProcessInfo::new("a".into(), "a".into(), "u".into(), 0, 1, 0),
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
            ProcessInfo::new("a".into(), "a".into(), "u".into(), 0, 1, 0),
        );
        assert!(!store.is_empty());
    }

    #[test]
    fn all_pids_returns_keys() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo::new("a".into(), "a".into(), "u".into(), 0, 1, 0),
        );
        store.insert_process(
            2,
            ProcessInfo::new("b".into(), "b".into(), "u".into(), 1, 2, 0),
        );
        store.insert_process(
            3,
            ProcessInfo::new("c".into(), "c".into(), "u".into(), 1, 3, 0),
        );
        let mut pids = store.all_pids();
        pids.sort();
        assert_eq!(pids, vec![1, 2, 3]);
    }

    #[test]
    fn ttl_contains_key_does_not_expire() {
        let store = DefaultStore::new(1);
        store.insert_process(
            1,
            ProcessInfo::new("a".into(), "a".into(), "u".into(), 0, 1, 0),
        );
        assert!(store.contains_key(1));
        std::thread::sleep(Duration::from_millis(1100));
        // contains_key does not trigger TTL eviction
        assert!(store.contains_key(1));
        // but get_process does
        assert!(store.get_process(1).is_none());
    }

    #[test]
    fn remove_process() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            2,
            ProcessInfo::new("bash".into(), "bash".into(), "root".into(), 1, 2, 0),
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
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            100,
            ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0),
        );
        store.insert_process(
            200,
            ProcessInfo::new("b".into(), "b".into(), "root".into(), 1, 200, 0),
        );
        store.insert_process(
            300,
            ProcessInfo::new("c".into(), "c".into(), "root".into(), 100, 300, 0),
        );

        let mut kids = store.children_of(1);
        kids.sort();
        assert_eq!(kids, vec![100, 200]);

        let kids_100 = store.children_of(100);
        assert_eq!(kids_100, vec![300]);

        assert!(store.children_of(999).is_empty());
    }

    #[test]
    fn insert_ppid_change_removes_from_old_parent() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            100,
            ProcessInfo::new("other".into(), "other".into(), "root".into(), 0, 100, 0),
        );
        store.insert_process(
            200,
            ProcessInfo::new("child".into(), "child".into(), "root".into(), 100, 200, 0),
        );

        // child 200 is under parent 100
        assert_eq!(store.children_of(100), vec![200]);
        assert!(store.children_of(1).is_empty());

        // Re-parent 200 from 100 to 1
        store.insert_process(
            200,
            ProcessInfo::new("child".into(), "child".into(), "root".into(), 1, 200, 0),
        );

        // 200 should be removed from old parent's index
        assert!(
            store.children_of(100).is_empty(),
            "old parent should have no children"
        );
        // 200 should be in new parent's index
        assert_eq!(store.children_of(1), vec![200]);
    }

    #[test]
    fn insert_same_ppid_no_duplicate() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            100,
            ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0),
        );
        // Insert same pid with same ppid again
        store.insert_process(
            100,
            ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0),
        );
        // Should not duplicate
        assert_eq!(store.children_of(1), vec![100]);
    }

    #[test]
    fn remove_process_cleans_own_children_index() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            100,
            ProcessInfo::new("parent".into(), "parent".into(), "root".into(), 1, 100, 0),
        );
        store.insert_process(
            200,
            ProcessInfo::new("child".into(), "child".into(), "root".into(), 100, 200, 0),
        );

        assert_eq!(store.children_of(100), vec![200]);

        // Remove parent 100
        store.remove_process(100);

        // children_of(100) should return empty, not stale [200]
        assert!(
            store.children_of(100).is_empty(),
            "removed process should have no children index"
        );
        // Child 200 still exists in store but parent is gone
        assert!(store.get_process(200).is_some());
    }

    #[test]
    fn ttl_expiration_cleans_own_children_index() {
        let store = DefaultStore::new(1); // 1 second TTL
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            100,
            ProcessInfo::new("parent".into(), "parent".into(), "root".into(), 1, 100, 0),
        );
        store.insert_process(
            200,
            ProcessInfo::new("child".into(), "child".into(), "root".into(), 100, 200, 0),
        );

        assert_eq!(store.children_of(100), vec![200]);

        // Wait for parent to expire
        std::thread::sleep(Duration::from_millis(1100));

        // Accessing expired parent triggers eviction
        assert!(store.get_process(100).is_none());

        // children_of(100) should return empty, not stale [200]
        assert!(
            store.children_of(100).is_empty(),
            "expired process should have no children index"
        );
    }
}
