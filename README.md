# proc-tree

[![Crates.io](https://img.shields.io/crates/v/proc-tree.svg)](https://docs.rs/proc-tree)
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
- **Pluggable storage**: implement `TreeStore` / `CacheStore` for any backend
- **Ready-to-use defaults**: `DefaultTree` / `DefaultCache` (HashMap + Mutex + TTL)
- **Zero heap allocation** for `/proc` path formatting (`ArrayString`)

## Quick start

```rust
use proc_tree::{DefaultTree, DefaultCache, snapshot, resolve, build_chain_string};

let tree = DefaultTree::new(65536, 600);  // capacity, TTL seconds
let cache = DefaultCache::new(65536, 600);

// Seed from /proc
snapshot(&tree, &cache);

// Resolve a PID
if let Some(info) = resolve(&cache, 1) {
    println!("PID 1: cmd={}, user={}", info.cmd, info.user);
}

// Build ancestry chain
let chain = build_chain_string(&tree, &cache, std::process::id());
```

## Custom backend

Implement the traits for any storage (Redis, moka, dashmap, etc.):

```rust
use proc_tree::{TreeStore, CacheStore, PidNode, ProcInfo, handle_events, ProcEvent};

struct MyTree(/* your storage */);
struct MyCache(/* your storage */);

impl TreeStore for MyTree {
    fn get_node(&self, pid: u32) -> Option<PidNode> { todo!() }
    fn insert_node(&self, pid: u32, node: PidNode) { todo!() }
    fn all_pids(&self) -> Vec<u32> { todo!() }
}

impl CacheStore for MyCache {
    fn get_info(&self, pid: u32) -> Option<ProcInfo> { todo!() }
    fn insert_info(&self, pid: u32, info: ProcInfo) { todo!() }
}
```

## API

### Traits

| Trait | Description |
|-------|-------------|
| `TreeStore` | Process tree storage (parent→child relationships) |
| `CacheStore` | Process info cache (PID→metadata mapping) |

### Default implementations

| Type | Description |
|------|-------------|
| `DefaultTree` | `HashMap<Mutex>` with TTL, `Clone` shares data via `Arc` |
| `DefaultCache` | Same as above, for `ProcInfo` |

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

### Low-level `/proc` access

| Function | Description |
|----------|-------------|
| `read_proc_comm(pid)` | Read `/proc/{pid}/comm` |
| `read_proc_status_fields(pid)` | Read user, ppid, tgid from `/proc/{pid}/status` |
| `read_proc_start_time_ns(pid)` | Read start time from `/proc/{pid}/stat` |
| `uid_to_username(uid)` | UID→username via `/etc/passwd` |

### Types

```rust
pub struct PidNode {
    pub ppid: u32,
    pub cmd: String,
}

pub struct ProcInfo {
    pub cmd: String,
    pub user: String,
    pub ppid: u32,
    pub tgid: u32,
    pub start_time_ns: u64,
}

pub struct ProcessLink {
    pub pid: u32,
    pub cmd: String,
    pub user: String,
}

pub enum ProcEvent {
    Fork { child_pid: u32, parent_pid: u32, timestamp_ns: u64 },
    Exec { pid: u32, timestamp_ns: u64 },
    Exit { pid: u32 },
}
```

## Testing

```bash
cargo test
```

90 tests covering snapshot, chain building, descendant checks, display formatting, edge cases (cycles, nonexistent PIDs, concurrent access).

## Modules

```
src/
├── lib.rs            — Re-exports
├── proc.rs           — Raw /proc reading (pub)
├── cache.rs          — ProcInfo type
├── traits.rs         — TreeStore, CacheStore traits + algorithm functions
├── tree.rs           — ProcEvent, ProcessLink types
└── default_store.rs  — DefaultTree, DefaultCache implementations
```

## License

[MIT License](./LICENSE)
