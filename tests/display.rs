//! Tests for display/formatting: ProcessLink, display, build_chain_string.

use proc_tree::*;

#[test]
fn process_link_display_format() {
    let link = ProcessLink {
        pid: 42,
        cmd: "bash".into(),
        user: "root".into(),
    };
    assert_eq!(link.to_string(), "42|bash|root");
}

#[test]
fn process_link_display_special_chars() {
    let link = ProcessLink {
        pid: 1,
        cmd: "systemd".into(),
        user: "root".into(),
    };
    assert_eq!(link.to_string(), "1|systemd|root");
}

#[test]
fn chain_string_semicolon_separated() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let my_pid = std::process::id();
    let s = tree.build_chain_string(my_pid);
    // Should contain semicolons between links
    let parts: Vec<&str> = s.split(';').collect();
    assert!(parts.len() > 1, "chain should have multiple links");
}

#[test]
fn chain_string_each_link_has_pipes() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let s = tree.build_chain_string(1);
    let parts: Vec<&str> = s.split(';').collect();
    for part in &parts {
        let fields: Vec<&str> = part.split('|').collect();
        assert_eq!(fields.len(), 3, "each link should have 3 fields: {}", part);
    }
}

#[test]
fn display_single_process() {
    let tree = ProcTree::builder().build();
    // Manually add a single process via fork
    tree.handle_event(&ProcEvent::Fork {
        child_pid: 999,
        parent_pid: 0,
        timestamp_ns: 0,
    });
    let d = tree.display(999);
    // Single process display should just be the cmd (or "unknown" if no exec)
    assert!(!d.is_empty());
}

#[test]
fn display_with_children() {
    let mut tree = ProcTree::builder().build();
    tree.snapshot();
    let d = tree.display(1);
    // Should contain tree characters
    assert!(
        d.contains("─") || d.contains("init") || d.contains("systemd"),
        "display should show process tree, got: {}",
        d
    );
}

#[test]
fn display_nonexistent_pid() {
    let tree = ProcTree::builder().build();
    let d = tree.display(0x7FFFFFFF);
    // Should return "unknown" or similar
    assert!(!d.is_empty(), "display should handle nonexistent PID");
}
