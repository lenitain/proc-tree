//! # proc-tree
//!
//! Linux process tree: snapshot, incremental maintenance via fork/exec events,
//! ancestry chain queries, and PID reuse detection.
//!
//! ## Quick Start
//!
//! ```rust
//! use proc_tree::{ProcessStore, ProcessInfo, ProcEvent};
//! use proc_tree::{snapshot, resolve, handle_events, build_chain_string};
//!
//! // Implement your own storage (or use a provided example)
//! # #[derive(Clone)]
//! # struct MyStore;
//! # impl ProcessStore for MyStore {
//! #     fn get_process(&self, pid: u32) -> Option<ProcessInfo> { None }
//! #     fn insert_process(&self, pid: u32, info: ProcessInfo) {}
//! #     fn remove_process(&self, pid: u32) -> Option<ProcessInfo> { None }
//! #     fn all_pids(&self) -> Vec<u32> { vec![] }
//! #     fn children_of(&self, pid: u32) -> Vec<u32> { vec![] }
//! # }
//!
//! let store = MyStore;
//!
//! // Seed from /proc
//! snapshot(&store);
//!
//! // Resolve a PID
//! if let Some(info) = resolve(&store, 1) {
//!     println!("PID 1: cmd={}, user={}", info.cmd, info.user);
//! }
//!
//! // Build ancestry chain
//! let s = build_chain_string(&store, 1234);
//! println!("Chain: {}", s);
//!
//! // Handle events (returns exited PIDs)
//! let exited = handle_events(&store, &[
//!     ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
//! ]);
//! // Caller decides when to remove exited processes
//! for pid in exited {
//!     store.remove_process(pid);
//! }
//! ```
//!
//! ## PID Reuse Detection
//!
//! When a process exits and its PID is reused by a new process, cached data
//! becomes stale. `ProcessStore` implementations should compare `start_time_ns`
//! with the current `/proc` value to detect reuse.

mod default_store;
mod ops;
mod proc;
mod traits;
mod tree;
mod types;

// Public API — types
pub use types::ProcessInfo;

// Public API — traits
pub use traits::ProcessStore;

// Public API — default implementations
pub use default_store::DefaultStore;

// Public API — tree types
pub use tree::{ProcEvent, ProcessLink};

// Public API — operations
pub use ops::{
    build_chain_links, build_chain_string, children, descendants, display, find_by_cmd,
    find_by_user, handle_event, handle_events, is_descendant, resolve, siblings, snapshot,
    tree_len,
};

// Public API — proc utilities
pub use proc::{parse_proc_entry, read_proc_comm, read_proc_start_time_ns, uid_to_username};
