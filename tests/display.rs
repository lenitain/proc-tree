//! Tests for display/formatting: ProcessLink, display, build_chain_string.

use proc_tree::*;
mod helpers;
use helpers::TestStore;

#[test]
fn process_link_display_format() {
    let link = ProcessLink::new(42, "bash".into(), "bash".into(), "root".into());
    assert_eq!(link.to_string(), "42|bash|root");
}

#[test]
fn process_link_display_special_chars() {
    let link = ProcessLink::new(1, "systemd".into(), "systemd".into(), "root".into());
    assert_eq!(link.to_string(), "1|systemd|root");
}

#[test]
fn chain_string_json_array() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let my_pid = std::process::id();
    let s = build_chain_string(&store, my_pid);
    // Should be valid JSON array
    let links: Vec<serde_json::Value> = serde_json::from_str(&s).unwrap();
    assert!(links.len() > 1, "chain should have multiple links");
}

#[test]
fn chain_string_each_link_has_required_fields() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let s = build_chain_string(&store, 1);
    let links: Vec<serde_json::Value> = serde_json::from_str(&s).unwrap();
    for link in &links {
        assert!(link.get("pid").is_some(), "link should have pid field");
        assert!(link.get("comm").is_some(), "link should have comm field");
        assert!(link.get("cmd").is_some(), "link should have cmd field");
        assert!(link.get("user").is_some(), "link should have user field");
    }
}

#[test]
fn display_single_process() {
    let store = TestStore::default();
    // Manually add a single process via fork
    let _ = handle_event(
        &store,
        &ProcEvent::Fork {
            child_pid: 999,
            parent_pid: 0,
            timestamp_ns: 0,
        },
    );
    let d = display(&store, 999);
    // Single process display should just be the cmd (or "unknown" if no exec)
    assert!(!d.is_empty());
}

#[test]
fn display_with_children() {
    let store = TestStore::default();
    let _ = snapshot(&store);
    let d = display(&store, 1);
    // Should contain tree characters
    assert!(
        d.contains("─") || d.contains("init") || d.contains("systemd"),
        "display should show process tree, got: {}",
        d
    );
}

#[test]
fn display_nonexistent_pid() {
    let store = TestStore::default();
    let d = display(&store, 0x7FFFFFFF);
    // Should return "unknown" or similar
    assert!(!d.is_empty(), "display should handle nonexistent PID");
}
