//! Tests for proc module: /proc reading functions.

use proc_tree::*;
mod helpers;
use helpers::{TestCache, TestTree};

// ---- read_proc_comm (via resolve) ----

#[test]
fn resolve_pid1_exists() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    let info = resolve(&cache, 1).expect("PID 1 should exist");
    assert!(!info.cmd.is_empty(), "PID 1 should have a command name");
}

#[test]
fn resolve_pid1_is_init() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    let info = resolve(&cache, 1).unwrap();
    // PID 1 is typically "systemd" or "init"
    assert!(
        info.cmd == "systemd" || info.cmd == "init" || info.cmd == "runit",
        "PID 1 cmd should be init-like, got: {}",
        info.cmd
    );
}

#[test]
fn resolve_nonexistent_pid() {
    let cache = TestCache::default();
    assert!(
        resolve(&cache, 0x7FFFFFFF).is_none(),
        "nonexistent PID should return None"
    );
}

#[test]
fn resolve_self() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    let my_pid = std::process::id();
    let info = resolve(&cache, my_pid);
    assert!(info.is_some(), "current process should be resolvable");
}

#[test]
fn resolve_current_process_fields() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    let my_pid = std::process::id();
    let info = resolve(&cache, my_pid).unwrap();
    assert!(!info.cmd.is_empty(), "cmd should not be empty");
    assert!(!info.user.is_empty(), "user should not be empty");
    assert!(info.ppid > 0, "current process should have a parent");
    assert!(info.tgid > 0, "tgid should be > 0");
    assert!(info.start_time_ns > 0, "start_time_ns should be > 0");
}

#[test]
fn resolve_pid1_fields() {
    let tree = TestTree::default();
    let cache = TestCache::default();
    snapshot(&tree, &cache);
    let info = resolve(&cache, 1).unwrap();
    assert_eq!(info.ppid, 0, "PID 1's ppid should be 0");
    assert_eq!(info.tgid, 1, "PID 1's tgid should be 1");
    assert_eq!(info.user, "root", "PID 1 should run as root");
}
