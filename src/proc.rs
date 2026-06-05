//! Raw /proc reading — zero caching, zero state.
//!
//! Each function reads directly from /proc at call time.
//! Returns `None` if the process doesn't exist or the file can't be parsed.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Clock ticks per second (POSIX `sysconf(_SC_CLK_TCK)`).
///
/// Returns 100 as fallback — the overwhelmingly common value on Linux.
/// This is a system-wide constant that never changes at runtime.
fn clock_ticks_per_sec() -> i64 {
    // SAFETY: sysconf(_SC_CLK_TCK) is a pure read-only query with no
    // side effects, cannot fail or cause UB. It returns a system-wide
    // constant that is set at boot and never changes.
    let ticks = unsafe { libc::sysconf(libc::_SC_CLK_TCK) };
    if ticks <= 0 { 100 } else { ticks }
}

/// Read the command name for a PID from `/proc/{pid}/comm`.
///
/// Returns `None` if the process doesn't exist or the file can't be read.
pub fn read_proc_comm(pid: u32) -> Option<String> {
    std::fs::read_to_string(format!("/proc/{}/comm", pid))
        .ok()
        .map(|s| s.trim().to_string())
}

/// Read user, ppid, tgid from `/proc/{pid}/status` in one pass.
///
/// Returns `None` if the process doesn't exist or parsing fails.
pub fn read_proc_status_fields(pid: u32) -> Option<(String, u32, u32)> {
    let status = std::fs::read_to_string(format!("/proc/{}/status", pid)).ok()?;
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
pub fn read_proc_start_time_ns(pid: u32) -> u64 {
    let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
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
/// The cmdline file contains NUL-separated arguments. Returns a `Vec<String>`
/// of the command and its arguments, with empty strings filtered out.
/// Returns `None` if the process doesn't exist or the file can't be read.
pub fn read_proc_cmdline(pid: u32) -> Option<Vec<String>> {
    let bytes = std::fs::read(format!("/proc/{}/cmdline", pid)).ok()?;
    let args: Vec<String> = bytes
        .split(|&b| b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| String::from_utf8_lossy(s).into_owned())
        .collect();
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

/// Memory size info from `/proc/{pid}/statm`.
///
/// All values are in **pages**. Use [`to_bytes`](Self::to_bytes) to convert
/// to bytes using the system page size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStatm {
    /// Total program size (pages).
    pub size: u64,
    /// Resident set size (pages).
    pub resident: u64,
    /// Shared pages (file-backed, e.g. shared libraries).
    pub shared: u64,
    /// Text (code) pages.
    pub text: u64,
    /// Library pages (unused on modern Linux).
    pub lib: u64,
    /// Data + stack pages.
    pub data: u64,
    /// Dirty pages (unused on modern Linux).
    pub dt: u64,
}

impl ProcStatm {
    /// Convert page-based values to bytes using the given page size.
    pub fn to_bytes(&self, page_size: u64) -> ProcStatmBytes {
        ProcStatmBytes {
            size: self.size * page_size,
            resident: self.resident * page_size,
            shared: self.shared * page_size,
            text: self.text * page_size,
            lib: self.lib * page_size,
            data: self.data * page_size,
            dt: self.dt * page_size,
        }
    }
}

/// Memory size info in bytes (converted from pages).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcStatmBytes {
    pub size: u64,
    pub resident: u64,
    pub shared: u64,
    pub text: u64,
    pub lib: u64,
    pub data: u64,
    pub dt: u64,
}

/// Get the system page size in bytes.
pub fn page_size() -> u64 {
    // SAFETY: sysconf(_SC_PAGESIZE) is a pure read-only query.
    let ps = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if ps <= 0 { 4096 } else { ps as u64 }
}

/// Read cgroup info from `/proc/{pid}/cgroup`.
///
/// Returns the cgroup path string. On non-containerized systems, this is
/// typically "/". On containerized systems, it contains the container path.
pub fn read_proc_cgroup(pid: u32) -> Option<String> {
    let content = std::fs::read_to_string(format!("/proc/{}/cgroup", pid)).ok()?;
    // cgroup v2 format: "0::/path"
    // cgroup v1 format: "hierarchy:id:controllers:path"
    for line in content.lines() {
        if let Some(path) = line.strip_prefix("0::") {
            // cgroup v2
            return Some(path.to_string());
        }
        // cgroup v1: take the last field (path)
        if let Some(path) = line.rsplit(':').next() {
            if !path.is_empty() {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Read namespace info from `/proc/{pid}/ns/*`.
///
/// Returns device/inode pairs for each namespace type.
pub fn read_proc_namespaces(pid: u32) -> Option<ProcNamespaces> {
    let ns_dir = format!("/proc/{}/ns", pid);
    // Check if the ns directory exists
    if !std::path::Path::new(&ns_dir).is_dir() {
        return None;
    }
    let read_ns = |name: &str| -> Option<u64> {
        let path = format!("{}/{}", ns_dir, name);
        let link = std::fs::read_link(&path).ok()?;
        // Link target format: "type:[inode]"
        let s = link.to_string_lossy();
        let inode_str = s.split('[').nth(1)?.trim_end_matches(']');
        inode_str.parse().ok()
    };
    Some(ProcNamespaces {
        pid: read_ns("pid"),
        net: read_ns("net"),
        mnt: read_ns("mnt"),
        user: read_ns("user"),
        uts: read_ns("uts"),
        ipc: read_ns("ipc"),
        cgroup: read_ns("cgroup"),
    })
}

/// Namespace inode info for a process.
#[derive(Debug, Clone, Default)]
pub struct ProcNamespaces {
    pub pid: Option<u64>,
    pub net: Option<u64>,
    pub mnt: Option<u64>,
    pub user: Option<u64>,
    pub uts: Option<u64>,
    pub ipc: Option<u64>,
    pub cgroup: Option<u64>,
}

/// Read memory info from `/proc/{pid}/statm`.
///
/// Returns page-based values. Use [`page_size`] and
/// [`ProcStatm::to_bytes`] to convert to bytes.
pub fn read_proc_statm(pid: u32) -> Option<ProcStatm> {
    let content = std::fs::read_to_string(format!("/proc/{}/statm", pid)).ok()?;
    let parts: Vec<u64> = content
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    if parts.len() < 7 {
        return None;
    }
    Some(ProcStatm {
        size: parts[0],
        resident: parts[1],
        shared: parts[2],
        text: parts[3],
        lib: parts[4],
        data: parts[5],
        dt: parts[6],
    })
}

/// Convert a UID to a username by looking up `/etc/passwd`.
///
/// Results are cached after the first call. Returns `None` if the UID
/// is not found in `/etc/passwd`.
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

    // ---- read_proc_cmdline ----

    #[test]
    fn test_read_proc_cmdline_pid1() {
        let cmdline = read_proc_cmdline(1);
        assert!(cmdline.is_some(), "PID 1 should have a cmdline");
        let args = cmdline.unwrap();
        assert!(!args.is_empty(), "cmdline should have at least one arg");
        // PID 1's first arg is typically the init system (systemd, init, etc.)
        assert!(!args[0].is_empty());
    }

    #[test]
    fn test_read_proc_cmdline_nonexistent() {
        assert!(read_proc_cmdline(0x7FFFFFFF).is_none());
    }

    #[test]
    fn test_read_proc_cmdline_self() {
        // Current process should have a cmdline
        let pid = std::process::id();
        let cmdline = read_proc_cmdline(pid);
        assert!(cmdline.is_some());
        let args = cmdline.unwrap();
        // Test binary name should be in the first arg
        assert!(args[0].contains("proc") || args[0].contains("deps"),
            "expected test binary name, got: {}", args[0]);
    }

    // ---- read_proc_statm + page_size ----

    #[test]
    fn test_page_size() {
        let ps = page_size();
        assert!(ps > 0, "page size should be > 0");
        assert!(ps.is_power_of_two(), "page size should be a power of 2, got {ps}");
    }

    #[test]
    fn test_read_proc_statm_pid1() {
        let statm = read_proc_statm(1);
        assert!(statm.is_some(), "PID 1 should have statm");
        let s = statm.unwrap();
        assert!(s.resident > 0, "PID 1 should have resident pages > 0");
        assert!(s.size >= s.resident, "size should be >= resident");
    }

    #[test]
    fn test_read_proc_statm_nonexistent() {
        assert!(read_proc_statm(0x7FFFFFFF).is_none());
    }

    #[test]
    fn test_statm_to_bytes() {
        let statm = ProcStatm {
            size: 100,
            resident: 50,
            shared: 20,
            text: 10,
            lib: 0,
            data: 30,
            dt: 0,
        };
        let bytes = statm.to_bytes(4096);
        assert_eq!(bytes.size, 100 * 4096);
        assert_eq!(bytes.resident, 50 * 4096);
        assert_eq!(bytes.shared, 20 * 4096);
    }

    // ---- read_proc_cgroup ----

    #[test]
    fn test_read_proc_cgroup_pid1() {
        let cgroup = read_proc_cgroup(1);
        assert!(cgroup.is_some(), "PID 1 should have a cgroup");
        let cg = cgroup.unwrap();
        // On non-containerized systems, cgroup is typically "/"
        assert!(cg.starts_with('/') || cg.is_empty(),
            "cgroup path should start with / or be empty, got: {}", cg);
    }

    #[test]
    fn test_read_proc_cgroup_nonexistent() {
        assert!(read_proc_cgroup(0x7FFFFFFF).is_none());
    }

    // ---- read_proc_namespaces ----

    #[test]
    fn test_read_proc_namespaces_self() {
        let pid = std::process::id();
        let ns = read_proc_namespaces(pid);
        // May be None if /proc/{pid}/ns is not accessible (e.g. restricted container)
        if let Some(ns) = ns {
            // At least pid namespace should be readable on most systems
            assert!(ns.pid.is_some() || ns.net.is_some(),
                "at least one namespace should be readable");
        }
    }

    #[test]
    fn test_read_proc_namespaces_nonexistent() {
        assert!(read_proc_namespaces(0x7FFFFFFF).is_none());
    }
}
