//! Tests for cache behavior: PID reuse detection, resolve caching.

use proc_tree::*;
mod helpers;
use helpers::{TestCache, TestTree};

#[test]
fn cache_hit_on_resolve() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    // First resolve populates cache
    let info1 = resolve(&cache, 1).unwrap();
    // Second resolve should hit cache
    let info2 = resolve(&cache, 1).unwrap();
    assert_eq!(info1.cmd, info2.cmd);
    assert_eq!(info1.ppid, info2.ppid);
}

#[test]
fn cache_updated_on_exec_event() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    // Fork creates tree node
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: 200,
            parent_pid: 100,
            timestamp_ns: 12345,
        },
    );
    // Exec updates cache with cmd info
    // Note: this reads from /proc, so 200 must exist or it'll use "unknown"
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Exec {
            pid: 200,
            timestamp_ns: 12345,
        },
    );
    // Cache should have entry for PID 200 (even if "unknown")
    let info = resolve(&cache, 200);
    assert!(info.is_some(), "PID 200 should be resolvable after exec");
}

#[test]
fn cache_preserves_on_exit() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    // Get info before exit
    let info_before = resolve(&cache, 1).unwrap();
    // Exit event should not remove from cache
    handle_event(&tree, &cache, &ProcEvent::Exit { pid: 1 });
    let info_after = resolve(&cache, 1).unwrap();
    assert_eq!(info_before.cmd, info_after.cmd);
}

#[test]
fn tree_len_tracks_entries() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    assert_eq!(tree_len(&tree), 0);
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: 100,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    assert_eq!(tree_len(&tree), 1);
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: 200,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    assert_eq!(tree_len(&tree), 2);
}
