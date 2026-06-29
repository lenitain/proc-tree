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
proc-tree = "0.3.0"
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
println!("PID 1: {} ({})", info.cmd(), info.user());

// Render pstree-style tree
println!("{}", display(&store, 1));
```
