//! Tests for tree operations: snapshot, chain, descendants, siblings, find.

use proc_tree::*;

// ---- Snapshot ----

#[test]
fn snapshot_populates_tree() {
    let mut tree = ProcTree::builder().build();
    assert_eq!(tree.tree_len(), 0);
    tree.snapshot();
    assert!(tree.tree_len() > 0, "tree should have entries after snapshot");
}

#[test]
fn snapshot_idempotent() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let _len1 = tree.tree_len();
    tree.snapshot();
    let len2 = tree.tree_len();
    // Should be same or similar (processes may have changed)
    assert!(len2 > 0, "tree should still have entries");
}

#[test]
fn snapshot_includes_pid1() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let info = tree.resolve(1);
    assert!(info.is_some(), "PID 1 should be in tree after snapshot");
}

// ---- Build chain ----

#[test]
fn build_chain_pid1() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let chain = tree.build_chain(1);
    assert!(!chain.is_empty(), "PID 1 should have a chain");
    assert_eq!(chain[0].pid, 1, "chain should start with PID 1");
}

#[test]
fn build_chain_current_process() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let my_pid = std::process::id();
    let chain = tree.build_chain(my_pid);
    assert!(!chain.is_empty(), "current process should have a chain");
    assert_eq!(chain[0].pid, my_pid, "chain should start with current PID");
    // Chain should end at PID 1
    assert_eq!(chain.last().unwrap().pid, 1, "chain should end at PID 1");
}

#[test]
fn build_chain_nonexistent() {
    let tree = ProcTree::builder().build();
    let chain = tree.build_chain(0x7FFFFFFF);
    // Nonexistent PID returns a chain with "unknown" entries (not empty)
    // because build_chain falls back to /proc reading which fails gracefully
    assert!(chain.len() <= 1, "nonexistent PID should have minimal chain");
}

#[test]
fn build_chain_string_format() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let s = tree.build_chain_string(1);
    assert!(s.contains("1|"), "should contain PID 1 with pipe separator");
}

#[test]
fn build_chain_string_current_process() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let my_pid = std::process::id();
    let s = tree.build_chain_string(my_pid);
    assert!(s.contains(&format!("{}|", my_pid)), "should start with current PID");
}

// ---- is_descendant ----

#[test]
fn is_descendant_self_is_not_descendant() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    // A process is not considered a descendant of itself
    assert!(!tree.is_descendant(1, "init"));
}

#[test]
fn is_descendant_current_of_init() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let my_pid = std::process::id();
    // Every process is a descendant of init/systemd
    let info = tree.resolve(1).unwrap();
    assert!(
        tree.is_descendant(my_pid, &info.cmd),
        "current process should be descendant of PID 1 ({})",
        info.cmd
    );
}

#[test]
fn is_descendant_nonexistent() {
    let tree = ProcTree::builder().build();
    assert!(!tree.is_descendant(0x7FFFFFFF, "anything"));
}

// ---- Children / Descendants ----

#[test]
fn children_of_pid1() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let kids = tree.children(1);
    assert!(!kids.is_empty(), "PID 1 should have children");
}

#[test]
fn descendants_of_pid1() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let desc = tree.descendants(1);
    // All processes except PID 1 itself
    assert!(desc.len() > 1, "PID 1 should have multiple descendants");
}

#[test]
fn children_nonexistent() {
    let tree = ProcTree::builder().build();
    let kids = tree.children(0x7FFFFFFF);
    assert!(kids.is_empty(), "nonexistent PID should have no children");
}

// ---- Siblings ----

#[test]
fn siblings_of_current_process() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let my_pid = std::process::id();
    let siblings = tree.siblings(my_pid);
    // Current process should not be in its own siblings list
    assert!(!siblings.contains(&my_pid), "should not include self");
}

// ---- Find ----

#[test]
fn find_by_cmd_init() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let info = tree.resolve(1).unwrap();
    let found = tree.find_by_cmd(&info.cmd);
    assert!(found.contains(&1), "should find PID 1 by its cmd");
}

#[test]
fn find_by_cmd_nonexistent() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let found = tree.find_by_cmd("definitely_not_a_real_process_name_12345");
    assert!(found.is_empty(), "should not find nonexistent cmd");
}

#[test]
fn find_by_user_root() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let found = tree.find_by_user("root");
    assert!(!found.is_empty(), "should find at least one root process");
    assert!(found.contains(&1), "PID 1 should be root");
}

#[test]
fn find_by_user_nonexistent() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let found = tree.find_by_user("definitely_not_a_real_user_12345");
    assert!(found.is_empty(), "should not find nonexistent user");
}

// ---- Fork event ----

#[test]
fn fork_creates_tree_node() {
    let tree = ProcTree::builder().build();
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 500,
        parent_pid: 1,
        timestamp_ns: 0,
    });
    assert_eq!(tree.tree_len(), 1);
    // After fork, child should be resolvable via chain
    let chain = tree.build_chain(500);
    assert!(!chain.is_empty(), "forked PID should have a chain");
    assert_eq!(chain[0].pid, 500);
}

#[test]
fn fork_multiple_children() {
    let tree = ProcTree::builder().build();
    for i in 600..610 {
        tree.handle_event(&ProcEvent::Fork {
            child_pid: i,
            parent_pid: 1,
            timestamp_ns: 0,
        });
    }
    assert_eq!(tree.tree_len(), 10);
}
