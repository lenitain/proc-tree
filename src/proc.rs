//! Raw /proc reading for process tree construction.
//!
//! These are internal functions used by the process tree operations.
//! They are not part of the public API.
//!
//! Contains functions needed to build and maintain the process tree:
//! comm (cmd name), status (ppid/user/tgid), stat (start_time), uid lookup.

use std::collections::HashMap;
use std::sync::OnceLock;

use arrayvec::ArrayString;

/// Clock ticks per second (POSIX `sysconf(_SC_CLK_TCK)`).
///
/// Returns 100 as fallback — the overwhelmingly common value on Linux.
/// Cached after the first call since the value never changes at runtime.
fn clock_ticks_per_sec() -> i64 {
    static TICKS: OnceLock<i64> = OnceLock::new();
    *TICKS.get_or_init(|| {
        // SAFETY: sysconf(_SC_CLK_TCK) is a pure read-only query with no
        // side effects, cannot fail or cause UB. It returns a system-wide
        // constant that is set at boot and never changes.
        let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
        if ticks <= 0 { 100 } else { ticks }
    })
}

/// Read the command name for a PID from `/proc/{pid}/comm`.
///
/// Returns `None` if the process doesn't exist or the file can't be read.
///
/// # Internal Usage
///
/// This function is used internally by `ops::resolve` and
/// `ops::get_cmd` as a fallback when the command name is not
/// in the store.
pub fn read_proc_comm(pid: u32) -> Option<String> {
    let path = proc_path(pid, "comm");
    let mut buf = [0u8; 64];
    let mut file = std::fs::File::open(path.as_str()).ok()?;
    use std::io::Read;
    let n = file.read(&mut buf).ok()?;
    let s = std::str::from_utf8(&buf[..n]).ok()?;
    Some(s.trim().to_string())
}

/// Format `/proc/{pid}/{suffix}` into a stack-allocated string.
fn proc_path(pid: u32, suffix: &str) -> ArrayString<32> {
    use std::fmt::Write;
    let mut buf = ArrayString::new();
    write!(buf, "/proc/{pid}/{suffix}").unwrap();
    buf
}

/// Read the process start time in nanoseconds from `/proc/{pid}/stat`.
///
/// Returns 0 if the process doesn't exist or parsing fails.
/// The value is jiffies-since-boot converted to nanoseconds.
///
/// # Internal Usage
///
/// This function is used internally by [`parse_proc_entry`] to populate
/// the `start_time_ns` field of [`super::types::ProcessInfo`].
pub fn read_proc_start_time_ns(pid: u32) -> u64 {
    let path = proc_path(pid, "stat");
    let stat = match std::fs::read_to_string(path.as_str()) {
        Ok(s) => s,
        Err(_) => return 0,
    };
    // Skip comm field (which may contain spaces and ')') by finding the
    // last ')' followed by a space — this is the standard Linux convention.
    let after_comm = match stat.rfind(") ") {
        Some(pos) => pos + 2,
        None => return 0,
    };
    let mut rest = &stat[after_comm..];
    // Fields after comm: state, ppid, pgrp, session, tty_nr, tpgid,
    // flags, minflt, cminflt, majflt, cmajflt, utime, stime, cutime,
    // cstime, priority, nice, num_threads, itrealvalue, starttime
    // That's 19 fields to skip (indices 3..22, 0-indexed from after comm).
    for _ in 0..19 {
        if let Some(pos) = rest.find(' ') {
            rest = &rest[pos + 1..];
        } else {
            return 0;
        }
    }
    let starttime_jiffies: u64 = match rest.split_whitespace().next() {
        Some(s) => s.parse().unwrap_or(0),
        None => return 0,
    };
    if starttime_jiffies == 0 {
        return 0;
    }
    (starttime_jiffies as u128 * 1_000_000_000 / clock_ticks_per_sec() as u128) as u64
}

// ---- UID → username lookup ----

fn uid_passwd_map() -> &'static HashMap<u32, String> {
    static MAP: OnceLock<HashMap<u32, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        let mut map = HashMap::new();
        if let Ok(passwd) = std::fs::read_to_string("/etc/passwd") {
            for entry in passwd.lines() {
                let mut parts = entry.splitn(4, ':');
                let name = parts.next();
                let _shell = parts.next(); // password field
                let uid_str = parts.next();
                if let (Some(name), Some(uid_str)) = (name, uid_str)
                    && let Ok(uid) = uid_str.parse::<u32>()
                {
                    map.insert(uid, name.to_string());
                }
            }
        }
        map
    })
}

/// Read the full command line for a PID from `/proc/{pid}/cmdline`.
///
/// Returns `None` if the process doesn't exist or the file can't be read.
/// The null-byte separators between arguments are replaced with spaces.
/// Kernel threads (which have empty cmdline) return `None`.
pub fn read_proc_cmdline(pid: u32) -> Option<String> {
    let path = proc_path(pid, "cmdline");
    let bytes = std::fs::read(path.as_str()).ok()?;
    if bytes.is_empty() {
        return None;
    }
    // cmdline uses NUL bytes between arguments; trim trailing NUL and join with spaces.
    let s = String::from_utf8_lossy(&bytes);
    let trimmed = s.trim_end_matches('\0');
    Some(trimmed.replace('\0', " "))
}

/// Parse `/proc/{pid}/status` and `/proc/{pid}/cmdline` into a `ProcessInfo`.
///
/// The `cmd` field is populated from `/proc/{pid}/cmdline` (full command line
/// with arguments), falling back to the `Name:` field from `/proc/{pid}/status`
/// (truncated to 15 chars) for kernel threads where cmdline is empty.
///
/// Returns `None` if the process doesn't exist or the status file can't be read.
///
/// # Internal Usage
///
/// This function is used internally by:
/// - [`super::ops::snapshot`] to populate the store
/// - [`super::ops::resolve`] as a fallback
/// - [`super::ops::handle_event`] for Exec events
/// - [`super::ops::build_chain_links`] for chain lookups
/// - [`super::ops::find_by_cmd`] and [`super::ops::find_by_user`] for fallback
pub fn parse_proc_entry(pid: u32) -> Option<crate::types::ProcessInfo> {
    let path = proc_path(pid, "status");
    let status = std::fs::read_to_string(path.as_str()).ok()?;
    let mut ppid = 0u32;
    let mut name = String::new();
    let mut user = String::new();
    let mut tgid = 0u32;
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("PPid:") {
            ppid = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("Name:") {
            name = val.trim().to_string();
        } else if let Some(val) = line.strip_prefix("Uid:") {
            if let Some(uid_str) = val.split_whitespace().next()
                && let Ok(uid) = uid_str.parse::<u32>()
            {
                user = uid_to_username(uid).unwrap_or_else(|| "unknown".to_string());
            } else {
                user = "unknown".to_string();
            }
        } else if let Some(val) = line.strip_prefix("Tgid:") {
            tgid = val.trim().parse().unwrap_or(0);
        }
    }
    // Use full cmdline (e.g. "bun /home/user/.bun/bin/pi") when available,
    // fall back to short Name: field (e.g. "bun") for kernel threads.
    let cmd = read_proc_cmdline(pid).unwrap_or(name);
    let start_time_ns = read_proc_start_time_ns(pid);
    Some(crate::types::ProcessInfo {
        cmd,
        user,
        ppid,
        tgid,
        start_time_ns,
    })
}

/// Convert a UID to a username by looking up `/etc/passwd`.
///
/// Results are cached after the first call. Returns `None` if the UID
/// is not found in `/etc/passwd`.
///
/// # Internal Usage
///
/// This function is used internally by [`parse_proc_entry`] to populate
/// the `user` field of [`super::types::ProcessInfo`].
pub fn uid_to_username(uid: u32) -> Option<String> {
    uid_passwd_map().get(&uid).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_proc_comm_pid1() {
        let comm = read_proc_comm(1);
        assert!(comm.is_some(), "PID 1 should exist");
        assert!(!comm.unwrap().is_empty());
    }

    #[test]
    fn test_read_proc_comm_nonexistent() {
        assert!(read_proc_comm(0x7FFFFFFF).is_none());
    }

    #[test]
    fn test_read_proc_start_time_ns_pid1() {
        let ns = read_proc_start_time_ns(1);
        assert!(ns > 0, "PID 1 start_time_ns should be > 0, got {ns}");
    }

    #[test]
    fn test_read_proc_start_time_ns_nonexistent() {
        assert_eq!(read_proc_start_time_ns(0x7FFFFFFF), 0);
    }

    #[test]
    fn test_uid_to_username_root() {
        // root (UID 0) should always exist on Linux
        let name = uid_to_username(0);
        assert_eq!(name.as_deref(), Some("root"));
    }

    #[test]
    fn test_uid_to_username_nonexistent() {
        // UID 0xFFFFFFFF almost certainly doesn't exist
        assert!(uid_to_username(0xFFFFFFFF).is_none());
    }

    #[test]
    fn test_read_proc_cmdline_pid1() {
        let cmdline = read_proc_cmdline(1);
        assert!(cmdline.is_some(), "PID 1 should have a cmdline");
        let s = cmdline.unwrap();
        assert!(!s.is_empty());
        // PID 1 is typically "init" or "systemd" — should not contain NUL bytes
        assert!(!s.contains('\0'), "cmdline should not contain NUL bytes: {:?}", s);
    }

    #[test]
    fn test_read_proc_cmdline_nonexistent() {
        assert!(read_proc_cmdline(0x7FFFFFFF).is_none());
    }

    #[test]
    fn test_read_proc_cmdline_kernel_thread() {
        // PID 2 (kthreadd) has empty cmdline on Linux
        let cmdline = read_proc_cmdline(2);
        // Should be None for kernel threads
        assert!(cmdline.is_none(), "kernel thread PID 2 should have empty cmdline");
    }

    #[test]
    fn test_parse_proc_entry_uses_full_cmdline() {
        // PID 1 should have a full cmdline (e.g. "/sbin/init" or "/usr/lib/systemd/systemd")
        let info = parse_proc_entry(1).expect("PID 1 should exist");
        // The cmd should NOT be just "init" or "systemd" (the Name: field),
        // but the full path from cmdline
        assert!(
            info.cmd.len() > 15 || !info.cmd.contains(' '),
            "PID 1 cmd should be full cmdline, got: {:?}",
            info.cmd
        );
    }
}
