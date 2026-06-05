//! # proc-tree
//!
//! Linux process tree: snapshot, incremental maintenance via fork/exec events,
//! ancestry chain queries, and PID reuse detection.
//!
//! ## Quick Start
//!
//! ```rust
//! use proc_tree::{TreeStore, CacheStore, PidNode, ProcInfo, ProcEvent};
//! use proc_tree::{snapshot, resolve, handle_events, build_chain_string};
//!
//! // Implement your own storage (or use a provided example)
//! # struct MyTree;
//! # impl TreeStore for MyTree {
//! #     fn get_node(&self, pid: u32) -> Option<PidNode> { None }
//! #     fn insert_node(&self, pid: u32, node: PidNode) {}
//! #     fn all_pids(&self) -> Vec<u32> { vec![] }
//! # }
//! # struct MyCache;
//! # impl CacheStore for MyCache {
//! #     fn get_info(&self, pid: u32) -> Option<ProcInfo> { None }
//! #     fn insert_info(&self, pid: u32, info: ProcInfo) {}
//! }
//!
//! let tree = MyTree;
//! let cache = MyCache;
//!
//! // Seed from /proc
//! snapshot(&tree, &cache);
//!
//! // Resolve a PID
//! if let Some(info) = resolve(&cache, 1) {
//!     println!("PID 1: cmd={}, user={}", info.cmd, info.user);
//! }
//!
//! // Build ancestry chain
//! let s = build_chain_string(&tree, &cache, 1234);
//! println!("Chain: {}", s);
//!
//! // Handle events
//! handle_events(&tree, &cache, &[
//!     ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
//! ]);
//! ```
//!
//! ## PID Reuse Detection
//!
//! When a process exits and its PID is reused by a new process, cached data
//! becomes stale. `CacheStore` implementations should compare `start_time_ns`
//! with the current `/proc` value to detect reuse.

mod default_store;
mod ops;
pub mod proc;
mod traits;
mod tree;
mod types;

// Public API — types
pub use types::{PidNode, ProcInfo};

// Public API — traits
pub use traits::{CacheStore, TreeStore};

// Public API — default implementations
pub use default_store::{DefaultCache, DefaultStore, DefaultTree};

// Public API — tree types
pub use tree::{ProcEvent, ProcessLink};

// Public API — operations
pub use ops::{
    build_chain_links, build_chain_string, children, descendants, display, find_by_cmd,
    find_by_user, handle_event, handle_events, is_descendant, resolve, siblings, snapshot,
    tree_len,
};

// Public API — proc utilities
pub use proc::{parse_proc_entry, read_proc_start_time_ns};
