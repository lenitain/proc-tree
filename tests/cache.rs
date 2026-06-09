//! Tests for resolve caching.

use proc_tree::*;
mod helpers;
use helpers::TestStore;

#[test]
fn cache_hit_on_resolve() {
    let store = TestStore::default();
    snapshot(&store);
    // First resolve populates store
    let info1 = resolve(&store, 1).unwrap();
    // Second resolve should hit store
    let info2 = resolve(&store, 1).unwrap();
    assert_eq!(info1.cmd, info2.cmd);
    assert_eq!(info1.ppid, info2.ppid);
}

#[test]
fn store_updated_on_exec_event() {
    let store = TestStore::default();
    // Fork creates process
    let _ = handle_event(
        &store,
        &ProcEvent::Fork {
            child_pid: 200,
            parent_pid: 100,
            timestamp_ns: 12345,
        },
    );
    // Exec updates store with cmd info
    // Note: this reads from /proc, so 200 must exist or it'll use "unknown"
    let _ = handle_event(
        &store,
        &ProcEvent::Exec {
            pid: 200,
            timestamp_ns: 12345,
        },
    );
    // Store should have entry for PID 200 (even if "unknown")
    let info = resolve(&store, 200);
    assert!(info.is_some(), "PID 200 should be resolvable after exec");
}

#[test]
fn store_removes_on_exit() {
    let store = TestStore::default();
    snapshot(&store);
    // Get info before exit
    let info_before = resolve(&store, 1).unwrap();
    // Exit event marks for removal but doesn't remove
    let exited = handle_event(&store, &ProcEvent::Exit { pid: 1 });
    assert_eq!(exited, Some(1));
    // Process still in store
    let info_after = resolve(&store, 1).unwrap();
    assert_eq!(info_before.cmd, info_after.cmd);
    // Caller removes the process
    store.remove_process(1);
    // After removal, resolve should fall back to /proc
    let info_after = resolve(&store, 1);
    // If PID 1 still exists in /proc, it should be resolvable
    if let Some(info_after) = info_after {
        assert_eq!(info_before.cmd, info_after.cmd);
    }
}

#[test]
fn tree_len_tracks_entries() {
    let store = TestStore::default();
    assert_eq!(tree_len(&store), 0);
    let _ = handle_event(
        &store,
        &ProcEvent::Fork {
            child_pid: 100,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    assert_eq!(tree_len(&store), 1);
    let _ = handle_event(
        &store,
        &ProcEvent::Fork {
            child_pid: 200,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    assert_eq!(tree_len(&store), 2);
}
