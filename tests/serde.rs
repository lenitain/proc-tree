//! Integration tests for serde feature.

#![cfg(feature = "serde")]

use proc_tree::ProcessInfo;

#[test]
fn serde_roundtrip_process_info() {
    let info = ProcessInfo::new("bash".into(), "root".into(), 1, 100, 1234567890);

    let json = serde_json::to_string(&info).unwrap();
    let parsed: ProcessInfo = serde_json::from_str(&json).unwrap();

    assert_eq!(info, parsed);
    assert_eq!(parsed.cmd(), "bash");
    assert_eq!(parsed.user(), "root");
    assert_eq!(parsed.ppid(), 1);
    assert_eq!(parsed.tgid(), 100);
    assert_eq!(parsed.start_time_ns(), 1234567890);
}

#[test]
fn serde_json_format() {
    let info = ProcessInfo::new("sshd".into(), "root".into(), 0, 50, 999);

    let json = serde_json::to_string_pretty(&info).unwrap();
    assert!(json.contains("\"cmd\""));
    assert!(json.contains("\"user\""));
    assert!(json.contains("\"ppid\""));
    assert!(json.contains("\"tgid\""));
    assert!(json.contains("\"start_time_ns\""));
}
