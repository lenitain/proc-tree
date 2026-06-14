# proc-tree

[![Crates.io](https://img.shields.io/crates/v/proc-tree.svg)](https://crates.io/crates/v/proc-tree)
[![Docs.rs](https://docs.rs/proc-tree/badge.svg)](https://docs.rs/proc-tree)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/lenitain/proc-tree/actions/workflows/ci.yml/badge.svg)](https://github.com/lenitain/proc-tree/actions/workflows/ci.yml)

Linux process tree — snapshot from `/proc`, incremental maintenance via fork/exec/exit events, ancestry chain queries, PID reuse detection, and pstree-style display.

## Installation

```bash
cargo add proc-tree
```

Minimum supported Rust version: **1.85** (edition 2024).

---

## About this crate

A Linux process tree library with a unified storage interface.

**Key design decisions:**
- **Process tree only contains living processes** — Exit removes the node, children are orphaned to init (PID 1)
- **Unified storage** — single `ProcessStore` trait for both tree structure and process info
- **O(1) child lookups** — `children_index` maintained on insert/remove

---

## Quick Start

### One-shot snapshot

```rust
use proc_tree::{DefaultStore, snapshot, resolve, display};

let store = DefaultStore::new(600);  // TTL in seconds
snapshot(&store);

// Resolve any PID
let info = resolve(&store, 1).unwrap();
println!("PID 1: {} ({})", info.cmd, info.user);

// Render pstree-style tree
println!("{}", display(&store, 1));
```

### Incremental maintenance

```rust
use proc_tree::{DefaultStore, snapshot, handle_events, ProcEvent};

let store = DefaultStore::new(600);
snapshot(&store);

// Events from proc-connector, audit, or any source
let exited = handle_events(&store, &[
    ProcEvent::Fork { child_pid: 5000, parent_pid: 1234, timestamp_ns: 0 },
    ProcEvent::Exec { pid: 5000, timestamp_ns: 1 },
    ProcEvent::Exit { pid: 5000 },  // Children orphaned to init
]);

// Caller explicitly removes when done processing related events
for ep in exited {
    ep.remove(&store);
}
```

---

## Complete Example

```rust
use proc_tree::{
    DefaultStore, snapshot, resolve, handle_events,
    build_chain_string, is_descendant, children, descendants, siblings,
    find_by_cmd, find_by_user, display, ProcEvent, ProcessStore,
};

// Create store (TTL in seconds)
let store = DefaultStore::new(600);

// Seed from /proc
snapshot(&store);

// Resolve a PID
let info = resolve(&store, 1).unwrap();
println!("PID 1: cmd={}, user={}, tgid={}", info.cmd, info.user, info.tgid);

// Build ancestry chain: "200|bash|root;100|sshd|root;1|systemd|root"
let chain = build_chain_string(&store, std::process::id());

// Query relationships (O(1) for children)
let kids = children(&store, 1);          // direct children of PID 1
let all = descendants(&store, 1);        // all descendants (BFS)
let sibs = siblings(&store, std::process::id()); // same-parent processes

// Find by name or user (O(n) - requires full scan)
let sshds = find_by_cmd(&store, "sshd");
let roots = find_by_user(&store, "root");

// Ancestry check
let mine = std::process::id();
if is_descendant(&store, mine, "sshd") {
    println!("running under sshd");
}

// pstree-style display
println!("{}", display(&store, 1));
```

---

## Types

### `ProcessInfo`

```rust
pub struct ProcessInfo {
    pub ppid: u32,            // parent PID
    pub cmd: String,          // command name from /proc/{pid}/status
    pub user: String,         // username (from UID → /etc/passwd lookup)
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
| `Fork` | Inserts a new process (`child_pid → parent_pid`), cmd left empty |
| `Exec` | Reads `/proc/{pid}/status` to update cmd, user, ppid, tgid |
| `Exit` | Returns `ExitedProcess` handle, orphans children to init (PID 1) |

### `ProcessLink`

```rust
pub struct ProcessLink {
    pub pid: u32,
    pub cmd: String,
    pub user: String,
}
```

Displayed as `"pid|cmd|user"`. A chain is a `Vec<ProcessLink>` ordered from child to ancestor.

### `ExitedProcess`

```rust
pub struct ExitedProcess {
    pub pid: u32,
}
```

Returned by `handle_event` / `handle_events` for Exit events. The process info **stays in the store** until `remove()` is called, allowing late-arriving events to still look up process info.

```rust
let exited = handle_event(&store, &ProcEvent::Exit { pid: 100 }).unwrap();
assert!(store.get_process(100).is_some());  // still accessible
exited.remove(&store);                       // explicitly remove when done
assert!(store.get_process(100).is_none());
```

---

## Trait (custom backend)

Implement `ProcessStore` for any storage (Redis, moka, dashmap, ...):

```rust
pub trait ProcessStore {
    fn get_process(&self, pid: u32) -> Option<ProcessInfo>;
    fn insert_process(&self, pid: u32, info: ProcessInfo);
    fn remove_process(&self, pid: u32) -> Option<ProcessInfo>;
    fn all_pids(&self) -> Vec<u32>;
    fn for_each_child(&self, pid: u32, f: &mut dyn FnMut(u32));
    fn children_of(&self, pid: u32) -> Vec<u32>; // default impl via for_each_child
}
```

All functions in `ops` are generic over this trait — bring your own storage.

> **Performance note**: `for_each_child` is the core method — it iterates children without allocating a return `Vec`. The `children_of` convenience method has a default implementation that collects into a `Vec`. Hot paths (e.g., `handle_event` Exit handler) use `for_each_child` directly for zero-allocation iteration.

---

## Performance

| Operation | Complexity | Notes |
|-----------|------------|-------|
| `children(pid)` | O(1) | Uses `children_index` |
| `descendants(pid)` | O(k) | k = number of descendants |
| `build_chain_links(pid)` | O(d) | d = depth of process |
| `is_descendant(pid, cmd)` | O(d) | d = depth of process |
| `find_by_cmd(cmd)` | O(n) | n = total processes |
| `find_by_user(user)` | O(n) | n = total processes |
| `snapshot()` | O(n) | n = total processes |

---

## PID Reuse Detection

When a process exits and its PID is reused, cached data becomes stale. The `start_time_ns` field (nanoseconds since boot, from `/proc/{pid}/stat`) lets implementations detect reuse by comparing cached vs. current values.

---

## Thread Safety

`DefaultStore` is `Arc<Mutex<HashMap>>` — clone shares the same underlying data. Safe to pass across threads.

---

## License

[MIT License](./LICENSE)
