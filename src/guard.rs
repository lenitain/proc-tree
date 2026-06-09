//! Deferred process removal for exited processes.
//!
//! When a process exits, `handle_event` returns a `ProcessExitGuard` that
//! keeps the process info in the store until the guard is dropped. This
//! allows callers to access process info (cmd, user, chain) after the exit
//! event but before removal.
//!
//! # Why deferred removal?
//!
//! In event-driven systems, file events (fanotify) may arrive after the
//! proc connector exit event. If process info is removed immediately on
//! exit, these file events would lose access to process info.
//!
//! # How it works
//!
//! 1. When a process exits, `handle_event` returns a `ProcessExitGuard`
//! 2. The guard keeps the process info in the store
//! 3. When the guard is dropped (goes out of scope), the process info is
//!    automatically removed from the store
//! 4. Callers can also call `.remove()` for explicit removal
//!
//! # Example: Basic usage
//!
//! ```rust
//! use proc_tree::{DefaultStore, handle_event, ProcEvent, ProcessStore};
//!
//! let store = DefaultStore::new(0);
//!
//! // Create a process
//! handle_event(&store, &ProcEvent::Fork {
//!     child_pid: 100,
//!     parent_pid: 1,
//!     timestamp_ns: 0,
//! });
//!
//! // Exit returns a guard (process info stays in store)
//! let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
//!
//! // Process info is still accessible
//! assert!(store.get_process(100).is_some());
//! assert_eq!(store.get_process(100).unwrap().ppid, 1);
//!
//! // When guard is dropped, process info is removed
//! drop(guard);
//! assert!(store.get_process(100).is_none());
//! ```
//!
//! # Example: Batch processing
//!
//! ```rust
//! use proc_tree::{DefaultStore, handle_events, ProcEvent, ProcessStore};
//!
//! let store = DefaultStore::new(0);
//!
//! // Create processes
//! handle_events(&store, &[
//!     ProcEvent::Fork { child_pid: 100, parent_pid: 1, timestamp_ns: 0 },
//!     ProcEvent::Fork { child_pid: 200, parent_pid: 1, timestamp_ns: 0 },
//! ]);
//!
//! // Exit returns guards (process info stays in store)
//! let guards = handle_events(&store, &[
//!     ProcEvent::Exit { pid: 100 },
//!     ProcEvent::Exit { pid: 200 },
//! ]);
//!
//! // Process info is still accessible
//! assert!(store.get_process(100).is_some());
//! assert!(store.get_process(200).is_some());
//!
//! // When guards are dropped, process info is removed
//! drop(guards);
//! assert!(store.get_process(100).is_none());
//! assert!(store.get_process(200).is_none());
//! ```
//!
//! # Example: Explicit removal
//!
//! ```rust
//! use proc_tree::{DefaultStore, handle_event, ProcEvent, ProcessStore};
//!
//! let store = DefaultStore::new(0);
//!
//! handle_event(&store, &ProcEvent::Fork {
//!     child_pid: 100,
//!     parent_pid: 1,
//!     timestamp_ns: 0,
//! });
//!
//! let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
//!
//! // Explicit removal (optional)
//! guard.remove();
//! assert!(store.get_process(100).is_none());
//! ```

use crate::traits::ProcessStore;

/// Guard that keeps exited process info in the store until dropped.
///
/// When dropped, automatically removes the process from the store.
/// Use `.remove()` for explicit removal before the guard goes out of scope.
///
/// See [module documentation](crate::guard) for detailed usage examples.
pub struct ProcessExitGuard<S: ProcessStore> {
    store: S,
    pid: u32,
    removed: bool,
}

impl<S: ProcessStore> ProcessExitGuard<S> {
    /// Create a new guard for an exited process.
    pub(crate) fn new(store: S, pid: u32) -> Self {
        Self {
            store,
            pid,
            removed: false,
        }
    }

    /// Get the PID of the exited process.
    ///
    /// # Example
    ///
    /// ```rust
    /// use proc_tree::{DefaultStore, handle_event, ProcEvent};
    ///
    /// let store = DefaultStore::new(0);
    /// handle_event(&store, &ProcEvent::Fork {
    ///     child_pid: 100,
    ///     parent_pid: 1,
    ///     timestamp_ns: 0,
    /// });
    ///
    /// let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
    /// assert_eq!(guard.pid(), 100);
    /// ```
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Explicitly remove the process from the store.
    ///
    /// This is optional - the process will be removed automatically when the
    /// guard is dropped. Call this if you want to remove the process earlier.
    ///
    /// # Example
    ///
    /// ```rust
    /// use proc_tree::{DefaultStore, handle_event, ProcEvent, ProcessStore};
    ///
    /// let store = DefaultStore::new(0);
    /// handle_event(&store, &ProcEvent::Fork {
    ///     child_pid: 100,
    ///     parent_pid: 1,
    ///     timestamp_ns: 0,
    /// });
    ///
    /// let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
    /// assert!(store.get_process(100).is_some());
    ///
    /// guard.remove();
    /// assert!(store.get_process(100).is_none());
    /// ```
    pub fn remove(mut self) {
        self.store.remove_process(self.pid);
        self.removed = true;
    }
}

impl<S: ProcessStore> Drop for ProcessExitGuard<S> {
    fn drop(&mut self) {
        if !self.removed {
            self.store.remove_process(self.pid);
        }
    }
}

impl<S: ProcessStore> std::fmt::Debug for ProcessExitGuard<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessExitGuard")
            .field("pid", &self.pid)
            .field("removed", &self.removed)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_store::DefaultStore;
    use crate::types::ProcessInfo;

    #[test]
    fn guard_removes_on_drop() {
        let store = DefaultStore::new(0);
        store.insert_process(
            100,
            ProcessInfo {
                ppid: 1,
                cmd: "test".into(),
                user: "root".into(),
                tgid: 100,
                start_time_ns: 0,
            },
        );
        assert!(store.get_process(100).is_some());

        {
            let _guard = ProcessExitGuard::new(store.clone(), 100);
            assert!(store.get_process(100).is_some()); // Still accessible
        } // _guard dropped here

        assert!(store.get_process(100).is_none()); // Removed
    }

    #[test]
    fn guard_explicit_remove() {
        let store = DefaultStore::new(0);
        store.insert_process(
            100,
            ProcessInfo {
                ppid: 1,
                cmd: "test".into(),
                user: "root".into(),
                tgid: 100,
                start_time_ns: 0,
            },
        );

        let guard = ProcessExitGuard::new(store.clone(), 100);
        assert!(store.get_process(100).is_some());

        guard.remove();
        assert!(store.get_process(100).is_none());
    }

    #[test]
    fn guard_pid() {
        let store = DefaultStore::new(0);
        let guard = ProcessExitGuard::new(store, 42);
        assert_eq!(guard.pid(), 42);
    }

    #[test]
    fn guard_debug() {
        let store = DefaultStore::new(0);
        let guard = ProcessExitGuard::new(store, 42);
        let debug = format!("{:?}", guard);
        assert!(debug.contains("ProcessExitGuard"));
        assert!(debug.contains("42"));
    }
}
