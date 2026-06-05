//! Process tree operations: snapshot, resolve, queries, display.
//!
//! All functions are generic over [`TreeStore`] and [`CacheStore`] so they
//! work with any storage backend.

use crate::types::ProcInfo;
use crate::traits::{CacheStore, TreeStore};
use crate::tree::{ProcEvent, ProcessLink};
use crate::types::PidNode;

/// Snapshot all running processes from `/proc`.
///
/// Populates both the tree and cache. Call once at startup before
/// processing events.
///
/// ```no_run
/// use proc_tree::{DefaultTree, DefaultCache, snapshot, TreeStore};
///
/// let tree = DefaultTree::new(65536, 600);
/// let cache = DefaultCache::new(65536, 600);
/// snapshot(&tree, &cache);
///
/// // PID 1 should always exist on Linux
/// assert!(tree.get_node(1).is_some());
/// ```
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
        if let Some((node, info)) = crate::proc::parse_proc_entry(pid) {
            tree.insert_node(pid, node);
            cache.insert_info(pid, info);
        }
    }
}

/// Resolve a PID to its process info.
///
/// Checks the cache first, then falls back to reading `/proc` directly.
///
/// ```no_run
/// use proc_tree::{DefaultTree, DefaultCache, snapshot, resolve, TreeStore};
///
/// let tree = DefaultTree::new(65536, 600);
/// let cache = DefaultCache::new(65536, 600);
/// snapshot(&tree, &cache);
///
/// let info = resolve(&cache, 1).unwrap();
/// assert!(!info.cmd.is_empty());
/// ```
pub fn resolve(cache: &impl CacheStore, pid: u32) -> Option<ProcInfo> {
    // Try cache first
    if let Some(info) = cache.get_info(pid) {
        return Some(info);
    }
    // Fallback: read /proc directly via parse_proc_entry
    let (_node, info) = crate::proc::parse_proc_entry(pid)?;
    // Populate cache for future lookups
    cache.insert_info(pid, info.clone());
    Some(info)
}

/// Handle a batch of process lifecycle events.
///
/// ```
/// use proc_tree::{DefaultTree, DefaultCache, handle_events, ProcEvent, CacheStore, TreeStore};
///
/// let tree = DefaultTree::new(100, 0);
/// let cache = DefaultCache::new(100, 0);
///
/// handle_events(&tree, &cache, &[
///     ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
/// ]);
///
/// let node = tree.get_node(200).unwrap();
/// assert_eq!(node.ppid, 100);
/// ```
pub fn handle_events(tree: &impl TreeStore, cache: &impl CacheStore, events: &[ProcEvent]) {
    for event in events {
        handle_event(tree, cache, event);
    }
}

/// Handle a single process lifecycle event.
pub fn handle_event(tree: &impl TreeStore, cache: &impl CacheStore, event: &ProcEvent) {
    match event {
        ProcEvent::Fork {
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
        ProcEvent::Exec { pid, timestamp_ns } => {
            if let Some((node, mut info)) = crate::proc::parse_proc_entry(*pid) {
                info.start_time_ns = *timestamp_ns;
                tree.insert_node(*pid, node);
                cache.insert_info(*pid, info);
            }
        }
        ProcEvent::Exit { .. } => {
            // Keep the node — still valid for historical chain lookups
        }
    }
}

/// Check if `pid` is a descendant of any process whose cmd == `target_cmd`.
///
/// ```
/// use proc_tree::{DefaultTree, is_descendant, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "sshd".into() });
/// tree.insert_node(200, PidNode { ppid: 100, cmd: "bash".into() });
///
/// assert!(is_descendant(&tree, 200, "sshd"));
/// assert!(is_descendant(&tree, 200, "init"));
/// assert!(!is_descendant(&tree, 200, "nginx"));
/// assert!(!is_descendant(&tree, 1, "sshd")); // init is not a descendant of sshd
/// ```
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
) -> Vec<ProcessLink> {
    let mut parts = Vec::new();
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            break;
        }
        let (ppid, cmd, user) = if let Some(node) = tree.get_node(current) {
            let user = cache
                .get_info(current)
                .map(|info| info.user)
                .unwrap_or_else(|| "unknown".to_string());
            (node.ppid, node.cmd, user)
        } else if let Some((node, info)) = crate::proc::parse_proc_entry(current) {
            (node.ppid, node.cmd, info.user)
        } else {
            parts.push(ProcessLink {
                pid: current,
                cmd: "unknown".to_string(),
                user: "unknown".to_string(),
            });
            break;
        };
        parts.push(ProcessLink {
            pid: current,
            cmd,
            user,
        });
        if ppid == 0 || current == ppid {
            break;
        }
        current = ppid;
    }
    parts
}

/// Build a chain string from the process tree.
///
/// Format: `"102|touch|root;101|sh|root;100|openclaw|root;1|systemd|root"`
///
/// ```
/// use proc_tree::{DefaultTree, DefaultCache, build_chain_string, TreeStore, CacheStore, PidNode, ProcInfo};
///
/// let tree = DefaultTree::new(100, 0);
/// let cache = DefaultCache::new(100, 0);
///
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// cache.insert_info(1, ProcInfo { cmd: "init".into(), user: "root".into(), ppid: 0, tgid: 1, start_time_ns: 0 });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "sshd".into() });
/// cache.insert_info(100, ProcInfo { cmd: "sshd".into(), user: "root".into(), ppid: 1, tgid: 100, start_time_ns: 0 });
/// tree.insert_node(200, PidNode { ppid: 100, cmd: "bash".into() });
/// cache.insert_info(200, ProcInfo { cmd: "bash".into(), user: "root".into(), ppid: 100, tgid: 200, start_time_ns: 0 });
///
/// let chain = build_chain_string(&tree, &cache, 200);
/// assert_eq!(chain, "200|bash|root;100|sshd|root;1|init|root");
/// ```
pub fn build_chain_string(tree: &impl TreeStore, cache: &impl CacheStore, pid: u32) -> String {
    build_chain_links(tree, cache, pid)
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Get direct children of a PID.
///
/// ```
/// use proc_tree::{DefaultTree, children, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "a".into() });
/// tree.insert_node(200, PidNode { ppid: 1, cmd: "b".into() });
/// tree.insert_node(300, PidNode { ppid: 100, cmd: "c".into() });
///
/// let mut kids = children(&tree, 1);
/// kids.sort();
/// assert_eq!(kids, vec![100, 200]);
/// assert_eq!(children(&tree, 100), vec![300]);
/// assert!(children(&tree, 999).is_empty());
/// ```
pub fn children(tree: &impl TreeStore, pid: u32) -> Vec<u32> {
    tree.all_pids()
        .into_iter()
        .filter(|&p| tree.get_node(p).map(|n| n.ppid == pid).unwrap_or(false))
        .collect()
}

/// Get all descendants of a PID (BFS traversal).
///
/// ```
/// use proc_tree::{DefaultTree, descendants, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "a".into() });
/// tree.insert_node(200, PidNode { ppid: 100, cmd: "b".into() });
/// tree.insert_node(300, PidNode { ppid: 200, cmd: "c".into() });
///
/// let mut desc = descendants(&tree, 1);
/// desc.sort();
/// assert_eq!(desc, vec![100, 200, 300]);
/// assert_eq!(descendants(&tree, 300), Vec::<u32>::new());
/// ```
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
///
/// ```
/// use proc_tree::{DefaultTree, siblings, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "a".into() });
/// tree.insert_node(200, PidNode { ppid: 1, cmd: "b".into() });
/// tree.insert_node(300, PidNode { ppid: 1, cmd: "c".into() });
///
/// let mut sibs = siblings(&tree, 100);
/// sibs.sort();
/// assert_eq!(sibs, vec![200, 300]);
/// assert!(siblings(&tree, 1).is_empty()); // init has no siblings
/// ```
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
///
/// ```
/// use proc_tree::{DefaultTree, find_by_cmd, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "sshd".into() });
/// tree.insert_node(200, PidNode { ppid: 1, cmd: "sshd".into() });
/// tree.insert_node(300, PidNode { ppid: 1, cmd: "bash".into() });
///
/// let mut sshds = find_by_cmd(&tree, "sshd");
/// sshds.sort();
/// assert_eq!(sshds, vec![100, 200]);
/// assert_eq!(find_by_cmd(&tree, "nginx"), Vec::<u32>::new());
/// ```
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
///
/// ```
/// use proc_tree::{DefaultTree, DefaultCache, find_by_user, TreeStore, CacheStore, PidNode, ProcInfo};
///
/// let tree = DefaultTree::new(100, 0);
/// let cache = DefaultCache::new(100, 0);
///
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// cache.insert_info(1, ProcInfo { cmd: "init".into(), user: "root".into(), ppid: 0, tgid: 1, start_time_ns: 0 });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "bash".into() });
/// cache.insert_info(100, ProcInfo { cmd: "bash".into(), user: "alice".into(), ppid: 1, tgid: 100, start_time_ns: 0 });
///
/// assert_eq!(find_by_user(&tree, &cache, "root"), vec![1]);
/// assert_eq!(find_by_user(&tree, &cache, "alice"), vec![100]);
/// assert_eq!(find_by_user(&tree, &cache, "nobody"), Vec::<u32>::new());
/// ```
pub fn find_by_user(tree: &impl TreeStore, cache: &impl CacheStore, target_user: &str) -> Vec<u32> {
    tree.all_pids()
        .into_iter()
        .filter(|&pid| {
            let user = cache
                .get_info(pid)
                .map(|info| info.user)
                .or_else(|| crate::proc::parse_proc_entry(pid).map(|(_, info)| info.user));
            user.as_deref() == Some(target_user)
        })
        .collect()
}

/// Render a pstree-style display starting from the given root PID.
///
/// ```
/// use proc_tree::{DefaultTree, display, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(100, PidNode { ppid: 1, cmd: "sshd".into() });
/// tree.insert_node(200, PidNode { ppid: 1, cmd: "cron".into() });
///
/// let output = display(&tree, 1);
/// assert!(output.starts_with("init"));
/// assert!(output.contains("sshd"));
/// assert!(output.contains("cron"));
/// ```
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
    // Root node: first child attaches with "─", rest with tree prefixes
    let mut output = cmd;
    for (i, &kid) in kids.iter().enumerate() {
        let is_last = i == kids.len() - 1;
        let prefix = if is_last { "└─" } else { "├─" };
        let continuation = if is_last { "  " } else { "│ " };
        let sub = display_subtree(tree, kid);
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

/// Recursive helper for non-root subtrees.
fn display_subtree(tree: &impl TreeStore, pid: u32) -> String {
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
        let sub = display_subtree(tree, kid);
        let lines: Vec<&str> = sub.lines().collect();
        output.push('\n');
        output.push_str(prefix);
        output.push_str(lines[0]);
        for line in &lines[1..] {
            output.push('\n');
            output.push_str(continuation);
            output.push_str(line);
        }
    }
    output
}

/// Get the number of entries in the tree.
///
/// ```
/// use proc_tree::{DefaultTree, tree_len, TreeStore, PidNode};
///
/// let tree = DefaultTree::new(100, 0);
/// assert_eq!(tree_len(&tree), 0);
///
/// tree.insert_node(1, PidNode { ppid: 0, cmd: "init".into() });
/// tree.insert_node(2, PidNode { ppid: 1, cmd: "bash".into() });
/// assert_eq!(tree_len(&tree), 2);
/// ```
pub fn tree_len(tree: &impl TreeStore) -> u64 {
    tree.all_pids().len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_store::DefaultTree;

    #[test]
    fn display_single_node() {
        let tree = DefaultTree::new(100, 0);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        assert_eq!(display(&tree, 1), "init");
    }

    #[test]
    fn display_root_with_children() {
        let tree = DefaultTree::new(100, 0);
        tree.insert_node(
            1,
            PidNode {
                ppid: 0,
                cmd: "init".into(),
            },
        );
        tree.insert_node(
            100,
            PidNode {
                ppid: 1,
                cmd: "a".into(),
            },
        );
        tree.insert_node(
            200,
            PidNode {
                ppid: 1,
                cmd: "b".into(),
            },
        );
        let d = display(&tree, 1);
        assert!(d.starts_with("init"));
        assert!(d.contains("a"));
        assert!(d.contains("b"));
    }
}
