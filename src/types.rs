//! Data types for the process tree.

/// Process info for a single PID.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProcessInfo {
    /// Command name (from `/proc/{pid}/comm`).
    cmd: String,
    /// Username (from UID lookup via `/etc/passwd`).
    user: String,
    /// Parent PID.
    ppid: u32,
    /// Thread group ID.
    tgid: u32,
    /// Process start time in nanoseconds since boot.
    /// Used for PID reuse detection.
    start_time_ns: u64,
}

impl ProcessInfo {
    /// Create a new `ProcessInfo`.
    pub fn new(
        cmd: String,
        user: String,
        ppid: u32,
        tgid: u32,
        start_time_ns: u64,
    ) -> Self {
        Self {
            cmd,
            user,
            ppid,
            tgid,
            start_time_ns,
        }
    }

    /// Command name (from `/proc/{pid}/comm`).
    pub fn cmd(&self) -> &str {
        &self.cmd
    }

    /// Username (from UID lookup via `/etc/passwd`).
    pub fn user(&self) -> &str {
        &self.user
    }

    /// Parent PID.
    pub fn ppid(&self) -> u32 {
        self.ppid
    }

    /// Thread group ID.
    pub fn tgid(&self) -> u32 {
        self.tgid
    }

    /// Process start time in nanoseconds since boot.
    /// Used for PID reuse detection.
    pub fn start_time_ns(&self) -> u64 {
        self.start_time_ns
    }
}
