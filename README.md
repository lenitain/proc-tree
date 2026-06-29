# proc-tree

Linux process tree — snapshot from /proc, incremental maintenance via fork/exec/exit events, ancestry chain queries, and pstree-style display.

[![Crates.io](https://img.shields.io/crates/v/proc-tree.svg)](https://crates.io/crates/v/proc-tree)
[![Docs.rs](https://docs.rs/proc-tree/badge.svg)](https://docs.rs/proc-tree)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![CI](https://github.com/lenitain/proc-tree/actions/workflows/ci.yml/badge.svg)](https://github.com/lenitain/proc-tree/actions/workflows/ci.yml)

## Overview

**proc-tree** provides a unified interface for maintaining a Linux process tree with O(1) child lookups, incremental updates from process events, and ancestry chain queries. It supports both one-shot snapshots from `/proc` and real-time updates via fork/exec/exit events, with PID reuse detection and thread-safe storage.

### Why proc-tree?

Unlike simple process listing tools that only show a point-in-time snapshot, **proc-tree** maintains an in-memory process tree that can be incrementally updated as processes fork, exec, and exit. This makes it ideal for tools that need to track process hierarchies over time, build ancestry chains, or find processes by name or user. The library's unified storage interface and zero-allocation iteration patterns make it suitable for high-performance system monitoring applications.

## Usage

Add to your `Cargo.toml`:

```toml
[dependencies]
proc-tree = "0.5.0"
```

### Requirements

- Linux with `/proc` filesystem
- No special capabilities required for snapshot mode
- `CAP_NET_ADMIN` for real-time event updates via proc-connector

### Quick start

```rust
use proc_tree::{DefaultStore, snapshot, resolve, display};

let store = DefaultStore::new(600);  // TTL in seconds
snapshot(&store).expect("failed to read /proc");

// Resolve any PID
let info = resolve(&store, 1).unwrap();
println!("PID 1: {} ({})", info.comm(), info.user());  // "systemd" (binary name)
println!("PID 1 cmd: {}", info.cmd());  // "/usr/lib/systemd/systemd --user" (full cmdline)

// Render pstree-style tree
println!("{}", display(&store, 1));
```

### Process matching

Use `comm()` for process tree matching — it returns the binary name from `/proc/pid/comm`:

```rust
use proc_tree::{DefaultStore, snapshot, is_descendant, find_by_cmd};

let store = DefaultStore::new(600);
snapshot(&store).expect("failed to read /proc");

// Check if current process is a descendant of "bash"
let my_pid = std::process::id();
if is_descendant(&store, my_pid, "bash") {
    println!("Running under bash");
}

// Find all bash processes
let bash_pids = find_by_cmd(&store, "bash");
```

### Ancestry chains

`build_chain_string()` returns a JSON array, `build_chain_links()` returns `Vec<ProcessLink>`:

```rust
use proc_tree::{DefaultStore, snapshot, build_chain_string, build_chain_links};

let store = DefaultStore::new(600);
snapshot(&store).expect("failed to read /proc");

let my_pid = std::process::id();

// JSON string (for logging / serialization)
let json = build_chain_string(&store, my_pid);
println!("{}", json);
// [{"pid":1234,"comm":"touch","cmd":"touch /tmp/foo","user":"root"}, ...]

// Structured data (for programmatic access)
let links = build_chain_links(&store, my_pid);
for link in &links {
    println!("{} ({})", link.comm(), link.user());
}
```

### Serde support

`serde` feature is enabled by default — `ProcessInfo` and `ProcessLink` support `Serialize`/`Deserialize`:

```toml
[dependencies]
proc-tree = "0.5.0"
```

To disable:
```toml
[dependencies]
proc-tree = { version = "0.5.0", default-features = false }
```
