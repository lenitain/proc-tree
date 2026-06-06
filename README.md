# proc-tree

[![Crates.io](https://img.shields.io/crates/v/proc-tree.svg)](https://crates.io/crates/proc-tree)
[![Docs.rs](https://docs.rs/proc-tree/badge.svg)](https://docs.rs/proc-tree)

Linux process tree — snapshot from `/proc`, incremental maintenance via fork/exec/exit events, ancestry chain queries, PID reuse detection, and pstree-style display.

## Installation

```bash
cargo add proc-tree
```

Minimum supported Rust version: **1.85** (edition 2024).

| Module | Coverage |
|--------|----------|
| `default_store` | Insert/get, TTL expiration, clone shares data, len/contains |
| `ops` | snapshot, resolve, handle_events (fork→exec→exit), children, descendants, siblings, find_by_cmd, find_by_user, is_descendant (with cycle detection), build_chain_string, display, tree_len |
| `proc` | read_proc_comm, read_proc_start_time_ns, uid_to_username, nonexistent PID handling |
| `tree` | ProcessLink display/clone/debug, ProcEvent clone/debug |
| `tests/cache` | Cache hit on resolve, cache updated on exec, cache preserved on exit |
| `tests/display` | Single process, with children, nonexistent PID, ProcessLink special chars |
| `tests/edge_cases` | Cycle detection, PID 0, max PID, large fork batches |
| `tests/proc` | PID 1 fields, self-resolve, nonexistent PID |
| `tests/tree` | Snapshot idempotent, snapshot includes PID 1, fork creates node, multiple children, build_chain with nonexistent PIDs |

All tests run without root privileges.

---

## About this crate

A Linux process tree library. Two use cases:

**1. One-shot snapshot** — take a point-in-time picture of all running processes from `/proc`:

```rust
use proc_tree::{DefaultTree, DefaultCache, snapshot, resolve, display};

let tree = DefaultTree::new(65536, 600);
let cache = DefaultCache::new(65536, 600);
snapshot(&tree, &cache);

// Resolve any PID
let info = resolve(&cache, 1).unwrap();
println!("PID 1: {} ({})", info.cmd, info.user);

// Render pstree-style tree
println!("{}", display(&tree, 1));
```

**2. Incremental maintenance** — seed from snapshot, then keep the tree up-to-date with fork/exec/exit events (e.g. from [`proc-connector`](https://crates.io/crates/proc-connector)):

```rust
use proc_tree::{DefaultTree, DefaultCache, snapshot, handle_events, ProcEvent};

let tree = DefaultTree::new(65536, 600);
let cache = DefaultCache::new(65536, 600);
snapshot(&tree, &cache);

// Events from proc-connector, audit, or any source
handle_events(&tree, &cache, &[
    ProcEvent::Fork { child_pid: 5000, parent_pid: 1234, timestamp_ns: 0 },
    ProcEvent::Exec { pid: 5000, timestamp_ns: 1 },
    ProcEvent::Exit { pid: 5000 },
]);
```

`ProcEvent` is decoupled from any specific event source — adapt events from proc-connector, audit, or any other mechanism.

### PID reuse detection

When a process exits and its PID is reused, cached `ProcInfo` becomes stale. The `start_time_ns` field (nanoseconds since boot, from `/proc/{pid}/stat`) lets implementations detect reuse by comparing cached vs. current values.

### Thread safety

`DefaultTree` / `DefaultCache` are `Arc<Mutex<HashMap>>` — clone shares the same underlying data. Safe to pass across threads.

---

## Quick example

```rust
use proc_tree::{
    DefaultTree, DefaultCache, snapshot, resolve, handle_events,
    build_chain_string, is_descendant, children, descendants, siblings,
    find_by_cmd, find_by_user, display, ProcEvent, PidNode, ProcInfo,
};

// Create stores (capacity hint, TTL in seconds)
let tree = DefaultTree::new(65536, 600);
let cache = DefaultCache::new(65536, 600);

// Seed from /proc
snapshot(&tree, &cache);

// Resolve a PID (cache-first, falls back to /proc)
let info = resolve(&cache, 1).unwrap();
println!("PID 1: cmd={}, user={}, tgid={}", info.cmd, info.user, info.tgid);

// Build ancestry chain: "200|bash|root;100|sshd|root;1|systemd|root"
let chain = build_chain_string(&tree, &cache, std::process::id());

// Query relationships
let kids = children(&tree, 1);          // direct children of PID 1
let all = descendants(&tree, 1);        // all descendants (BFS)
let sibs = siblings(&tree, std::process::id()); // same-parent processes

// Find by name or user
let sshds = find_by_cmd(&tree, "sshd");
let roots = find_by_user(&tree, &cache, "root");

// Ancestry check: is my process a descendant of "sshd"?
let mine = std::process::id();
if is_descendant(&tree, mine, "sshd") {
    println!("running under sshd");
}

// pstree-style display
println!("{}", display(&tree, 1));

// Handle incremental events
handle_events(&tree, &cache, &[
    ProcEvent::Fork { child_pid: 9999, parent_pid: 1, timestamp_ns: 0 },
    ProcEvent::Exec { pid: 9999, timestamp_ns: 1 },
]);
```

---

## Types

### `PidNode`

```rust
pub struct PidNode {
    pub ppid: u32,   // parent PID
    pub cmd: String, // command name from /proc/{pid}/status
}
```

### `ProcInfo`

```rust
pub struct ProcInfo {
    pub cmd: String,          // command name
    pub user: String,         // username (from UID → /etc/passwd lookup)
    pub ppid: u32,            // parent PID
    pub tgid: u32,            // thread group ID
    pub start_time_ns: u64,   // start time in nanoseconds since boot
}
```

### `ProcEvent`

```rust
pub enum ProcEvent {
    Fork { child_pid: u32, parent_pid: u32, timestamp_ns: u64 },
    Exec { pid: u32, timestamp_ns: u64 },
    Exit { pid: u32 },
}
```

| Variant | Behavior |
|---------|----------|
| `Fork` | Inserts a new tree node (`child_pid → parent_pid`), cmd left empty |
| `Exec` | Reads `/proc/{pid}/status` to update cmd, user, ppid, tgid in both tree and cache |
| `Exit` | No-op — node is preserved for historical chain lookups |

### `ProcessLink`

```rust
pub struct ProcessLink {
    pub pid: u32,
    pub cmd: String,
    pub user: String,
}
```

Displayed as `"pid|cmd|user"`. A chain is a `Vec<ProcessLink>` ordered from child to ancestor.

---

## Traits (custom backend)

Implement `TreeStore` and `CacheStore` for any storage (Redis, moka, dashmap, ...):

```rust
pub trait TreeStore {
    fn get_node(&self, pid: u32) -> Option<PidNode>;
    fn insert_node(&self, pid: u32, node: PidNode);
    fn all_pids(&self) -> Vec<u32>;
}

pub trait CacheStore {
    fn get_info(&self, pid: u32) -> Option<ProcInfo>;
    fn insert_info(&self, pid: u32, info: ProcInfo);
}
```

All functions in `ops` are generic over these traits — bring your own storage.

---

## /proc utilities

The `proc` module exposes low-level `/proc` reading functions:

| Function | Description |
|----------|-------------|
| `parse_proc_entry(pid)` | Read `/proc/{pid}/status` → `(PidNode, ProcInfo)` |
| `read_proc_comm(pid)` | Read `/proc/{pid}/comm` → command name |
| `read_proc_start_time_ns(pid)` | Read `/proc/{pid}/stat` → start time in nanoseconds since boot |
| `uid_to_username(uid)` | UID → username via `/etc/passwd` (cached) |

---

## License

[MIT License](./LICENSE)
