//! Integration tests for serde feature.

#![cfg(feature = "serde")]

use proc_tree::{ProcessInfo, ProcessLink};

#[test]
fn serde_roundtrip_process_info() {
    let info = ProcessInfo::new(
        "bash".into(),
        "bash /tmp/foo".into(),
        "root".into(),
        1,
        100,
        1234567890,
    );

    let json = serde_json::to_string(&info).unwrap();
    let parsed: ProcessInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(info, parsed);
    assert_eq!(parsed.comm(), "bash");
    assert_eq!(parsed.cmd(), "bash /tmp/foo");
    assert_eq!(parsed.user(), "root");
    assert_eq!(parsed.ppid(), 1);
    assert_eq!(parsed.tgid(), 100);
    assert_eq!(parsed.start_time_ns(), 1234567890);
}

#[test]
fn serde_json_format() {
    let info = ProcessInfo::new("sshd".into(), "sshd".into(), "root".into(), 0, 50, 999);

    let json = serde_json::to_string_pretty(&info).unwrap();
    assert!(json.contains("\"comm\""));
    assert!(json.contains("\"cmd\""));
    assert!(json.contains("\"user\""));
    assert!(json.contains("\"ppid\""));
    assert!(json.contains("\"tgid\""));
    assert!(json.contains("\"start_time_ns\""));
}

#[test]
fn process_info_comm_vs_cmd() {
    // comm is binary name, cmd is full command line
    let info = ProcessInfo::new(
        "touch".into(),
        "touch /tmp/foo bar".into(),
        "root".into(),
        0,
        1,
        0,
    );
    assert_eq!(info.comm(), "touch");
    assert_eq!(info.cmd(), "touch /tmp/foo bar");
    // comm != cmd in this case
    assert_ne!(info.comm(), info.cmd());
}

#[test]
fn process_link_comm_getter() {
    let link = ProcessLink::new(100, "bash".into(), "bash -l".into(), "root".into());
    assert_eq!(link.pid(), 100);
    assert_eq!(link.comm(), "bash");
    assert_eq!(link.cmd(), "bash -l");
    assert_eq!(link.user(), "root");
}

#[test]
fn process_link_serde_roundtrip() {
    let link = ProcessLink::new(
        42,
        "touch".into(),
        "touch /tmp/foo".into(),
        "lenitain".into(),
    );
    let json = serde_json::to_string(&link).unwrap();
    let parsed: ProcessLink = serde_json::from_str(&json).unwrap();
    assert_eq!(link, parsed);
    assert_eq!(parsed.comm(), "touch");
    assert_eq!(parsed.cmd(), "touch /tmp/foo");
}
