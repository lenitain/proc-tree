//! Traits for process tree storage backends.
//!
//! Implement `TreeStore` and `CacheStore` to provide your own storage
//! (e.g., moka cache, Redis, dashmap) while reusing the process tree
//! algorithms in [`crate::ops`].

use crate::types::{PidNode, ProcInfo};

/// Trait for process tree storage.
///
/// Implement this trait to provide your own storage backend.
pub trait TreeStore {
    /// Get a tree node by PID.
    fn get_node(&self, pid: u32) -> Option<PidNode>;

    /// Insert or update a tree node.
    fn insert_node(&self, pid: u32, node: PidNode);

    /// Remove a tree node by PID. Returns the removed node.
    fn remove_node(&self, pid: u32) -> Option<PidNode>;

    /// Get all PIDs in the tree (including historical).
    fn all_pids(&self) -> Vec<u32>;

    /// Get direct children of a PID (only active children).
    fn children_of(&self, pid: u32) -> Vec<u32>;

    /// Get only active (non-removed) PIDs.
    fn active_pids(&self) -> Vec<u32>;
}

/// Trait for process info cache.
///
/// Implement this trait to provide your own cache backend.
pub trait CacheStore {
    /// Get cached process info by PID.
    fn get_info(&self, pid: u32) -> Option<ProcInfo>;

    /// Insert or update process info.
    fn insert_info(&self, pid: u32, info: ProcInfo);
}
