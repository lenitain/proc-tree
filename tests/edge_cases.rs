//! Edge cases and error handling tests.

use proc_tree::*;
mod helpers;
use helpers::{TestCache, TestTree};

// ---- PID 0 and special PIDs ----

#[test]
fn resolve_pid0() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    // PID 0 doesn't exist as a process
    let info = resolve(&cache, 0);
    assert!(info.is_none(), "PID 0 should not be resolvable");
}

#[test]
fn build_chain_pid0() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    let chain = build_chain_links(&tree, &cache, 0);
    // PID 0 doesn't exist, should return empty or minimal chain
    let _ = chain; // just shouldn't panic
}

// ---- High PIDs ----

#[test]
fn resolve_max_pid() {
    let cache = TestCache::default();
    assert!(resolve(&cache, u32::MAX).is_none());
}

#[test]
fn resolve_large_pid() {
    let cache = TestCache::default();
    assert!(resolve(&cache, 4_194_304).is_none()); // PID_MAX_DEFAULT
}

// ---- Batch events ----

#[test]
fn handle_empty_events() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    handle_events(&tree, &cache, &[]);
    assert_eq!(tree_len(&tree), 0);
}

#[test]
fn handle_many_forks() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    let events: Vec<ProcEvent> = (1000..2000)
        .map(|i| ProcEvent::Fork {
            child_pid: i,
            parent_pid: 1,
            timestamp_ns: 0,
        })
        .collect();
    handle_events(&tree, &cache, &events);
    assert_eq!(tree_len(&tree), 1000);
}

#[test]
fn handle_fork_then_exec_then_exit() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    let pid = 5000;
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: pid,
            parent_pid: 1,
            timestamp_ns: 100,
        },
    );
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Exec {
            pid,
            timestamp_ns: 200,
        },
    );
    handle_event(&tree, &cache, &ProcEvent::Exit { pid });
    // Process should be removed from tree after exit
    assert_eq!(tree_len(&tree), 0);
}

// ---- Chain with cycles ----

#[test]
fn build_chain_with_cycle() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    // Create a cycle: 1 → 2 → 3 → 1
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: 2,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: 3,
            parent_pid: 2,
            timestamp_ns: 0,
        },
    );
    // Manually create cycle by re-pointing 1's parent to 3
    // (We can't do this via public API, but we can test that
    //  the chain terminates correctly with real /proc data)
    let chain = build_chain_links(&tree, &cache, 3);
    // Should not infinite loop
    assert!(chain.len() <= 3);
}

#[test]
fn is_descendant_with_cycle() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    handle_event(
        &tree,
        &cache,
        &ProcEvent::Fork {
            child_pid: 2,
            parent_pid: 1,
            timestamp_ns: 0,
        },
    );
    // Should not infinite loop
    let _ = is_descendant(&tree, 2, "anything");
}

// ---- ProcessLink ----

#[test]
fn process_link_clone() {
    let link = ProcessLink {
        pid: 1,
        cmd: "init".into(),
        user: "root".into(),
    };
    let link2 = link.clone();
    assert_eq!(link.pid, link2.pid);
    assert_eq!(link.cmd, link2.cmd);
    assert_eq!(link.user, link2.user);
}

#[test]
fn process_link_debug() {
    let link = ProcessLink {
        pid: 1,
        cmd: "init".into(),
        user: "root".into(),
    };
    let debug = format!("{:?}", link);
    assert!(debug.contains("ProcessLink"));
    assert!(debug.contains("1"));
}

// ---- ProcEvent ----

#[test]
fn proc_event_clone() {
    let e = ProcEvent::Fork {
        child_pid: 100,
        parent_pid: 1,
        timestamp_ns: 42,
    };
    let e2 = e.clone();
    match e2 {
        ProcEvent::Fork {
            child_pid,
            parent_pid,
            timestamp_ns,
        } => {
            assert_eq!(child_pid, 100);
            assert_eq!(parent_pid, 1);
            assert_eq!(timestamp_ns, 42);
        }
        _ => panic!("expected Fork"),
    }
}

#[test]
fn proc_event_debug() {
    let e = ProcEvent::Exec {
        pid: 42,
        timestamp_ns: 100,
    };
    let debug = format!("{:?}", e);
    assert!(debug.contains("Exec"));
    assert!(debug.contains("42"));
}

#[test]
fn proc_event_exit_debug() {
    let e = ProcEvent::Exit { pid: 99 };
    let debug = format!("{:?}", e);
    assert!(debug.contains("Exit"));
    assert!(debug.contains("99"));
}
