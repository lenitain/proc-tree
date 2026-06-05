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

/// Snapshot all running processes from `/proc`.
///
/// Populates both the tree and cache. Call once at startup before
/// processing events.
pub fn snapshot(tree: &impl TreeStore, cache: &impl CacheStore) {
    let dir = match std::fs::read_dir("/proc") {
        Ok(d) => d,
        Err(e) => {
            eprintln!("[WARNING] proc-tree: cannot read /proc: {e}");
            return;
        }
    };
    for entry in dir.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let pid: u32 = match name_str.parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let status = match std::fs::read_to_string(format!("/proc/{}/status", pid)) {
            Ok(s) => s,
            Err(_) => continue,
        };
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
                    user =
                        crate::proc::uid_to_username(uid).unwrap_or_else(|| "unknown".to_string());
                } else {
                    user = "unknown".to_string();
                }
            } else if let Some(val) = line.strip_prefix("Tgid:") {
                tgid = val.trim().parse().unwrap_or(0);
            }
        }
        let start_time_ns = crate::proc::read_proc_start_time_ns(pid);
        tree.insert_node(
            pid,
            PidNode {
                ppid,
                cmd: cmd.clone(),
            },
        );
        cache.insert_info(
            pid,
            ProcInfo {
                cmd,
                user,
                ppid,
                tgid,
                start_time_ns,
            },
        );
    }
}

/// Resolve a PID to its process info.
///
/// Checks the cache first, then falls back to reading `/proc` directly.
pub fn resolve(cache: &impl CacheStore, pid: u32) -> Option<ProcInfo> {
    // Try cache first
    if let Some(info) = cache.get_info(pid) {
        return Some(info);
    }
    // Fallback: read /proc directly
    let cmd = crate::proc::read_proc_comm(pid)?;
    let (user, ppid, tgid) =
        crate::proc::read_proc_status_fields(pid).unwrap_or_else(|| ("unknown".to_string(), 0, 0));
    let start_time_ns = crate::proc::read_proc_start_time_ns(pid);
    let info = ProcInfo {
        cmd,
        user,
        ppid,
        tgid,
        start_time_ns,
    };
    // Populate cache for future lookups
    cache.insert_info(pid, info.clone());
    Some(info)
}

/// Handle a batch of process lifecycle events.
pub fn handle_events(tree: &impl TreeStore, cache: &impl CacheStore, events: &[crate::ProcEvent]) {
    for event in events {
        handle_event(tree, cache, event);
    }
}

/// Handle a single process lifecycle event.
pub fn handle_event(tree: &impl TreeStore, cache: &impl CacheStore, event: &crate::ProcEvent) {
    match event {
        crate::ProcEvent::Fork {
            child_pid,
            parent_pid,
            ..
        } => {
            tree.insert_node(
                *child_pid,
                PidNode {
                    ppid: *parent_pid,
                    cmd: String::new(),
                },
            );
        }
        crate::ProcEvent::Exec { pid, timestamp_ns } => {
            let cmd = crate::proc::read_proc_comm(*pid).unwrap_or_else(|| "unknown".to_string());
            let (user, ppid, tgid) = crate::proc::read_proc_status_fields(*pid)
                .unwrap_or_else(|| ("unknown".to_string(), 0, 0));
            tree.insert_node(
                *pid,
                PidNode {
                    ppid,
                    cmd: cmd.clone(),
                },
            );
            cache.insert_info(
                *pid,
                ProcInfo {
                    cmd,
                    user,
                    ppid,
                    tgid,
                    start_time_ns: *timestamp_ns,
                },
            );
        }
        crate::ProcEvent::Exit { .. } => {
            // Keep the node — still valid for historical chain lookups
        }
    }
}

/// Check if `pid` is a descendant of any process whose cmd == `target_cmd`.
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

/// Build a chain of ProcessLink from the process tree.
pub fn build_chain_links(
    tree: &impl TreeStore,
    cache: &impl CacheStore,
    pid: u32,
) -> Vec<crate::ProcessLink> {
    let mut parts = Vec::new();
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
            match crate::proc::read_proc_status_fields(current) {
                Some((u, p, _)) => {
                    let c = crate::proc::read_proc_comm(current)
                        .unwrap_or_else(|| "unknown".to_string());
                    (p, c, u)
                }
                None => {
                    parts.push(crate::ProcessLink {
                        pid: current,
                        cmd: "unknown".to_string(),
                        user: "unknown".to_string(),
                    });
                    break;
                }
            }
        };
        parts.push(crate::ProcessLink {
            pid: current,
            cmd,
            user,
        });
        if ppid == 0 || current == ppid {
            break;
        }
        if !visited.insert(current) {
            break;
        }
        current = ppid;
    }
    parts
}

/// Build a chain string from the process tree.
///
/// Format: `"102|touch|root;101|sh|root;100|openclaw|root;1|systemd|root"`
pub fn build_chain_string(tree: &impl TreeStore, cache: &impl CacheStore, pid: u32) -> String {
    build_chain_links(tree, cache, pid)
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Get direct children of a PID.
pub fn children(tree: &impl TreeStore, pid: u32) -> Vec<u32> {
    tree.all_pids()
        .into_iter()
        .filter(|&p| tree.get_node(p).map(|n| n.ppid == pid).unwrap_or(false))
        .collect()
}

/// Get all descendants of a PID (BFS traversal.
pub fn descendants(tree: &impl TreeStore, pid: u32) -> Vec<u32> {
    let mut result = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(pid);
    while let Some(current) = queue.pop_front() {
        let kids = children(tree, current);
        for kid in kids {
            result.push(kid);
            queue.push_back(kid);
        }
    }
    result
}

/// Get siblings of a PID (processes with the same parent).
///
/// Excludes the given pid itself.
pub fn siblings(tree: &impl TreeStore, pid: u32) -> Vec<u32> {
    let ppid = match tree.get_node(pid) {
        Some(node) => node.ppid,
        None => return Vec::new(),
    };
    children(tree, ppid)
        .into_iter()
        .filter(|&c| c != pid)
        .collect()
}

/// Find all PIDs whose cmd matches the given string.
pub fn find_by_cmd(tree: &impl TreeStore, target_cmd: &str) -> Vec<u32> {
    tree.all_pids()
        .into_iter()
        .filter(|&pid| {
            let cmd = tree
                .get_node(pid)
                .map(|n| n.cmd)
                .filter(|c| !c.is_empty())
                .or_else(|| crate::proc::read_proc_comm(pid));
            cmd.as_deref() == Some(target_cmd)
        })
        .collect()
}

/// Find all PIDs whose user matches the given string.
pub fn find_by_user(tree: &impl TreeStore, cache: &impl CacheStore, target_user: &str) -> Vec<u32> {
    tree.all_pids()
        .into_iter()
        .filter(|&pid| {
            let user = cache
                .get_info(pid)
                .map(|info| info.user)
                .or_else(|| crate::proc::read_proc_status_fields(pid).map(|(u, _, _)| u));
            user.as_deref() == Some(target_user)
        })
        .collect()
}

/// Render a pstree-style display starting from the given root PID.
pub fn display(tree: &impl TreeStore, root_pid: u32) -> String {
    let cmd = tree
        .get_node(root_pid)
        .map(|n| n.cmd)
        .filter(|c| !c.is_empty())
        .or_else(|| crate::proc::read_proc_comm(root_pid))
        .unwrap_or_else(|| "unknown".to_string());
    let kids = children(tree, root_pid);
    if kids.is_empty() {
        return cmd;
    }
    let mut output = cmd;
    for (i, &kid) in kids.iter().enumerate() {
        let is_last = i == kids.len() - 1;
        let prefix = if is_last { "└─" } else { "├─" };
        let continuation = if is_last { "  " } else { "│ " };
        let sub = display_inner(tree, kid);
        let lines: Vec<&str> = sub.lines().collect();
        if i == 0 {
            output.push_str(&format!("─{}", lines[0]));
        } else {
            output.push('\n');
            output.push_str(prefix);
            output.push_str(lines[0]);
        }
        for line in &lines[1..] {
            output.push('\n');
            output.push_str(continuation);
            output.push_str(line);
        }
    }
    output
}

fn display_inner(tree: &impl TreeStore, pid: u32) -> String {
    let cmd = tree
        .get_node(pid)
        .map(|n| n.cmd)
        .filter(|c| !c.is_empty())
        .or_else(|| crate::proc::read_proc_comm(pid))
        .unwrap_or_else(|| "unknown".to_string());
    let kids = children(tree, pid);
    if kids.is_empty() {
        return cmd;
    }
    let mut output = cmd;
    for (i, &kid) in kids.iter().enumerate() {
        let is_last = i == kids.len() - 1;
        let prefix = if is_last { "└─" } else { "├─" };
        let continuation = if is_last { "  " } else { "│ " };
        let sub = display_inner(tree, kid);
        let lines: Vec<&str> = sub.lines().collect();
        if i == 0 {
            output.push_str(&format!("─{}", lines[0]));
        } else {
            output.push('\n');
            output.push_str(prefix);
            output.push_str(lines[0]);
        }
        for line in &lines[1..] {
            output.push('\n');
            output.push_str(continuation);
            output.push_str(line);
        }
    }
    output
}

/// Get the number of entries in the tree.
pub fn tree_len(tree: &impl TreeStore) -> u64 {
    tree.all_pids().len() as u64
}
