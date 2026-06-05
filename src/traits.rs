//! Traits for process tree operations.
//!
//! These traits allow downstream users to provide their own storage
//! implementations while reusing the process tree algorithms.

use crate::ProcInfo;

/// A node in the process tree.
#[derive(Clone, Debug)]
pub struct PidNode {
    pub ppid: u32,
    pub cmd: String,
}

/// Trait for process tree storage.
///
/// Implement this trait to provide your own storage backend
/// (e.g., moka cache, Redis, etc.).
pub trait TreeStore {
    /// Get a tree node by PID.
    fn get_node(&self, pid: u32) -> Option<PidNode>;

    /// Insert or update a tree node.
    fn insert_node(&self, pid: u32, node: PidNode);

    /// Get all PIDs in the tree.
    fn all_pids(&self) -> Vec<u32>;
}

/// Trait for process info cache.
///
/// Implement this trait to provide your own cache backend.
pub trait CacheStore {
    /// Get cached process info by PID.
    fn get_info(&self, pid: u32) -> Option<ProcInfo>;

    /// Insert or update process info.
    fn insert_info(&self, pid: u32, info: ProcInfo);
}

/// Check if `pid` is a descendant of any process whose cmd == `target_cmd`.
///
/// Works with any TreeStore implementation.
pub fn is_descendant(tree: &impl TreeStore, pid: u32, target_cmd: &str) -> bool {
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    while let Some(node) = tree.get_node(current) {
        if !visited.insert(current) {
            break;
        }
        if node.cmd == target_cmd {
            return true;
        }
        if node.ppid == 0 || current == node.ppid {
            break;
        }
        current = node.ppid;
    }
    false
}

/// Build a chain string from the process tree.
///
/// Format: `"102|touch|root;101|sh|root;100|openclaw|root;1|systemd|root"`
pub fn build_chain(tree: &impl TreeStore, cache: &impl CacheStore, pid: u32) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    loop {
        let (ppid, cmd, user) = if let Some(node) = tree.get_node(current) {
            let user = cache
                .get_info(current)
                .map(|info| info.user)
                .unwrap_or_else(|| "unknown".to_string());
            (node.ppid, node.cmd, user)
        } else {
            // Fallback: read from /proc
            match crate::proc::read_proc_status_fields(current) {
                Some((u, p, _)) => {
                    let c = crate::proc::read_proc_comm(current)
                        .unwrap_or_else(|| "unknown".to_string());
                    (p, c, u)
                }
                None => {
                    parts.push(format!("{}|unknown|unknown", current));
                    break;
                }
            }
        };

        parts.push(format!("{}|{}|{}", current, cmd, user));
        if ppid == 0 || current == ppid {
            break;
        }
        if !visited.insert(current) {
            break;
        }
        current = ppid;
    }
    parts.join(";")
}
