//! Raw /proc reading for process tree construction.
//!
//! Only contains functions needed to build and maintain the process tree:
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
/// ```no_run
/// use proc_tree::proc::read_proc_comm;
///
/// let comm = read_proc_comm(1).unwrap();
/// assert!(!comm.is_empty()); // PID 1 is always init/systemd
/// assert!(read_proc_comm(0xFFFF_FFFF).is_none());
/// ```
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

/// Read user, ppid, tgid from `/proc/{pid}/status` in one pass.
///
/// Returns `None` if the process doesn't exist or parsing fails.
///
/// ```no_run
/// use proc_tree::proc::read_proc_status_fields;
///
/// let (user, ppid, tgid) = read_proc_status_fields(1).unwrap();
/// assert_eq!(user, "root");
/// assert_eq!(ppid, 0); // PID 1 has no parent
/// ```
pub fn read_proc_status_fields(pid: u32) -> Option<(String, u32, u32)> {
    let path = proc_path(pid, "status");
    let status = std::fs::read_to_string(path.as_str()).ok()?;
    let mut user = String::new();
    let mut ppid = 0u32;
    let mut tgid = 0u32;
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("Uid:") {
            let uid: u32 = val.split_whitespace().next()?.parse().ok()?;
            user = uid_to_username(uid).unwrap_or_else(|| "unknown".to_string());
        } else if let Some(val) = line.strip_prefix("PPid:") {
            ppid = val.trim().parse().ok()?;
        } else if let Some(val) = line.strip_prefix("Tgid:") {
            tgid = val.trim().parse().ok()?;
        }
    }
    Some((user, ppid, tgid))
}

/// Read the process start time in nanoseconds from `/proc/{pid}/stat`.
///
/// Returns 0 if the process doesn't exist or parsing fails.
/// The value is jiffies-since-boot converted to nanoseconds.
///
/// ```no_run
/// use proc_tree::proc::read_proc_start_time_ns;
///
/// let ns = read_proc_start_time_ns(1);
/// assert!(ns > 0); // PID 1 always has a start time
///
/// assert_eq!(read_proc_start_time_ns(0xFFFF_FFFF), 0); // nonexistent
/// ```
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

/// Parse `/proc/{pid}/status` into a `(PidNode, ProcInfo)` pair.
///
/// Returns `None` if the process doesn't exist or the status file can't be read.
pub fn parse_proc_entry(pid: u32) -> Option<(crate::types::PidNode, crate::types::ProcInfo)> {
    let path = proc_path(pid, "status");
    let status = std::fs::read_to_string(path.as_str()).ok()?;
    let mut ppid = 0u32;
    let mut cmd = String::new();
    let mut user = String::new();
    let mut tgid = 0u32;
    for line in status.lines() {
        if let Some(val) = line.strip_prefix("PPid:") {
            ppid = val.trim().parse().unwrap_or(0);
        } else if let Some(val) = line.strip_prefix("Name:") {
            cmd = val.trim().to_string();
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
    let start_time_ns = read_proc_start_time_ns(pid);
    Some((
        crate::types::PidNode { ppid, cmd: cmd.clone() },
        crate::types::ProcInfo { cmd, user, ppid, tgid, start_time_ns },
    ))
}

/// Convert a UID to a username by looking up `/etc/passwd`.
///
/// Results are cached after the first call. Returns `None` if the UID
/// is not found in `/etc/passwd`.
///
/// ```no_run
/// use proc_tree::proc::uid_to_username;
///
/// assert_eq!(uid_to_username(0).as_deref(), Some("root"));
/// assert!(uid_to_username(0xFFFF_FFFF).is_none());
/// ```
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
    fn test_read_proc_status_fields_pid1() {
        let result = read_proc_status_fields(1);
        assert!(result.is_some(), "PID 1 should have status");
        let (user, ppid, tgid) = result.unwrap();
        assert!(!user.is_empty());
        assert_eq!(ppid, 0, "PID 1's ppid should be 0");
        assert_eq!(tgid, 1, "PID 1's tgid should be 1");
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
}
