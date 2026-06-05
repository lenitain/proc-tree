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

mod cache;
mod proc;
mod traits;
mod tree;

// Public API
pub use cache::ProcInfo;
pub use proc::read_proc_start_time_ns;
pub use traits::{
    CacheStore, PidNode, TreeStore, build_chain_links, build_chain_string, children, descendants,
    display, find_by_cmd, find_by_user, handle_event, handle_events, is_descendant, resolve,
    siblings, snapshot, tree_len,
};
pub use tree::{ProcEvent, ProcTree, ProcTreeBuilder, ProcessLink};

#[cfg(test)]
mod tests {
    use super::*;

    /// Integration test: snapshot → resolve → chain → is_descendant
    #[test]
    fn test_full_workflow() {
        let mut tree = ProcTree::builder().build();
        tree.snapshot();

        let info = tree.resolve(1).expect("PID 1 should exist");
        assert!(!info.cmd.is_empty());

        let chain = tree.build_chain(1);
        assert!(!chain.is_empty());
        assert_eq!(chain.last().unwrap().pid, 1);

        let s = tree.build_chain_string(1);
        assert!(s.contains("1|"));
    }

    /// Test that resolve works standalone
    #[test]
    fn test_standalone_resolve() {
        let mut tree = ProcTree::builder().build();
        tree.snapshot();
        let info = tree.resolve(1).expect("PID 1 should be resolvable");
        assert_eq!(info.ppid, 0);
    }
}
