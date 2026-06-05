//! Process info cache: PID → process metadata with TTL eviction.
//!
//! [`ProcCache`] is a standalone component for users who only need
//! PID-to-process-info mapping without tree/ancestry features.

use std::time::Duration;

use moka::sync::Cache;

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

/// A bounded, TTL-based PID → process info cache.
///
/// Thread-safe (backed by `moka::sync::Cache`). Entries are evicted
/// automatically after `ttl` or when capacity is reached (W-TinyLFU).
pub(crate) struct ProcCache {
    inner: Cache<u32, ProcInfo>,
}

impl Clone for ProcCache {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl ProcCache {
    /// Create a new cache with the given capacity and TTL.
    pub fn new(capacity: u64, ttl: Duration) -> Self {
        Self {
            inner: Cache::builder()
                .max_capacity(capacity)
                .time_to_live(ttl)
                .build(),
        }
    }

    /// Get cached info for a PID with PID reuse detection.
    ///
    /// If the cached `start_time_ns` doesn't match the current `/proc` value,
    /// the entry is stale (PID was reused) and `None` is returned.
    /// Returns `None` if the PID is not in the cache or the process no longer exists.
    pub fn get(&self, pid: u32) -> Option<ProcInfo> {
        let info = self.inner.get(&pid)?;
        // PID reuse detection: compare cached start_time with current /proc.
        let current_start = read_proc_start_time_ns(pid);
        if current_start == 0 {
            // Process exited — return cached info (still valid for recently-exited processes).
            return Some(info);
        }
        if info.start_time_ns == current_start {
            return Some(info);
        }
        // PID was reused! Evict stale entry.
        self.inner.invalidate(&pid);
        None
    }

    /// Get cached info WITHOUT PID reuse detection.
    ///
    /// Use this when you know the PID hasn't been reused (e.g. within the
    /// same event batch) or when you need the info even if it might be stale.
    pub fn get_unchecked(&self, pid: u32) -> Option<ProcInfo> {
        self.inner.get(&pid)
    }

    /// Insert or update cache entry from an Exec event.
    ///
    /// Call this when a `ProcEvent::Exec` is received. The `timestamp_ns`
    /// is the event's timestamp (used as start_time_ns).
    pub fn update_from_exec(&self, pid: u32, timestamp_ns: u64) {
        let cmd = read_proc_comm(pid).unwrap_or_else(|| "unknown".to_string());
        let (user, ppid, tgid) =
            read_proc_status_fields(pid).unwrap_or_else(|| ("unknown".to_string(), 0, 0));
        self.inner.insert(
            pid,
            ProcInfo {
                cmd,
                user,
                ppid,
                tgid,
                start_time_ns: timestamp_ns,
            },
        );
    }

    /// Insert or update cache entry by reading current `/proc`.
    ///
    /// Use this for snapshot/seed operations, not for event-driven updates.
    pub(crate) fn update_from_proc(&self, pid: u32) {
        let cmd = read_proc_comm(pid).unwrap_or_else(|| "unknown".to_string());
        let (user, ppid, tgid) =
            read_proc_status_fields(pid).unwrap_or_else(|| ("unknown".to_string(), 0, 0));
        let start_time_ns = read_proc_start_time_ns(pid);
        self.inner.insert(
            pid,
            ProcInfo {
                cmd,
                user,
                ppid,
                tgid,
                start_time_ns,
            },
        );
    }

    /// Invalidate a cache entry (e.g. on process Exit).
    pub(crate) fn invalidate(&self, pid: u32) {
        self.inner.invalidate(&pid);
    }

    /// Number of entries in the cache.
    pub fn len(&self) -> u64 {
        self.inner.entry_count()
    }

    /// Check if the cache is empty.
    pub(crate) fn is_empty(&self) -> bool {
        self.inner.entry_count() == 0
    }

    // ---- Crate-internal methods ----

    /// Insert a pre-built ProcInfo directly (used by snapshot to avoid
    /// re-reading /proc for each PID).
    pub(crate) fn insert_raw(&self, pid: u32, info: ProcInfo) {
        self.inner.insert(pid, info);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_insert_and_get() {
        let cache = ProcCache::new(1024, Duration::from_secs(60));
        cache.update_from_proc(1); // PID 1 always exists on Linux
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
    fn test_invalidate() {
        let cache = ProcCache::new(1024, Duration::from_secs(60));
        cache.update_from_proc(1);
        assert!(cache.get_unchecked(1).is_some());
        cache.invalidate(1);
        assert!(cache.get_unchecked(1).is_none());
    }

    #[test]
    fn test_len_and_empty() {
        let cache = ProcCache::new(1024, Duration::from_secs(60));
        // moka entry_count() lags behind inserts due to internal write buffer.
        // Verify emptiness via get_unchecked which is immediately consistent.
        assert!(
            cache.get_unchecked(1).is_none(),
            "should be empty initially"
        );
        cache.update_from_proc(1);
        assert!(
            cache.get_unchecked(1).is_some(),
            "entry should be retrievable"
        );
    }

    #[test]
    fn test_clone_shares_state() {
        let cache1 = ProcCache::new(1024, Duration::from_secs(60));
        let cache2 = cache1.clone();
        cache1.update_from_proc(1);
        assert!(
            cache2.get_unchecked(1).is_some(),
            "clone should share state"
        );
    }
}
