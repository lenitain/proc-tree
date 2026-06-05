//! Process info cache: PID → process metadata with TTL eviction.
//!
//! [`ProcCache`] is a standalone component for users who only need
//! PID-to-process-info mapping without tree/ancestry features.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::proc::{read_proc_comm, read_proc_start_time_ns, read_proc_status_fields};

/// Cached process info for a single PID.
#[derive(Clone, Debug)]
pub struct ProcInfo {
    /// Command name (from `/proc/{pid}/comm`).
    pub cmd: String,
    /// Username (from UID lookup via `/etc/passwd`).
    pub user: String,
    /// Parent PID.
    pub ppid: u32,
    /// Thread group ID.
    pub tgid: u32,
    /// Process start time in nanoseconds since boot.
    /// Used for PID reuse detection.
    pub start_time_ns: u64,
}

struct CacheEntry {
    info: ProcInfo,
    inserted_at: Instant,
}

/// A bounded, TTL-based PID → process info cache.
///
/// Thread-safe. Entries are evicted after `ttl` or when capacity is reached.
pub(crate) struct ProcCache {
    inner: Mutex<HashMap<u32, CacheEntry>>,
    ttl: Duration,
    capacity: usize,
}


impl ProcCache {
    /// Create a new cache with the given capacity and TTL.
    pub(crate) fn new(capacity: usize, ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(HashMap::with_capacity(capacity.min(1024))),
            ttl,
            capacity,
        }
    }

    /// Get cached info for a PID with PID reuse detection.
    ///
    /// If the cached `start_time_ns` doesn't match the current `/proc` value,
    /// the entry is stale (PID was reused) and `None` is returned.
    pub(crate) fn get(&self, pid: u32) -> Option<ProcInfo> {
        let map = self.inner.lock().unwrap();
        let entry = map.get(&pid)?;
        // Check TTL
        if entry.inserted_at.elapsed() >= self.ttl {
            return None;
        }
        let info = entry.info.clone();
        drop(map);
        // PID reuse detection
        let current_start = read_proc_start_time_ns(pid);
        if current_start == 0 {
            return Some(info); // process exited, return cached
        }
        if info.start_time_ns == current_start {
            return Some(info);
        }
        // PID reused — evict stale entry
        self.inner.lock().unwrap().remove(&pid);
        None
    }

    /// Get cached info WITHOUT PID reuse detection.
    pub(crate) fn get_unchecked(&self, pid: u32) -> Option<ProcInfo> {
        let map = self.inner.lock().unwrap();
        let entry = map.get(&pid)?;
        if entry.inserted_at.elapsed() >= self.ttl {
            return None;
        }
        Some(entry.info.clone())
    }

    /// Insert or update cache entry from an Exec event.
    pub(crate) fn update_from_exec(&self, pid: u32, timestamp_ns: u64) {
        let cmd = read_proc_comm(pid).unwrap_or_else(|| "unknown".to_string());
        let (user, ppid, tgid) =
            read_proc_status_fields(pid).unwrap_or_else(|| ("unknown".to_string(), 0, 0));
        self.insert(pid, ProcInfo { cmd, user, ppid, tgid, start_time_ns: timestamp_ns });
    }

/// Insert a pre-built ProcInfo directly.
    pub(crate) fn insert(&self, pid: u32, info: ProcInfo) {
        let mut map = self.inner.lock().unwrap();
        // Evict expired entries if over capacity
        if map.len() >= self.capacity {
            map.retain(|_, e| e.inserted_at.elapsed() < self.ttl);
        }
        map.insert(pid, CacheEntry { info, inserted_at: Instant::now() });
    }

    /// Number of entries in the cache.
    pub(crate) fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_insert_and_get() {
        let cache = ProcCache::new(1024, Duration::from_secs(60));
        // Use a timestamp that matches real PID 1's start time
        let start_time = crate::proc::read_proc_start_time_ns(1);
        cache.update_from_exec(1, start_time);
        let info = cache.get(1);
        assert!(info.is_some(), "PID 1 should be cached");
        let info = info.unwrap();
        assert!(!info.cmd.is_empty());
        assert_eq!(info.ppid, 0);
    }

    #[test]
    fn test_get_nonexistent() {
        let cache = ProcCache::new(1024, Duration::from_secs(60));
        assert!(cache.get(0x7FFFFFFF).is_none());
    }

    #[test]
    fn test_len_and_empty() {
        let cache = ProcCache::new(1024, Duration::from_secs(60));
        assert!(cache.get_unchecked(1).is_none(), "should be empty initially");
        cache.update_from_exec(1, 0);
        assert!(cache.get_unchecked(1).is_some(), "entry should be retrievable");
    }
}
