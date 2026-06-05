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
- **Short string optimization**: `CompactString` for process names and usernames (<23 bytes on stack)
- **Zero heap allocation** for `/proc` path formatting (`ArrayString`)
- **Thread-safe**: all operations protected by `Mutex`

## Testing

```bash
cargo test
```

95 tests (26 unit + 59 integration + 10 doc-tests) covering snapshot, chain building, descendant checks, display formatting, edge cases (cycles, nonexistent PIDs, concurrent access), and builder configuration.

## Quick example

```rust
use proc_tree::{ProcTree, ProcEvent};

// Seed from /proc
let mut tree = ProcTree::builder().build();
tree.snapshot();

// Resolve a PID
if let Some(info) = tree.resolve(1) {
    println!("PID 1: cmd={}, user={}", info.cmd, info.user);
}

// Build ancestry chain
let chain = tree.build_chain(std::process::id());
for link in &chain {
    println!("  {}", link); // "1234|bash|root"
}

// Check descendant
let info = tree.resolve(1).unwrap();
assert!(tree.is_descendant(std::process::id(), &info.cmd));

// Incremental updates from event source
tree.handle_events(&[
    ProcEvent::Fork { child_pid: 200, parent_pid: 100, timestamp_ns: 0 },
    ProcEvent::Exec { pid: 200, timestamp_ns: 1 },
]);

// Display as pstree
println!("{}", tree.display(1));
```

## API

### ProcTree

| Method | Description |
|--------|-------------|
| `snapshot()` | Scan `/proc` and populate tree + cache |
| `resolve(pid)` | Get process info (with PID reuse detection) |
| `build_chain(pid)` | Build ancestry chain `Vec<ProcessLink>` |
| `build_chain_string(pid)` | Chain as `"pid\|cmd\|user;..."` string |
| `is_descendant(pid, cmd)` | Check if PID descends from process with given cmd |
| `children(pid)` | Get direct children PIDs |
| `descendants(pid)` | Get all descendant PIDs (BFS) |
| `siblings(pid)` | Get sibling PIDs (same parent) |
| `find_by_cmd(cmd)` | Find PIDs by command name |
| `find_by_user(user)` | Find PIDs by username |
| `display(root_pid)` | Render pstree-style display |
| `handle_events(events)` | Process batch of fork/exec/exit events |

### ProcTreeBuilder

```rust
let tree = ProcTree::builder()
    .tree_capacity(1000)        // max PIDs tracked
    .tree_ttl(Duration::from_secs(300))
    .cache_capacity(2000)
    .cache_ttl(Duration::from_secs(600))
    .build();
```

### ProcessLink

```rust
let link = ProcessLink { pid: 102, cmd: "touch".into(), user: "root".into() };
assert_eq!(link.to_string(), "102|touch|root");
```

## Modules

```
src/
├── lib.rs    — Crate root, re-exports
├── proc.rs   — Raw /proc reading (comm, status, stat, uid lookup)
├── cache.rs  — PID → ProcInfo cache with TTL and PID reuse detection
└── tree.rs   — ProcTree, ProcEvent, ProcessLink, builder
```

## Dependencies

| Crate | Purpose |
|-------|---------|
| `libc` | `sysconf(_SC_CLK_TCK)` for jiffies → nanoseconds |
| `compact_str` | Short string optimization (<23 bytes on stack) |
| `arrayvec` | Stack-allocated path formatting |

## License

[MIT License](./LICENSE)
