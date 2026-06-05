//! Tests for cache behavior: TTL, PID reuse detection, capacity.

use proc_tree::*;
use std::time::Duration;

#[test]
fn cache_populated_after_snapshot() {
    let mut tree = ProcTree::builder()
        .cache_capacity(1024)
        .cache_ttl(Duration::from_secs(60))
        .build();
    assert_eq!(tree.cache_len(), 0, "cache should be empty before snapshot");
    tree.snapshot();
    assert!(tree.cache_len() > 0, "cache should be populated after snapshot");
}

#[test]
fn cache_hit_on_resolve() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    // First resolve populates cache
    let info1 = tree.resolve(1).unwrap();
    // Second resolve should hit cache
    let info2 = tree.resolve(1).unwrap();
    assert_eq!(info1.cmd, info2.cmd);
    assert_eq!(info1.ppid, info2.ppid);
}

#[test]
fn cache_updated_on_exec_event() {
    let tree = ProcTree::builder().build();
    // Fork creates tree node
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 200,
        parent_pid: 100,
        timestamp_ns: 12345,
    });
    // Exec updates cache with cmd info
    // Note: this reads from /proc, so 200 must exist or it'll use "unknown"
    tree.handle_event(&ProcEvent::Exec {
        pid: 200,
        timestamp_ns: 12345,
    });
    // Cache should have entry for PID 200 (even if "unknown")
    let info = tree.resolve(200);
    assert!(info.is_some(), "PID 200 should be resolvable after exec");
}

#[test]
fn cache_preserves_on_exit() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    // Get info before exit
    let info_before = tree.resolve(1).unwrap();
    // Exit event should not remove from cache
    tree.handle_event(&ProcEvent::Exit { pid: 1 });
    let info_after = tree.resolve(1).unwrap();
    assert_eq!(info_before.cmd, info_after.cmd);
}

#[test]
fn tree_len_tracks_entries() {
    let tree = ProcTree::builder().build();
    assert_eq!(tree.tree_len(), 0);
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 100,
        parent_pid: 1,
        timestamp_ns: 0,
    });
    assert_eq!(tree.tree_len(), 1);
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 200,
        parent_pid: 1,
        timestamp_ns: 0,
    });
    assert_eq!(tree.tree_len(), 2);
}

#[test]
fn builder_custom_capacity() {
    let tree = ProcTree::builder()
        .tree_capacity(100)
        .cache_capacity(200)
        .build();
    assert_eq!(tree.tree_len(), 0);
    assert_eq!(tree.cache_len(), 0);
}
