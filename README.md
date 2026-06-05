# proc-tree

[![Crates.io](https://img.shields.io/crates/v/proc-tree.svg)](https://crates.io/crates/proc-tree)
[![Docs.rs](https://docs.rs/proc-tree/badge.svg)](https://docs.rs/proc-tree)

Linux **process tree** — snapshot, incremental maintenance via fork/exec/exit events, ancestry chain queries, and PID reuse detection.

## Installation

```bash
cargo add proc-tree
```

Minimum supported Rust version: **1.85** (edition 2024).

## Features

- **Snapshot**: one-shot `/proc` scan to seed the tree
- **Incremental updates**: fork/exec/exit events for live maintenance
- **Ancestry queries**: build process chain, check descendants, find siblings
- **PID reuse detection**: via `start_time_ns` comparison
- **Trait-based storage**: implement `TreeStore` and `CacheStore` for your own backend
- **Zero heap allocation** for `/proc` path formatting (`ArrayString`)
- **Thread-safe**: traits accept `&self` for interior mutability

## Testing

```bash
cargo test
```

65 tests (13 unit + 50 integration + 2 doc-tests) covering snapshot, chain building, descendant checks, display formatting, edge cases (cycles, nonexistent PIDs, concurrent access).

## Quick example

```rust
use proc_tree::{TreeStore, CacheStore, PidNode, ProcInfo, ProcEvent};
use proc_tree::{snapshot, resolve, handle_events, build_chain_string, display};
use std::collections::HashMap;
use std::sync::Mutex;

// Provide your own storage implementation
struct MyTree(Mutex<HashMap<u32, PidNode>>);
struct MyCache(Mutex<HashMap<u32, ProcInfo>>);

impl TreeStore for MyTree {
    fn get_node(&self, pid: u32) -> Option<PidNode> {
        self.0.lock().unwrap().get(&pid).cloned()
    }
    fn insert_node(&self, pid: u32, node: PidNode) {
        self.0.lock().unwrap().insert(pid, node);
    }
    fn all_pids(&self) -> Vec<u32> {
        self.0.lock().unwrap().keys().copied().collect()
    }
}

impl CacheStore for MyCache {
    fn get_info(&self, pid: u32) -> Option<ProcInfo> {
        self.0.lock().unwrap().get(&pid).cloned()
    }
    fn insert_info(&self, pid: u32, info: ProcInfo) {
        self.0.lock().unwrap().insert(pid, info);
    }
}

fn main() {
    let tree = MyTree(Mutex::new(HashMap::new()));
    let cache = MyCache(Mutex::new(HashMap::new()));

    // Seed from /proc
    snapshot(&tree, &cache);

    // Resolve a PID
    if let Some(info) = resolve(&cache, 1) {
        println!("PID 1: cmd={}, user={}", info.cmd, info.user);
    }

    // Build ancestry chain
    let chain = build_chain_string(&tree, &cache, std::process::id());
    println!("Chain: {}", chain);

    // Check descendant
    let info = resolve(&cache, 1).unwrap();
    assert!(proc_tree::is_descendant(&tree, std::process::id(), &info.cmd));

    // Handle events
    handle_events(&tree, &cache, &[
        ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
        ProcEvent::Exec { pid: 200, timestamp_ns: 1 },
    ]);

    // Display as pstree
    println!("{}", display(&tree, 1));
}
```

## API

### Traits

| Trait | Description |
|-------|-------------|
| `TreeStore` | Process tree storage (parent→child relationships) |
| `CacheStore` | Process info cache (PID→metadata mapping) |

### Functions

| Function | Description |
|----------|-------------|
| `snapshot(tree, cache)` | Scan `/proc` and populate tree + cache |
| `resolve(cache, pid)` | Get process info (with PID reuse detection) |
| `build_chain_links(tree, cache, pid)` | Build ancestry chain `Vec<ProcessLink>` |
| `build_chain_string(tree, cache, pid)` | Chain as `"pid\|cmd\|user;..."` string |
| `is_descendant(tree, pid, cmd)` | Check if PID descends from process with given cmd |
| `children(tree, pid)` | Get direct children PIDs |
| `descendants(tree, pid)` | Get all descendant PIDs (BFS) |
| `siblings(tree, pid)` | Get sibling PIDs (same parent) |
| `find_by_cmd(tree, cmd)` | Find PIDs by command name |
| `find_by_user(tree, cache, user)` | Find PIDs by username |
| `display(tree, root_pid)` | Render pstree-style display |
| `handle_events(tree, cache, events)` | Process batch of fork/exec/exit events |
| `tree_len(tree)` | Get number of entries in tree |

### Types

```rust
// Tree node
pub struct PidNode {
    pub ppid: u32,
    pub cmd: String,
}

// Cached process info
pub struct ProcInfo {
    pub cmd: String,
    pub user: String,
    pub ppid: u32,
    pub tgid: u32,
    pub start_time_ns: u64,
}

// Chain element
pub struct ProcessLink {
    pub pid: u32,
    pub cmd: String,
    pub user: String,
}

// Process event
pub enum ProcEvent {
    Fork { child_pid: u32, parent_pid: u32, timestamp_ns: u64 },
    Exec { pid: u32, timestamp_ns: u64 },
    Exit { pid: u32 },
}
```

## Modules

```
src/
├── lib.rs    — Crate root, re-exports
├── proc.rs   — Raw /proc reading (comm, status, stat, uid lookup)
├── cache.rs  — PID → ProcInfo cache with TTL and PID reuse detection
├── traits.rs — TreeStore, CacheStore traits and algorithm functions
└── tree.rs   — ProcEvent, ProcessLink types
```

## License

[MIT License](./LICENSE)
