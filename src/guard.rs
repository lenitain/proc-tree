//! Process exit tracking via guards.
//!
//! When a process exits, `handle_event` returns a `ProcessExitGuard` that
//! records the exit. The process info **stays in the store** — it is not
//! removed automatically. This is critical for event-driven systems where
//! file events (fanotify) may arrive after the proc connector exit event.
//!
//! # Design principle
//!
//! The caller decides when to remove process info. The guard is just a
//! token that says "this process exited". Dropping the guard does nothing
//! to the store.
//!
//! # Example
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
//! // Exit returns a guard — process info stays in store
//! let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
//! assert!(store.get_process(100).is_some());
//!
//! // Dropping the guard does NOT remove the process
//! drop(guard);
//! assert!(store.get_process(100).is_some());
//!
//! // Caller explicitly removes when done
//! store.remove_process(100);
//! assert!(store.get_process(100).is_none());
//! ```

use crate::traits::ProcessStore;

/// Guard that marks an exited process.
///
/// The process info **stays in the store** until the caller explicitly
/// calls `.remove()` or `.remove_if_stale()`. Dropping the guard does
/// **not** remove the process — this ensures that events arriving after
/// the exit event can still look up process info.
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
/// // Process info is still in the store
/// assert!(store.get_process(100).is_some());
///
/// // Dropping the guard does NOT remove the process
/// drop(guard);
/// assert!(store.get_process(100).is_some());
///
/// // Explicit removal when you're done with the process
/// store.remove_process(100);
/// assert!(store.get_process(100).is_none());
/// ```
pub struct ProcessExitGuard<S: ProcessStore> {
    store: S,
    pid: u32,
}

impl<S: ProcessStore> ProcessExitGuard<S> {
    /// Create a new guard for an exited process.
    pub(crate) fn new(store: S, pid: u32) -> Self {
        Self {
            store,
            pid,
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

    /// Get a reference to the store.
    pub fn store(&self) -> &S {
        &self.store
    }
}

impl<S: ProcessStore> std::fmt::Debug for ProcessExitGuard<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessExitGuard")
            .field("pid", &self.pid)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_store::DefaultStore;
    use crate::types::ProcessInfo;

    #[test]
    fn guard_does_not_remove_on_drop() {
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
            assert!(store.get_process(100).is_some());
        } // _guard dropped here

        // Process info is still in the store after guard drop
        assert!(store.get_process(100).is_some());
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
