//! RAII guard for exited processes.
//!
//! When a process exits, `handle_event` returns an `ExitedProcessGuard` that
//! automatically removes the process from the store when dropped. This ensures
//! cleanup happens even if the caller forgets to call `.remove()`.

use crate::traits::ProcessStore;

/// RAII guard for an exited process.
///
/// When dropped, automatically calls `store.remove_process(pid)` to clean up.
/// Use `.remove()` for explicit removal before the guard goes out of scope.
///
/// # Example
///
/// ```rust
/// use proc_tree::{DefaultStore, handle_event, ProcEvent, ProcessStore};
///
/// let store = DefaultStore::new(0);
///
/// // Create a process
/// handle_event(&store, &ProcEvent::Fork {
///     child_pid: 100,
///     parent_pid: 1,
///     timestamp_ns: 0,
/// });
///
/// // Exit returns a guard
/// let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
///
/// // Process still accessible
/// assert!(store.get_process(100).is_some());
///
/// // Explicit removal (optional)
/// guard.remove();
/// assert!(store.get_process(100).is_none());
/// ```
///
/// # Automatic Cleanup
///
/// If you don't call `.remove()`, the process will be removed when the guard
/// is dropped (goes out of scope):
///
/// ```rust
/// use proc_tree::{DefaultStore, handle_event, ProcEvent, ProcessStore};
///
/// let store = DefaultStore::new(0);
///
/// handle_event(&store, &ProcEvent::Fork {
///     child_pid: 100,
///     parent_pid: 1,
///     timestamp_ns: 0,
/// });
///
/// {
///     let _guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
///     assert!(store.get_process(100).is_some()); // Still accessible
/// }  // _guard dropped here, process removed automatically
///
/// assert!(store.get_process(100).is_none());
/// ```
pub struct ExitedProcessGuard<S: ProcessStore> {
    store: S,
    pid: u32,
    removed: bool,
}

impl<S: ProcessStore> ExitedProcessGuard<S> {
    /// Create a new guard for an exited process.
    pub(crate) fn new(store: S, pid: u32) -> Self {
        Self {
            store,
            pid,
            removed: false,
        }
    }

    /// Get the PID of the exited process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Explicitly remove the process from the store.
    ///
    /// This is optional - the process will be removed automatically when the
    /// guard is dropped. Call this if you want to remove the process earlier.
    pub fn remove(mut self) {
        self.store.remove_process(self.pid);
        self.removed = true;
    }
}

impl<S: ProcessStore> Drop for ExitedProcessGuard<S> {
    fn drop(&mut self) {
        if !self.removed {
            self.store.remove_process(self.pid);
        }
    }
}

impl<S: ProcessStore> std::fmt::Debug for ExitedProcessGuard<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExitedProcessGuard")
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
            let _guard = ExitedProcessGuard::new(store.clone(), 100);
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

        let guard = ExitedProcessGuard::new(store.clone(), 100);
        assert!(store.get_process(100).is_some());

        guard.remove();
        assert!(store.get_process(100).is_none());
    }

    #[test]
    fn guard_pid() {
        let store = DefaultStore::new(0);
        let guard = ExitedProcessGuard::new(store, 42);
        assert_eq!(guard.pid(), 42);
    }

    #[test]
    fn guard_debug() {
        let store = DefaultStore::new(0);
        let guard = ExitedProcessGuard::new(store, 42);
        let debug = format!("{:?}", guard);
        assert!(debug.contains("ExitedProcessGuard"));
        assert!(debug.contains("42"));
    }
}
