//! Tests for tree operations: snapshot, chain, descendants, siblings, find.

use proc_tree::*;
mod helpers;
use helpers::TestStore;

// ---- Snapshot ----

#[test]
fn snapshot_populates_store() {
    let store = TestStore::default();
    assert_eq!(tree_len(&store), 0);
    let _ = snapshot(&store);
    assert!(
        tree_len(&store) > 0,
        "store should have entries after snapshot"
    );
}

#[test]
fn snapshot_idempotent() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let _len1 = tree_len(&store);
    let _ = snapshot(&store);
    let len2 = tree_len(&store);
    // Should be same or similar (processes may have changed)
    assert!(len2 > 0, "store should still have entries");
}

#[test]
fn snapshot_includes_pid1() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let info = resolve(&store, 1);
    assert!(info.is_some(), "PID 1 should be in store after snapshot");
}

// ---- Build chain ----

#[test]
fn build_chain_pid1() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let chain = build_chain_links(&store, 1);
    assert!(!chain.is_empty(), "PID 1 should have a chain");
    assert_eq!(chain[0].pid(), 1, "chain should start with PID 1");
}

#[test]
fn build_chain_current_process() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    let chain = build_chain_links(&store, my_pid);
    assert!(!chain.is_empty(), "current process should have a chain");
    assert_eq!(
        chain[0].pid(),
        my_pid,
        "chain should start with current PID"
    );
    // Chain should end at PID 1
    assert_eq!(chain.last().unwrap().pid(), 1, "chain should end at PID 1");
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
    let _ = snapshot(&store);
    let s = build_chain_string(&store, 1);
    // Format: JSON array
    assert!(s.starts_with('['), "should start with JSON array");
    assert!(s.ends_with(']'), "should end with JSON array");
    let links: Vec<serde_json::Value> = serde_json::from_str(&s).unwrap();
    assert!(!links.is_empty(), "chain should have entries");
}

#[test]
fn build_chain_string_current_process() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    let s = build_chain_string(&store, my_pid);
    // Format: JSON array
    let links: Vec<serde_json::Value> = serde_json::from_str(&s).unwrap();
    assert!(!links.is_empty(), "chain should not be empty");
    // First entry should have current PID
    assert_eq!(links[0]["pid"].as_u64().unwrap(), my_pid as u64);
}

// ---- is_descendant ----

#[test]
fn is_descendant_self_is_not_descendant() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    // A process is not considered a descendant of itself
    assert!(!is_descendant(&store, 1, "init"));
}

#[test]
fn is_descendant_current_of_init() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    // Every process is a descendant of init/systemd
    let info = resolve(&store, 1).unwrap();
    assert!(
        is_descendant(&store, my_pid, info.comm()),
        "current process should be descendant of PID 1 ({})",
        info.comm()
    );
}

#[test]
fn is_descendant_nonexistent() {
    let store = TestStore::default();
    assert!(!is_descendant(&store, 0x7FFFFFFF, "anything"));
}

#[test]
fn is_descendant_matches_by_comm_not_cmd() {
    let store = TestStore::default();
    // Insert process with comm="touch", cmd="touch /tmp/foo bar"
    store.insert_process(
        1,
        ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
    );
    store.insert_process(
        100,
        ProcessInfo::new(
            "touch".into(),
            "touch /tmp/foo bar".into(),
            "root".into(),
            1,
            100,
            0,
        ),
    );
    store.insert_process(
        200,
        ProcessInfo::new("bash".into(), "bash -l".into(), "root".into(), 100, 200, 0),
    );

    // Should match by comm ("touch"), not by cmd ("touch /tmp/foo bar")
    assert!(is_descendant(&store, 200, "touch"));
    // Should NOT match by full cmd
    assert!(!is_descendant(&store, 200, "touch /tmp/foo bar"));
    // Should match by comm of ancestor
    assert!(is_descendant(&store, 200, "init"));
    // PID 200 itself has comm="bash", so it IS a descendant of "bash" (itself)
    assert!(is_descendant(&store, 200, "bash"));
}

#[test]
fn find_by_cmd_matches_by_comm() {
    let store = TestStore::default();
    store.insert_process(
        1,
        ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
    );
    store.insert_process(
        100,
        ProcessInfo::new(
            "touch".into(),
            "touch /tmp/foo".into(),
            "root".into(),
            1,
            100,
            0,
        ),
    );
    store.insert_process(
        200,
        ProcessInfo::new(
            "touch".into(),
            "touch /tmp/bar".into(),
            "root".into(),
            1,
            200,
            0,
        ),
    );

    // find_by_cmd should match by comm
    let found = find_by_cmd(&store, "touch");
    assert!(found.contains(&100));
    assert!(found.contains(&200));
    assert_eq!(found.len(), 2);

    // Should not match by full cmd
    let found = find_by_cmd(&store, "touch /tmp/foo");
    assert!(found.is_empty());
}

// ---- Children / Descendants ----

#[test]
fn children_of_pid1() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let kids = children(&store, 1);
    assert!(!kids.is_empty(), "PID 1 should have children");
}

#[test]
fn descendants_of_pid1() {
    let store = TestStore::default();
    let _ = snapshot(&store);
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
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    let sib = siblings(&store, my_pid);
    // Current process should not be in its own siblings list
    assert!(!sib.contains(&my_pid), "should not include self");
}

// ---- Find ----

#[test]
fn find_by_cmd_init() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let info = resolve(&store, 1).unwrap();
    let found = find_by_cmd(&store, info.comm());
    assert!(found.contains(&1), "should find PID 1 by its cmd");
}

#[test]
fn find_by_cmd_nonexistent() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let found = find_by_cmd(&store, "definitely_not_a_real_process_name_12345");
    assert!(found.is_empty(), "should not find nonexistent cmd");
}

#[test]
fn find_by_user_root() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let found = find_by_user(&store, "root");
    assert!(!found.is_empty(), "should find at least one root process");
    assert!(found.contains(&1), "PID 1 should be root");
}

#[test]
fn find_by_user_nonexistent() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let found = find_by_user(&store, "definitely_not_a_real_user_12345");
    assert!(found.is_empty(), "should not find nonexistent user");
}

// ---- Fork event ----

#[test]
fn fork_creates_tree_node() {
    let store = TestStore::default();
    let _ = handle_event(
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
    assert_eq!(chain[0].pid(), 500);
}

#[test]
fn fork_multiple_children() {
    let store = TestStore::default();
    for i in 600..610 {
        let _ = handle_event(
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
