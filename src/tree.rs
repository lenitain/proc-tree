//! Process tree types: events, links, and node definitions.

use std::fmt;

use crate::traits::ProcessStore;

// ---- Process events (decoupled from proc-connector) ----

/// A process lifecycle event. Decoupled from any specific event source
/// (proc-connector, audit, etc.) so users can adapt their own events.
#[derive(Debug, Clone)]
pub enum ProcEvent {
    /// A new process was created. `parent_pid` is the parent.
    Fork {
        child_pid: u32,
        parent_pid: u32,
        timestamp_ns: u64,
    },
    /// A process executed a new program. Its cmd/user may have changed.
    Exec { pid: u32, timestamp_ns: u64 },
    /// A process exited. The node is preserved for historical chain lookups.
    Exit { pid: u32 },
}

// ---- ExitedProcess (explicit removal handle) ----

/// An exited process awaiting explicit removal from the store.
///
/// Returned by [`handle_event`](crate::handle_event) and [`handle_events`](crate::handle_events)
/// for Exit events. The process info **stays in the store** until
/// [`remove`](ExitedProcess::remove) is called, allowing late-arriving events
/// to still look up process info.
///
/// # Example
///
/// ```
/// use proc_tree::{DefaultStore, handle_event, ProcEvent, ExitedProcess, ProcessStore};
///
/// let store = DefaultStore::new(0);
/// handle_event(&store, &ProcEvent::Fork { child_pid: 100, parent_pid: 1, timestamp_ns: 0 });
///
/// let exited = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
/// assert_eq!(exited.pid, 100);
///
/// // Process still in store — caller can still query it
/// assert!(store.get_process(100).is_some());
///
/// // Explicitly remove when done
/// exited.remove(&store);
/// assert!(store.get_process(100).is_none());
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "call .remove(&store) after processing related events, or the process stays in store until TTL expires"]
pub struct ExitedProcess {
    /// The PID of the exited process.
    pub pid: u32,
}

impl ExitedProcess {
    /// Get the PID of the exited process.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Remove this process from the store.
    ///
    /// Call this after all related events have been processed.
    /// The process info is no longer accessible after this call.
    pub fn remove<S: ProcessStore>(self, store: &S) {
        store.remove_process(self.pid);
    }
}

// ---- ProcessLink (structured chain element) ----

/// A single entry in a process ancestry chain.
///
/// Displayed as `"pid|cmd|user"` by the `Display` impl.
///
/// ```
/// use proc_tree::ProcessLink;
///
/// let link = ProcessLink { pid: 102, cmd: "touch".into(), user: "root".into() };
/// assert_eq!(link.to_string(), "102|touch|root");
/// ```
///
/// A chain is a `Vec<ProcessLink>` ordered from child to ancestor.
#[derive(Debug, Clone)]
pub struct ProcessLink {
    pub pid: u32,
    pub cmd: String,
    pub user: String,
}

impl fmt::Display for ProcessLink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}|{}|{}", self.pid, self.cmd, self.user)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_link_display_format() {
        let link = ProcessLink {
            pid: 42,
            cmd: "bash".into(),
            user: "root".into(),
        };
        assert_eq!(link.to_string(), "42|bash|root");
    }

    #[test]
    fn process_link_clone() {
        let link = ProcessLink {
            pid: 1,
            cmd: "init".into(),
            user: "root".into(),
        };
        let link2 = link.clone();
        assert_eq!(link.pid, link2.pid);
        assert_eq!(link.cmd, link2.cmd);
        assert_eq!(link.user, link2.user);
    }

    #[test]
    fn proc_event_clone() {
        let e = ProcEvent::Fork {
            child_pid: 100,
            parent_pid: 1,
            timestamp_ns: 42,
        };
        let e2 = e.clone();
        match e2 {
            ProcEvent::Fork {
                child_pid,
                parent_pid,
                timestamp_ns,
            } => {
                assert_eq!(child_pid, 100);
                assert_eq!(parent_pid, 1);
                assert_eq!(timestamp_ns, 42);
            }
            _ => panic!("expected Fork"),
        }
    }
}
