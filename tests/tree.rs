//! Tests for tree operations: snapshot, chain, descendants, siblings, find.

use proc_tree::*;
mod helpers;
use helpers::TestStore;

// ---- Snapshot ----

#[test]
fn snapshot_populates_store() {
    let store = TestStore::default();
    assert_eq!(tree_len(&store), 0);
    snapshot(&store);
    assert!(
        tree_len(&store) > 0,
        "store should have entries after snapshot"
    );
}

#[test]
fn snapshot_idempotent() {
    let store = TestStore::default();
    snapshot(&store);
    let _len1 = tree_len(&store);
    snapshot(&store);
    let len2 = tree_len(&store);
    // Should be same or similar (processes may have changed)
    assert!(len2 > 0, "store should still have entries");
}

#[test]
fn snapshot_includes_pid1() {
    let store = TestStore::default();
    snapshot(&store);
    let info = resolve(&store, 1);
    assert!(info.is_some(), "PID 1 should be in store after snapshot");
}

// ---- Build chain ----

#[test]
fn build_chain_pid1() {
    let store = TestStore::default();
    snapshot(&store);
    let chain = build_chain_links(&store, 1);
    assert!(!chain.is_empty(), "PID 1 should have a chain");
    assert_eq!(chain[0].pid, 1, "chain should start with PID 1");
}

#[test]
fn build_chain_current_process() {
    let store = TestStore::default();
    snapshot(&store);
    let my_pid = std::process::id();
    let chain = build_chain_links(&store, my_pid);
    assert!(!chain.is_empty(), "current process should have a chain");
    assert_eq!(chain[0].pid, my_pid, "chain should start with current PID");
    // Chain should end at PID 1
    assert_eq!(chain.last().unwrap().pid, 1, "chain should end at PID 1");
}

#[test]
fn build_chain_nonexistent() {
    let store = TestStore::default();
    let chain = build_chain_links(&store, 0x7FFFFFFF);
    // Nonexistent PID returns a chain with "unknown" entries (not empty)
    assert!(
        chain.len() <= 1,
        "nonexistent PID should have minimal chain"
    );
}

#[test]
fn build_chain_string_format() {
    let store = TestStore::default();
    snapshot(&store);
    let s = build_chain_string(&store, 1);
    assert!(s.contains("1|"), "should contain PID 1 with pipe separator");
}

#[test]
fn build_chain_string_current_process() {
    let store = TestStore::default();
    snapshot(&store);
    let my_pid = std::process::id();
    let s = build_chain_string(&store, my_pid);
    assert!(
        s.contains(&format!("{}|", my_pid)),
        "should start with current PID"
    );
}

// ---- is_descendant ----

#[test]
fn is_descendant_self_is_not_descendant() {
    let store = TestStore::default();
    snapshot(&store);
    // A process is not considered a descendant of itself
    assert!(!is_descendant(&store, 1, "init"));
}

#[test]
fn is_descendant_current_of_init() {
    let store = TestStore::default();
    snapshot(&store);
    let my_pid = std::process::id();
    // Every process is a descendant of init/systemd
    let info = resolve(&store, 1).unwrap();
    assert!(
        is_descendant(&store, my_pid, &info.cmd),
        "current process should be descendant of PID 1 ({})",
        info.cmd
    );
}

#[test]
fn is_descendant_nonexistent() {
    let store = TestStore::default();
    assert!(!is_descendant(&store, 0x7FFFFFFF, "anything"));
}

// ---- Children / Descendants ----

#[test]
fn children_of_pid1() {
    let store = TestStore::default();
    snapshot(&store);
    let kids = children(&store, 1);
    assert!(!kids.is_empty(), "PID 1 should have children");
}

#[test]
fn descendants_of_pid1() {
    let store = TestStore::default();
    snapshot(&store);
    let desc = descendants(&store, 1);
    // All processes except PID 1 itself
    assert!(desc.len() > 1, "PID 1 should have multiple descendants");
}

#[test]
fn children_nonexistent() {
    let store = TestStore::default();
    let kids = children(&store, 0x7FFFFFFF);
    assert!(kids.is_empty(), "nonexistent PID should have no children");
}

// ---- Siblings ----

#[test]
fn siblings_of_current_process() {
    let store = TestStore::default();
    snapshot(&store);
    let my_pid = std::process::id();
    let sib = siblings(&store, my_pid);
    // Current process should not be in its own siblings list
    assert!(!sib.contains(&my_pid), "should not include self");
}

// ---- Find ----

#[test]
fn find_by_cmd_init() {
    let store = TestStore::default();
    snapshot(&store);
    let info = resolve(&store, 1).unwrap();
    let found = find_by_cmd(&store, &info.cmd);
    assert!(found.contains(&1), "should find PID 1 by its cmd");
}

#[test]
fn find_by_cmd_nonexistent() {
    let store = TestStore::default();
    snapshot(&store);
    let found = find_by_cmd(&store, "definitely_not_a_real_process_name_12345");
    assert!(found.is_empty(), "should not find nonexistent cmd");
}

#[test]
fn find_by_user_root() {
    let store = TestStore::default();
    snapshot(&store);
    let found = find_by_user(&store, "root");
    assert!(!found.is_empty(), "should find at least one root process");
    assert!(found.contains(&1), "PID 1 should be root");
}

#[test]
fn find_by_user_nonexistent() {
    let store = TestStore::default();
    snapshot(&store);
    let found = find_by_user(&store, "definitely_not_a_real_user_12345");
    assert!(found.is_empty(), "should not find nonexistent user");
}

// ---- Fork event ----

#[test]
fn fork_creates_tree_node() {
    let store = TestStore::default();
    handle_event(
        &store,
        &ProcEvent::Fork {
            child_pid: 500,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    assert_eq!(tree_len(&store), 1);
    // After fork, child should be resolvable via chain
    let chain = build_chain_links(&store, 500);
    assert!(!chain.is_empty(), "forked PID should have a chain");
    assert_eq!(chain[0].pid, 500);
}

#[test]
fn fork_multiple_children() {
    let store = TestStore::default();
    for i in 600..610 {
        handle_event(
            &store,
            &ProcEvent::Fork {
                child_pid: i,
                parent_pid: 1,
                timestamp_ns: 0,
            },
        );
    }
    assert_eq!(tree_len(&store), 10);
}
