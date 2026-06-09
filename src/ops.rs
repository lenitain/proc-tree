//! Process tree operations: snapshot, resolve, queries, display.
//!
//! All functions are generic over [`ProcessStore`] so they work with any storage backend.

use crate::guard::ExitedProcessGuard;
use crate::traits::ProcessStore;
use crate::tree::{ProcEvent, ProcessLink};
use crate::types::ProcessInfo;

/// Snapshot all running processes from `/proc`.
///
/// Populates the store. Call once at startup before processing events.
///
/// ```no_run
/// use proc_tree::{DefaultStore, snapshot, ProcessStore};
///
/// let store = DefaultStore::new(600);
/// snapshot(&store);
///
/// // PID 1 should always exist on Linux
/// assert!(store.get_process(1).is_some());
/// ```
pub fn snapshot(store: &impl ProcessStore) {
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
        if let Some(info) = crate::proc::parse_proc_entry(pid) {
            store.insert_process(pid, info);
        }
    }
}

/// Resolve a PID to its process info.
///
/// Checks the store first, then falls back to reading `/proc` directly.
///
/// ```no_run
/// use proc_tree::{DefaultStore, snapshot, resolve, ProcessStore};
///
/// let store = DefaultStore::new(600);
/// snapshot(&store);
///
/// let info = resolve(&store, 1).unwrap();
/// assert!(!info.cmd.is_empty());
/// ```
pub fn resolve(store: &impl ProcessStore, pid: u32) -> Option<ProcessInfo> {
    // Try store first
    if let Some(info) = store.get_process(pid) {
        return Some(info);
    }
    // Fallback: read /proc directly via parse_proc_entry
    let info = crate::proc::parse_proc_entry(pid)?;
    // Populate store for future lookups
    store.insert_process(pid, info.clone());
    Some(info)
}

/// Handle a batch of process lifecycle events.
///
/// Returns RAII guards for exited processes. Each guard automatically removes
/// the process from the store when dropped. Use `.remove()` for explicit removal.
///
/// # Why deferred removal?
///
/// When a process exits, its children are orphaned to init (PID 1), but the
/// process info is kept in the store. This allows callers to access process
/// info (cmd, user, chain) after the exit event but before removal. This is
/// important for event-driven systems where file events (fanotify) may arrive
/// after the proc connector exit event.
///
/// # Example
///
/// ```
/// use proc_tree::{DefaultStore, handle_events, ProcEvent, ProcessStore};
///
/// let store = DefaultStore::new(0);
///
/// // Fork creates a process
/// let guards = handle_events(&store, &[
///     ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
/// ]);
/// assert!(guards.is_empty());
///
/// // Exit returns guards
/// let guards = handle_events(&store, &[
///     ProcEvent::Exit { pid: 200 },
/// ]);
/// assert_eq!(guards.len(), 1);
///
/// // Process still accessible until guard is dropped
/// assert!(store.get_process(200).is_some());
///
/// // Explicit removal (optional)
/// for guard in guards {
///     guard.remove();
/// }
/// assert!(store.get_process(200).is_none());
/// ```
#[must_use = "returned guards must be stored or they will remove processes immediately"]
pub fn handle_events<S: ProcessStore + Clone>(store: &S, events: &[ProcEvent]) -> Vec<ExitedProcessGuard<S>> {
    let mut guards = Vec::new();
    for event in events {
        if let Some(guard) = handle_event(store, event) {
            guards.push(guard);
        }
    }
    guards
}

/// Handle a single process lifecycle event.
///
/// Returns `Some(ExitedProcessGuard)` for Exit events, `None` for other events.
/// The guard automatically removes the process when dropped.
///
/// # Example
///
/// ```
/// use proc_tree::{DefaultStore, handle_event, ProcEvent, ProcessStore};
///
/// let store = DefaultStore::new(0);
///
/// // Create a process
/// handle_event(&store, &ProcEvent::Fork {
///     child_pid: 100,
///     parent_pid: 1,
///     timestamp_ns: 0,
/// });
///
/// // Exit returns a guard
/// let guard = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
///
/// // Process still accessible
/// assert!(store.get_process(100).is_some());
///
/// // Guard removes process when dropped
/// drop(guard);
/// assert!(store.get_process(100).is_none());
/// ```
#[must_use = "returned guard must be stored or it will remove the process immediately"]
pub fn handle_event<S: ProcessStore + Clone>(store: &S, event: &ProcEvent) -> Option<ExitedProcessGuard<S>> {
    match event {
        ProcEvent::Fork {
            child_pid,
            parent_pid,
            ..
        } => {
            store.insert_process(
                *child_pid,
                ProcessInfo {
                    ppid: *parent_pid,
                    cmd: String::new(),
                    user: String::new(),
                    tgid: 0,
                    start_time_ns: 0,
                },
            );
        }
        ProcEvent::Exec { pid, timestamp_ns } => {
            let mut info = crate::proc::parse_proc_entry(*pid).unwrap_or_else(|| {
                let cmd = "unknown".to_string();
                ProcessInfo {
                    ppid: 0,
                    cmd,
                    user: "unknown".to_string(),
                    tgid: 0,
                    start_time_ns: 0,
                }
            });
            info.start_time_ns = *timestamp_ns;
            store.insert_process(*pid, info);
        }
        ProcEvent::Exit { pid } => {
            // Orphan children to init (PID 1)
            let children = store.children_of(*pid);
            for child_pid in children {
                if let Some(mut info) = store.get_process(child_pid) {
                    info.ppid = 1;
                    store.insert_process(child_pid, info);
                }
            }
            // Return guard for automatic cleanup
            return Some(ExitedProcessGuard::new(store.clone(), *pid));
        }
    }
    None
}

/// Check if `pid` is a descendant of any process whose cmd == `target_cmd`.
///
/// ```
/// use proc_tree::{DefaultStore, is_descendant, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "sshd".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 100, cmd: "bash".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
///
/// assert!(is_descendant(&store, 200, "sshd"));
/// assert!(is_descendant(&store, 200, "init"));
/// assert!(!is_descendant(&store, 200, "nginx"));
/// assert!(!is_descendant(&store, 1, "sshd")); // init is not a descendant of sshd
/// ```
pub fn is_descendant(store: &impl ProcessStore, pid: u32, target_cmd: &str) -> bool {
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    while let Some(info) = store.get_process(current) {
        if !visited.insert(current) {
            break;
        }
        if info.cmd == target_cmd {
            return true;
        }
        if info.ppid == 0 || current == info.ppid {
            break;
        }
        current = info.ppid;
    }
    false
}

/// Build a chain of ProcessLink from the process tree.
pub fn build_chain_links(store: &impl ProcessStore, pid: u32) -> Vec<ProcessLink> {
    let mut parts = Vec::new();
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    loop {
        if !visited.insert(current) {
            break;
        }
        let (ppid, cmd, user) = if let Some(info) = store.get_process(current) {
            (info.ppid, info.cmd, info.user)
        } else if let Some(info) = crate::proc::parse_proc_entry(current) {
            (info.ppid, info.cmd, info.user)
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
/// use proc_tree::{DefaultStore, build_chain_string, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
///
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "sshd".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 100, cmd: "bash".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
///
/// let chain = build_chain_string(&store, 200);
/// assert_eq!(chain, "200|bash|root;100|sshd|root;1|init|root");
/// ```
pub fn build_chain_string(store: &impl ProcessStore, pid: u32) -> String {
    build_chain_links(store, pid)
        .iter()
        .map(|l| l.to_string())
        .collect::<Vec<_>>()
        .join(";")
}

/// Get direct children of a PID.
///
/// ```
/// use proc_tree::{DefaultStore, children, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "a".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 1, cmd: "b".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
/// store.insert_process(300, ProcessInfo { ppid: 100, cmd: "c".into(), user: "root".into(), tgid: 300, start_time_ns: 0 });
///
/// let mut kids = children(&store, 1);
/// kids.sort();
/// assert_eq!(kids, vec![100, 200]);
/// assert_eq!(children(&store, 100), vec![300]);
/// assert!(children(&store, 999).is_empty());
/// ```
pub fn children(store: &impl ProcessStore, pid: u32) -> Vec<u32> {
    store.children_of(pid)
}

/// Get all descendants of a PID (BFS traversal).
///
/// ```
/// use proc_tree::{DefaultStore, descendants, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "a".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 100, cmd: "b".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
/// store.insert_process(300, ProcessInfo { ppid: 200, cmd: "c".into(), user: "root".into(), tgid: 300, start_time_ns: 0 });
///
/// let mut desc = descendants(&store, 1);
/// desc.sort();
/// assert_eq!(desc, vec![100, 200, 300]);
/// assert_eq!(descendants(&store, 300), Vec::<u32>::new());
/// ```
pub fn descendants(store: &impl ProcessStore, pid: u32) -> Vec<u32> {
    let mut result = Vec::new();
    let mut queue = std::collections::VecDeque::new();
    queue.push_back(pid);
    while let Some(current) = queue.pop_front() {
        let kids = children(store, current);
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
/// use proc_tree::{DefaultStore, siblings, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "a".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 1, cmd: "b".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
/// store.insert_process(300, ProcessInfo { ppid: 1, cmd: "c".into(), user: "root".into(), tgid: 300, start_time_ns: 0 });
///
/// let mut sibs = siblings(&store, 100);
/// sibs.sort();
/// assert_eq!(sibs, vec![200, 300]);
/// assert!(siblings(&store, 1).is_empty()); // init has no siblings
/// ```
pub fn siblings(store: &impl ProcessStore, pid: u32) -> Vec<u32> {
    let ppid = match store.get_process(pid) {
        Some(info) => info.ppid,
        None => return Vec::new(),
    };
    children(store, ppid)
        .into_iter()
        .filter(|&c| c != pid)
        .collect()
}

/// Find all PIDs whose cmd matches the given string.
///
/// ```
/// use proc_tree::{DefaultStore, find_by_cmd, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "sshd".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 1, cmd: "sshd".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
/// store.insert_process(300, ProcessInfo { ppid: 1, cmd: "bash".into(), user: "root".into(), tgid: 300, start_time_ns: 0 });
///
/// let mut sshds = find_by_cmd(&store, "sshd");
/// sshds.sort();
/// assert_eq!(sshds, vec![100, 200]);
/// assert_eq!(find_by_cmd(&store, "nginx"), Vec::<u32>::new());
/// ```
pub fn find_by_cmd(store: &impl ProcessStore, target_cmd: &str) -> Vec<u32> {
    store
        .all_pids()
        .into_iter()
        .filter(|&pid| {
            let cmd = store
                .get_process(pid)
                .map(|info| info.cmd)
                .filter(|c| !c.is_empty())
                .or_else(|| crate::proc::read_proc_comm(pid));
            cmd.as_deref() == Some(target_cmd)
        })
        .collect()
}

/// Find all PIDs whose user matches the given string.
///
/// ```
/// use proc_tree::{DefaultStore, find_by_user, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
///
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "bash".into(), user: "alice".into(), tgid: 100, start_time_ns: 0 });
///
/// assert_eq!(find_by_user(&store, "root"), vec![1]);
/// assert_eq!(find_by_user(&store, "alice"), vec![100]);
/// assert_eq!(find_by_user(&store, "nobody"), Vec::<u32>::new());
/// ```
pub fn find_by_user(store: &impl ProcessStore, target_user: &str) -> Vec<u32> {
    store
        .all_pids()
        .into_iter()
        .filter(|&pid| {
            let user = store
                .get_process(pid)
                .map(|info| info.user)
                .or_else(|| crate::proc::parse_proc_entry(pid).map(|info| info.user));
            user.as_deref() == Some(target_user)
        })
        .collect()
}

/// Render a pstree-style display starting from the given root PID.
///
/// ```
/// use proc_tree::{DefaultStore, display, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(100, ProcessInfo { ppid: 1, cmd: "sshd".into(), user: "root".into(), tgid: 100, start_time_ns: 0 });
/// store.insert_process(200, ProcessInfo { ppid: 1, cmd: "cron".into(), user: "root".into(), tgid: 200, start_time_ns: 0 });
///
/// let output = display(&store, 1);
/// assert!(output.starts_with("init"));
/// assert!(output.contains("sshd"));
/// assert!(output.contains("cron"));
/// ```
pub fn display(store: &impl ProcessStore, root_pid: u32) -> String {
    let cmd = get_cmd(store, root_pid);
    let kids = children(store, root_pid);
    if kids.is_empty() {
        return cmd;
    }
    // Root node: first child attaches with "─", rest with tree prefixes
    let mut output = cmd;
    for (i, &kid) in kids.iter().enumerate() {
        let is_last = i == kids.len() - 1;
        let prefix = if is_last { "└─" } else { "├─" };
        let continuation = if is_last { "  " } else { "│ " };
        let sub = display_subtree(store, kid);
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
fn display_subtree(store: &impl ProcessStore, pid: u32) -> String {
    let cmd = get_cmd(store, pid);
    let kids = children(store, pid);
    if kids.is_empty() {
        return cmd;
    }
    let mut output = cmd;
    for (i, &kid) in kids.iter().enumerate() {
        let is_last = i == kids.len() - 1;
        let prefix = if is_last { "└─" } else { "├─" };
        let continuation = if is_last { "  " } else { "│ " };
        let sub = display_subtree(store, kid);
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

/// Get command name for a PID, with fallback chain: store -> /proc -> "unknown"
fn get_cmd(store: &impl ProcessStore, pid: u32) -> String {
    store
        .get_process(pid)
        .map(|info| info.cmd)
        .filter(|c| !c.is_empty())
        .or_else(|| crate::proc::read_proc_comm(pid))
        .unwrap_or_else(|| "unknown".to_string())
}

/// Get the number of entries in the store.
///
/// ```
/// use proc_tree::{DefaultStore, tree_len, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// assert_eq!(tree_len(&store), 0);
///
/// store.insert_process(1, ProcessInfo { ppid: 0, cmd: "init".into(), user: "root".into(), tgid: 1, start_time_ns: 0 });
/// store.insert_process(2, ProcessInfo { ppid: 1, cmd: "bash".into(), user: "root".into(), tgid: 2, start_time_ns: 0 });
/// assert_eq!(tree_len(&store), 2);
/// ```
pub fn tree_len(store: &impl ProcessStore) -> u64 {
    store.all_pids().len() as u64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::default_store::DefaultStore;

    #[test]
    fn display_single_node() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        assert_eq!(display(&store, 1), "init");
    }

    #[test]
    fn display_root_with_children() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo {
                ppid: 0,
                cmd: "init".into(),
                user: "root".into(),
                tgid: 1,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            100,
            ProcessInfo {
                ppid: 1,
                cmd: "a".into(),
                user: "root".into(),
                tgid: 100,
                start_time_ns: 0,
            },
        );
        store.insert_process(
            200,
            ProcessInfo {
                ppid: 1,
                cmd: "b".into(),
                user: "root".into(),
                tgid: 200,
                start_time_ns: 0,
            },
        );
        let d = display(&store, 1);
        assert!(d.starts_with("init"));
        assert!(d.contains("a"));
        assert!(d.contains("b"));
    }
}
