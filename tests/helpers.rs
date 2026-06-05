//! Test helper functions for proc-tree tests.

use proc_tree::{ProcTree, ProcEvent};

/// Build a tree using events (public API only).
///
/// Creates:
/// ```text
/// PID 100 (parent) ─┬─ PID 200 (child1)
///                   └─ PID 300 (child2)
/// ```
///
/// Note: Fork events create tree nodes but no cmd. Exec reads from /proc,
/// so for integration tests we use snapshot() on real /proc instead.
pub fn tree_from_snapshot() -> ProcTree {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    tree
}

/// Process a batch of fork events to build tree structure.
pub fn apply_fork_events(tree: &ProcTree, events: &[(u32, u32)]) {
    let proc_events: Vec<ProcEvent> = events
        .iter()
        .map(|&(child, parent)| ProcEvent::Fork {
            child_pid: child,
            parent_pid: parent,
            timestamp_ns: 0,
        })
        .collect();
    tree.handle_events(&proc_events);
}
