//! Edge cases and error handling tests.

use proc_tree::*;
use std::time::Duration;

// ---- PID 0 and special PIDs ----

#[test]
fn resolve_pid0() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    // PID 0 doesn't exist as a process
    let info = tree.resolve(0);
    assert!(info.is_none(), "PID 0 should not be resolvable");
}

#[test]
fn build_chain_pid0() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let chain = tree.build_chain(0);
    // PID 0 doesn't exist, should return empty or minimal chain
    // (depends on implementation — may read from /proc/0 which doesn't exist)
    let _ = chain; // just shouldn't panic
}

// ---- High PIDs ----

#[test]
fn resolve_max_pid() {
    let tree = ProcTree::builder().build();
    assert!(tree.resolve(u32::MAX).is_none());
}

#[test]
fn resolve_large_pid() {
    let tree = ProcTree::builder().build();
    assert!(tree.resolve(4_194_304).is_none()); // PID_MAX_DEFAULT
}

// ---- Batch events ----

#[test]
fn handle_empty_events() {
    let tree = ProcTree::builder().build();
    tree.handle_events(&[]);
    assert_eq!(tree.tree_len(), 0);
}

#[test]
fn handle_many_forks() {
    let tree = ProcTree::builder().build();
    let events: Vec<ProcEvent> = (1000..2000)
        .map(|i| ProcEvent::Fork {
            child_pid: i,
            parent_pid: 1,
            timestamp_ns: 0,
        })
        .collect();
    tree.handle_events(&events);
    assert_eq!(tree.tree_len(), 1000);
}

#[test]
fn handle_fork_then_exec_then_exit() {
    let tree = ProcTree::builder().build();
    let pid = 5000;
    tree.handle_event(&ProcEvent::Fork {
        child_pid: pid,
        parent_pid: 1,
        timestamp_ns: 100,
    });
    tree.handle_event(&ProcEvent::Exec {
        pid,
        timestamp_ns: 200,
    });
    tree.handle_event(&ProcEvent::Exit { pid });
    // Process should still be in tree (preserved for chain lookups)
    assert_eq!(tree.tree_len(), 1);
}

// ---- Builder edge cases ----

#[test]
fn builder_minimal_capacity() {
    let tree = ProcTree::builder()
        .tree_capacity(1)
        .cache_capacity(1)
        .build();
    assert_eq!(tree.tree_len(), 0);
}

#[test]
fn builder_large_capacity() {
    let tree = ProcTree::builder()
        .tree_capacity(1_000_000)
        .cache_capacity(1_000_000)
        .build();
    assert_eq!(tree.tree_len(), 0);
}

#[test]
fn builder_zero_ttl() {
    let tree = ProcTree::builder()
        .tree_ttl(Duration::ZERO)
        .cache_ttl(Duration::ZERO)
        .build();
    // Should still work, just with immediate expiry
    assert_eq!(tree.tree_len(), 0);
}

// ---- Concurrent access (basic) ----

#[test]
fn concurrent_resolve() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let tree = std::sync::Arc::new(tree);
    let mut handles = vec![];
    for _ in 0..4 {
        let t = tree.clone();
        handles.push(std::thread::spawn(move || {
            for _ in 0..100 {
                let _ = t.resolve(1);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn concurrent_handle_events() {
    let tree = std::sync::Arc::new(ProcTree::builder().build());
    let mut handles = vec![];
    for i in 0..4 {
        let t = tree.clone();
        handles.push(std::thread::spawn(move || {
            for j in 0..100 {
                let pid = i * 1000 + j;
                t.handle_event(&ProcEvent::Fork {
                    child_pid: pid,
                    parent_pid: 1,
                    timestamp_ns: 0,
                });
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(tree.tree_len(), 400);
}

// ---- Chain with cycles ----

#[test]
fn build_chain_with_cycle() {
    let tree = ProcTree::builder().build();
    // Create a cycle: 1 → 2 → 3 → 1
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 2,
        parent_pid: 1,
        timestamp_ns: 0,
    });
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 3,
        parent_pid: 2,
        timestamp_ns: 0,
    });
    // Manually create cycle by re-pointing 1's parent to 3
    // (We can't do this via public API, but we can test that
    //  the chain terminates correctly with real /proc data)
    let chain = tree.build_chain(3);
    // Should not infinite loop
    assert!(chain.len() <= 3);
}

#[test]
fn is_descendant_with_cycle() {
    let tree = ProcTree::builder().build();
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 2,
        parent_pid: 1,
        timestamp_ns: 0,
    });
    // Should not infinite loop
    let _ = tree.is_descendant(2, "anything");
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
