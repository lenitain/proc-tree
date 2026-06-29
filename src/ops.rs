//! Process tree operations: snapshot, resolve, queries, display.
//!
//! All functions are generic over [`ProcessStore`] so they work with any storage backend.

use crate::traits::ProcessStore;
use crate::tree::{ExitedProcess, ProcEvent, ProcessLink};
use crate::types::ProcessInfo;

const UNKNOWN: &str = "unknown";

/// Snapshot all running processes from `/proc`.
///
/// Populates the store. Call once at startup before processing events.
///
/// # Errors
///
/// Returns an error if `/proc` cannot be read.
///
/// # Example
///
/// ```no_run
/// use proc_tree::{DefaultStore, snapshot, ProcessStore};
///
/// let store = DefaultStore::new(600);
/// snapshot(&store).expect("failed to read /proc");
///
/// // PID 1 should always exist on Linux
/// assert!(store.get_process(1).is_some());
/// ```
pub fn snapshot(store: &impl ProcessStore) -> Result<(), std::io::Error> {
    let dir = std::fs::read_dir("/proc")?;
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
    Ok(())
}

/// Resolve a PID to its process info.
///
/// Checks the store first, then falls back to reading `/proc` directly.
///
/// ```no_run
/// use proc_tree::{DefaultStore, snapshot, resolve, ProcessStore};
///
/// let store = DefaultStore::new(600);
/// snapshot(&store).expect("failed to read /proc");
///
/// let info = resolve(&store, 1).unwrap();
/// assert!(!info.cmd().is_empty());
/// ```
pub fn resolve(store: &impl ProcessStore, pid: u32) -> Option<ProcessInfo> {
    let info = resolve_process_info(store, pid);
    // Populate store for future lookups if we got info from /proc
    if let Some(ref info) = info
        && store.get_process(pid).is_none()
    {
        store.insert_process(pid, info.clone());
    }
    info
}

/// Internal helper to resolve process info with fallback chain.
///
/// Tries store first, then falls back to reading `/proc` directly.
/// Returns `None` if the process doesn't exist.
fn resolve_process_info(store: &impl ProcessStore, pid: u32) -> Option<ProcessInfo> {
    store
        .get_process(pid)
        .or_else(|| crate::proc::parse_proc_entry(pid))
}

/// Handle a batch of process lifecycle events.
///
/// Returns [`ExitedProcess`] handles for Exit events. The process info **stays in the store**
/// — callers decide when to remove it via [`ExitedProcess::remove`].
///
/// This is critical for event-driven systems where file events (fanotify)
/// may arrive after the proc connector exit event but still need to look
/// up process info.
///
/// # Example
///
/// ```
/// use proc_tree::{DefaultStore, handle_events, ProcEvent, ProcessStore};
///
/// let store = DefaultStore::new(0);
///
/// // Fork creates a process
/// let exited = handle_events(&store, &[
///     ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
/// ]);
/// assert!(exited.is_empty());
///
/// // Exit returns ExitedProcess handle — process info stays in store
/// let exited = handle_events(&store, &[
///     ProcEvent::Exit { pid: 200 },
/// ]);
/// assert_eq!(exited[0].pid, 200);
/// assert!(store.get_process(200).is_some());
///
/// // Caller explicitly removes when done
/// for ep in exited {
///     ep.remove(&store);
/// }
/// assert!(store.get_process(200).is_none());
/// ```
#[must_use = "call .remove(&store) on each ExitedProcess after processing related events"]
pub fn handle_events(store: &impl ProcessStore, events: &[ProcEvent]) -> Vec<ExitedProcess> {
    let mut exited = Vec::new();
    for event in events {
        if let Some(ep) = handle_event(store, event) {
            exited.push(ep);
        }
    }
    exited
}

/// Handle a single process lifecycle event.
///
/// Returns [`ExitedProcess`] for Exit events, `None` for other events.
/// The process info **stays in the store** — callers decide when to remove it
/// via [`ExitedProcess::remove`].
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
/// // Exit returns ExitedProcess handle — process info stays in store
/// let exited = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
/// assert_eq!(exited.pid, 100);
/// assert!(store.get_process(100).is_some());
///
/// // Caller explicitly removes when done
/// exited.remove(&store);
/// assert!(store.get_process(100).is_none());
/// ```
#[must_use = "call .remove(&store) after processing related events"]
pub fn handle_event(store: &impl ProcessStore, event: &ProcEvent) -> Option<ExitedProcess> {
    match event {
        ProcEvent::Fork {
            child_pid,
            parent_pid,
            ..
        } => {
            store.insert_process(
                *child_pid,
                ProcessInfo::new(
                    String::new(),
                    String::new(),
                    String::new(),
                    *parent_pid,
                    0,
                    0,
                ),
            );
        }
        ProcEvent::Exec { pid, .. } => {
            let info = crate::proc::parse_proc_entry(*pid).unwrap_or_else(|| {
                ProcessInfo::new(
                    UNKNOWN.to_string(),
                    UNKNOWN.to_string(),
                    UNKNOWN.to_string(),
                    0,
                    0,
                    0,
                )
            });
            // Keep start_time_ns from /proc, don't overwrite with event timestamp
            store.insert_process(*pid, info);
        }
        ProcEvent::Exit { pid } => {
            // Orphan children to init (PID 1)
            store.for_each_child(*pid, &mut |child_pid| {
                if let Some(mut info) = store.get_process(child_pid) {
                    info = ProcessInfo::new(
                        info.comm().to_string(),
                        info.cmd().to_string(),
                        info.user().to_string(),
                        1,
                        info.tgid(),
                        info.start_time_ns(),
                    );
                    store.insert_process(child_pid, info);
                }
            });
            return Some(ExitedProcess { pid: *pid });
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
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("sshd".into(), "sshd".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("bash".into(), "bash".into(), "root".into(), 100, 200, 0));
///
/// assert!(is_descendant(&store, 200, "sshd"));
/// assert!(is_descendant(&store, 200, "init"));
/// assert!(!is_descendant(&store, 200, "nginx"));
/// assert!(!is_descendant(&store, 1, "sshd")); // init is not a descendant of sshd
/// ```
pub fn is_descendant(store: &impl ProcessStore, pid: u32, target_cmd: &str) -> bool {
    walk_ancestors(store, pid, |info| info.comm() == target_cmd)
}

/// Walk up the process tree from `pid`, calling `predicate` on each ancestor.
///
/// Returns `true` if the predicate matches any ancestor, `false` otherwise.
/// Handles cycles by tracking visited PIDs.
fn walk_ancestors<P>(store: &impl ProcessStore, pid: u32, predicate: P) -> bool
where
    P: Fn(&ProcessInfo) -> bool,
{
    let mut current = pid;
    let mut visited = std::collections::HashSet::new();
    while let Some(info) = store.get_process(current) {
        if !visited.insert(current) {
            break;
        }
        if predicate(&info) {
            return true;
        }
        if info.ppid() == 0 || current == info.ppid() {
            break;
        }
        current = info.ppid();
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
        match resolve_process_info(store, current) {
            Some(info) => {
                parts.push(ProcessLink::new(
                    current,
                    info.comm().to_string(),
                    info.cmd().to_string(),
                    info.user().to_string(),
                ));
                if info.ppid() == 0 || current == info.ppid() {
                    break;
                }
                current = info.ppid();
            }
            None => {
                parts.push(ProcessLink::new(
                    current,
                    UNKNOWN.to_string(),
                    UNKNOWN.to_string(),
                    UNKNOWN.to_string(),
                ));
                break;
            }
        }
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
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("sshd".into(), "sshd".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("bash".into(), "bash".into(), "root".into(), 100, 200, 0));
///
/// let chain = build_chain_string(&store, 200);
/// // Returns JSON array: [{"pid":200,"comm":"bash","cmd":"bash","user":"root"}, ...]
/// assert!(chain.contains("\"comm\":\"bash\""));
/// ```
pub fn build_chain_string(store: &impl ProcessStore, pid: u32) -> String {
    let links = build_chain_links(store, pid);
    if links.is_empty() {
        return String::new();
    }
    serde_json::to_string(&links).unwrap_or_default()
}

/// Get direct children of a PID.
///
/// ```
/// use proc_tree::{DefaultStore, children, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("b".into(), "b".into(), "root".into(), 1, 200, 0));
/// store.insert_process(300, ProcessInfo::new("c".into(), "c".into(), "root".into(), 100, 300, 0));
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
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("b".into(), "b".into(), "root".into(), 100, 200, 0));
/// store.insert_process(300, ProcessInfo::new("c".into(), "c".into(), "root".into(), 200, 300, 0));
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
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("b".into(), "b".into(), "root".into(), 1, 200, 0));
/// store.insert_process(300, ProcessInfo::new("c".into(), "c".into(), "root".into(), 1, 300, 0));
///
/// let mut sibs = siblings(&store, 100);
/// sibs.sort();
/// assert_eq!(sibs, vec![200, 300]);
/// assert!(siblings(&store, 1).is_empty()); // init has no siblings
/// ```
pub fn siblings(store: &impl ProcessStore, pid: u32) -> Vec<u32> {
    let ppid = match store.get_process(pid) {
        Some(info) => info.ppid(),
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
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("sshd".into(), "sshd".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("sshd".into(), "sshd".into(), "root".into(), 1, 200, 0));
/// store.insert_process(300, ProcessInfo::new("bash".into(), "bash".into(), "root".into(), 1, 300, 0));
///
/// let mut sshds = find_by_cmd(&store, "sshd");
/// sshds.sort();
/// assert_eq!(sshds, vec![100, 200]);
/// assert_eq!(find_by_cmd(&store, "nginx"), Vec::<u32>::new());
/// ```
pub fn find_by_cmd(store: &impl ProcessStore, target_cmd: &str) -> Vec<u32> {
    find_by(
        store,
        |pid| {
            store
                .get_process(pid)
                .map(|info| info.comm().to_string())
                .filter(|c| !c.is_empty())
                .or_else(|| crate::proc::read_proc_comm(pid))
        },
        target_cmd,
    )
}

/// Find all PIDs whose user matches the given string.
///
/// ```
/// use proc_tree::{DefaultStore, find_by_user, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
///
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("bash".into(), "bash".into(), "alice".into(), 1, 100, 0));
///
/// assert_eq!(find_by_user(&store, "root"), vec![1]);
/// assert_eq!(find_by_user(&store, "alice"), vec![100]);
/// assert_eq!(find_by_user(&store, "nobody"), Vec::<u32>::new());
/// ```
pub fn find_by_user(store: &impl ProcessStore, target_user: &str) -> Vec<u32> {
    find_by(
        store,
        |pid| {
            store
                .get_process(pid)
                .map(|info| info.user().to_string())
                .or_else(|| crate::proc::parse_proc_entry(pid).map(|info| info.user().to_string()))
        },
        target_user,
    )
}

/// Generic find function that filters PIDs by a value extractor.
///
/// This is a helper to reduce code duplication between `find_by_cmd` and `find_by_user`.
fn find_by<F>(store: &impl ProcessStore, extract: F, target: &str) -> Vec<u32>
where
    F: Fn(u32) -> Option<String>,
{
    store
        .all_pids()
        .into_iter()
        .filter(|&pid| extract(pid).as_deref() == Some(target))
        .collect()
}

/// Render a pstree-style display starting from the given root PID.
///
/// ```
/// use proc_tree::{DefaultStore, display, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(100, ProcessInfo::new("sshd".into(), "sshd".into(), "root".into(), 1, 100, 0));
/// store.insert_process(200, ProcessInfo::new("cron".into(), "cron".into(), "root".into(), 1, 200, 0));
///
/// let output = display(&store, 1);
/// assert!(output.starts_with("init"));
/// assert!(output.contains("sshd"));
/// assert!(output.contains("cron"));
/// ```
pub fn display(store: &impl ProcessStore, root_pid: u32) -> String {
    render_tree(store, root_pid, true)
}

/// Recursive tree renderer.
///
/// `is_root` controls the rendering style:
/// - Root node: first child attaches with "─"
/// - Non-root nodes: all children use tree prefixes
fn render_tree(store: &impl ProcessStore, pid: u32, is_root: bool) -> String {
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
        let sub = render_tree(store, kid, false);
        let mut lines = sub.lines();
        if let Some(first) = lines.next() {
            if is_root && i == 0 {
                output.push('─');
                output.push_str(first);
            } else {
                output.push('\n');
                output.push_str(prefix);
                output.push_str(first);
            }
            for line in lines {
                output.push('\n');
                output.push_str(continuation);
                output.push_str(line);
            }
        }
    }
    output
}

/// Get command name for a PID, with fallback chain: store -> /proc -> "unknown"
fn get_cmd(store: &impl ProcessStore, pid: u32) -> String {
    resolve_process_info(store, pid)
        .map(|info| info.cmd().to_string())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| UNKNOWN.to_string())
}

/// Get the number of entries in the store.
///
/// ```
/// use proc_tree::{DefaultStore, tree_len, ProcessStore, ProcessInfo};
///
/// let store = DefaultStore::new(0);
/// assert_eq!(tree_len(&store), 0);
///
/// store.insert_process(1, ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0));
/// store.insert_process(2, ProcessInfo::new("bash".into(), "bash".into(), "root".into(), 1, 2, 0));
/// assert_eq!(tree_len(&store), 2);
/// ```
pub fn tree_len(store: &impl ProcessStore) -> usize {
    store.all_pids().len()
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
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        assert_eq!(display(&store, 1), "init");
    }

    #[test]
    fn display_root_with_children() {
        let store = DefaultStore::new(0);
        store.insert_process(
            1,
            ProcessInfo::new("init".into(), "init".into(), "root".into(), 0, 1, 0),
        );
        store.insert_process(
            100,
            ProcessInfo::new("a".into(), "a".into(), "root".into(), 1, 100, 0),
        );
        store.insert_process(
            200,
            ProcessInfo::new("b".into(), "b".into(), "root".into(), 1, 200, 0),
        );
        let d = display(&store, 1);
        assert!(d.starts_with("init"));
        assert!(d.contains("a"));
        assert!(d.contains("b"));
    }
}
