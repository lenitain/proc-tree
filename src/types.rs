//! Data types for the process tree.

/// Cached process info for a single PID.
#[derive(Clone, Debug)]
pub struct ProcInfo {
    /// Command name (from `/proc/{pid}/comm`).
    pub cmd: String,
    /// Username (from UID lookup via `/etc/passwd`).
    pub user: String,
    /// Parent PID.
    pub ppid: u32,
    /// Thread group ID.
    pub tgid: u32,
    /// Process start time in nanoseconds since boot.
    /// Used for PID reuse detection.
    pub start_time_ns: u64,
}

/// A node in the process tree.
#[derive(Clone, Debug)]
pub struct PidNode {
    pub ppid: u32,
    pub cmd: String,
}
