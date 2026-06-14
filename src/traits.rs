//! Traits for process tree storage backends.
//!
//! Implement `ProcessStore` to provide your own storage
//! (e.g., moka cache, Redis, dashmap) while reusing the process tree
//! algorithms in [`crate::ops`].

use crate::types::ProcessInfo;

/// Trait for process tree storage.
///
/// Implement this trait to provide your own storage backend.
pub trait ProcessStore {
    /// Get process info by PID.
    fn get_process(&self, pid: u32) -> Option<ProcessInfo>;

    /// Insert or update process info.
    fn insert_process(&self, pid: u32, info: ProcessInfo);

    /// Remove a process by PID. Returns the removed process info.
    fn remove_process(&self, pid: u32) -> Option<ProcessInfo>;

    /// Get all PIDs in the tree.
    fn all_pids(&self) -> Vec<u32>;

    /// Iterate direct children of a PID.
    ///
    /// Calls `f` for each child PID. Avoids allocating a return Vec.
    fn for_each_child(&self, pid: u32, f: &mut dyn FnMut(u32));

    /// Get direct children of a PID as a Vec.
    ///
    /// Convenience wrapper around [`for_each_child`](ProcessStore::for_each_child).
    fn children_of(&self, pid: u32) -> Vec<u32> {
        let mut v = Vec::new();
        self.for_each_child(pid, &mut |c| v.push(c));
        v
    }
}
