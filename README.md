# proc-tree

[![Crates.io](https://img.shields.io/crates/v/proc-tree.svg)](https://docs.rs/proc-tree)
[![Docs.rs](https://docs.rs/proc-tree/badge.svg)](https://docs.rs/proc-tree)

Linux process tree — snapshot, incremental fork/exec/exit maintenance, ancestry queries, PID reuse detection.

## Quick start

```rust
use proc_tree::{DefaultTree, DefaultCache, snapshot, resolve, build_chain_string};

let tree = DefaultTree::new(65536, 600);
let cache = DefaultCache::new(65536, 600);

snapshot(&tree, &cache);

let info = resolve(&cache, 1).unwrap();
println!("PID 1: {} ({})", info.cmd, info.user);

let chain = build_chain_string(&tree, &cache, std::process::id());
```

## Custom backend

Implement `TreeStore` and `CacheStore` for any storage (Redis, moka, dashmap, ...). See [docs.rs](https://docs.rs/proc-tree) for the trait definitions.

## License

[MIT LICENSE](./LICENSE)
