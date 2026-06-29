//! Tests for proc module: /proc reading functions.

use proc_tree::*;
mod helpers;
use helpers::TestStore;

// ---- read_proc_comm (via resolve) ----

#[test]
fn resolve_pid1_exists() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let info = resolve(&store, 1).expect("PID 1 should exist");
    assert!(!info.cmd().is_empty(), "PID 1 should have a command name");
}

#[test]
fn resolve_pid1_is_init() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let info = resolve(&store, 1).unwrap();
    // cmd is the full cmdline; extract basename for comparison
    let base = info.cmd().split_whitespace().next().unwrap_or("");
    let base = base.rsplit('/').next().unwrap_or(base);
    assert!(
        base == "systemd" || base == "init" || base == "runit",
        "PID 1 cmd should be init-like, got: {} (base: {})",
        info.cmd(),
        base
    );
}

#[test]
fn resolve_nonexistent_pid() {
    let store = TestStore::default();
    assert!(
        resolve(&store, 0x7FFFFFFF).is_none(),
        "nonexistent PID should return None"
    );
}

#[test]
fn resolve_self() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    let info = resolve(&store, my_pid);
    assert!(info.is_some(), "current process should be resolvable");
}

#[test]
fn resolve_current_process_fields() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    let info = resolve(&store, my_pid).unwrap();
    assert!(!info.cmd().is_empty(), "cmd should not be empty");
    assert!(!info.user().is_empty(), "user should not be empty");
    assert!(info.ppid() > 0, "current process should have a parent");
    assert!(info.tgid() > 0, "tgid should be > 0");
    assert!(info.start_time_ns() > 0, "start_time_ns should be > 0");
}

#[test]
fn resolve_pid1_fields() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let info = resolve(&store, 1).unwrap();
    assert_eq!(info.ppid(), 0, "PID 1's ppid should be 0");
    assert_eq!(info.tgid(), 1, "PID 1's tgid should be 1");
    assert_eq!(info.user(), "root", "PID 1 should run as root");
}
