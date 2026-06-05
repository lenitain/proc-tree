//! # proc-tree
//!
//! Linux process tree: snapshot, incremental maintenance via fork/exec events,
//! ancestry chain queries, and PID reuse detection.
//!
//! ## Quick Start
//!
//! ```rust
//! use proc_tree::ProcTree;
//!
//! // Seed from /proc
//! let mut tree = ProcTree::builder().build();
//! tree.snapshot();
//!
//! // Resolve a PID
//! if let Some(info) = tree.resolve(1) {
//!     println!("PID 1: cmd={}, user={}", info.cmd, info.user);
//! }
//!
//! // Build ancestry chain
//! let chain = tree.build_chain(1234);
//! for link in &chain {
//!     println!("  {}", link); // "1234|touch|root"
//! }
//!
//! // Check descendant
//! if tree.is_descendant(1234, "nginx") {
//!     println!("PID 1234 is a descendant of nginx");
//! }
//! ```
//!
//! ## Layers
//!
//! | Module | What | When to use |
//! |--------|------|-------------|
//! | [`proc`] | Raw `/proc` reading | Just need to read one PID's info |
//! | [`ProcCache`] | PID→info TTL cache | Need PID→cmd mapping, no tree |
//! | [`ProcTree`] | Full tree + cache | Need ancestry chains, descendant checks |
//!
//! ## PID Reuse Detection
//!
//! When a process exits and its PID is reused by a new process, cached data
//! becomes stale. `ProcCache::get()` and `ProcTree::resolve()` automatically
//! detect this by comparing `start_time_ns` with the current `/proc` value.
//!
//! ## Event-Driven Updates
//!
//! ```rust
//! use proc_tree::{ProcTree, ProcEvent};
//!
//! let mut tree = ProcTree::builder().build();
//! tree.snapshot();
//!
//! // In your event loop:
//! tree.handle_events(&[
//!     ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
//!     ProcEvent::Exec { pid: 200, timestamp_ns: 1 },
//! ]);
//! ```

pub mod cache;
pub mod proc;
pub mod tree;

// Re-export main types at crate root for convenience.
pub use cache::{ProcCache, ProcInfo};
pub use proc::{ProcNamespaces, ProcStatm, ProcStatmBytes, page_size, read_proc_cgroup, read_proc_cmdline, read_proc_comm, read_proc_namespaces, read_proc_start_time_ns, read_proc_statm, read_proc_status_fields, uid_to_username};
pub use tree::{PidNode, ProcEvent, ProcTree, ProcTreeBuilder, ProcessLink};

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration test: snapshot → resolve → chain → is_descendant
    #[test]
    fn test_full_workflow() {
        let mut tree = ProcTree::builder().build();
        tree.snapshot();

        // PID 1 should always resolve
        let info = tree.resolve(1).expect("PID 1 should exist");
        assert!(!info.cmd.is_empty());

        // Build chain for PID 1
        let chain = tree.build_chain(1);
        assert!(!chain.is_empty());
        assert_eq!(chain.last().unwrap().pid, 1);

        // Chain string format
        let s = tree.build_chain_string(1);
        assert!(s.contains("1|"));
    }

    /// Test that ProcCache can be used standalone
    #[test]
    fn test_standalone_cache() {
        let cache = ProcCache::new(1024, std::time::Duration::from_secs(60));
        cache.update_from_proc(1);
        let info = cache.get(1).expect("PID 1 should be cached");
        assert_eq!(info.ppid, 0);
    }

    /// Test that proc module functions work standalone
    #[test]
    fn test_standalone_proc() {
        let comm = proc::read_proc_comm(1);
        assert!(comm.is_some());

        let fields = proc::read_proc_status_fields(1);
        assert!(fields.is_some());

        let start = proc::read_proc_start_time_ns(1);
        assert!(start > 0);

        let user = proc::uid_to_username(0);
        assert_eq!(user.as_deref(), Some("root"));
    }
}
