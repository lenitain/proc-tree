# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-06-30

### Added

- **`comm` field on `ProcessInfo`**: stores binary name from `/proc/pid/comm` (e.g. `"touch"`), separate from `cmd` which stores full command line from `/proc/pid/cmdline` (e.g. `"touch /tmp/foo"`)
  - `ProcessInfo::comm()` — binary name for matching
  - `ProcessInfo::cmd()` — full command line for display
  - Mirrors how tools like `btop` identify processes

- **`comm` field on `ProcessLink`**: each chain link now carries both `comm` and `cmd`

- **`serde` feature enabled by default**: no longer need to specify `features = ["serde"]`
  ```toml
  # Before
  proc-tree = { version = "0.4", features = ["serde"] }
  # After
  proc-tree = "0.5"
  ```

### Changed

- **`is_descendant()` uses `comm` for matching**:
  ```rust
  // Before: exact match against full cmdline
  info.cmd() == target_cmd  // "touch /tmp/foo" != "touch"
  // After: match against binary name
  info.comm() == target_cmd  // "touch" == "touch"
  ```

- **`find_by_cmd()` uses `comm` for matching**

- **`build_chain_string()` returns JSON array**:
  ```rust
  // Before
  "200|bash|root;100|sshd|root;1|init|root"
  // After
  [{"pid":200,"comm":"bash","cmd":"bash","user":"root"},{"pid":100,"comm":"sshd","cmd":"sshd","user":"root"}]
  ```
  - Escaped by serde, safe for jq and other JSON tools
  - No separator conflicts with `|`, `;`, or `→` in command lines

- **`ProcessLink` derives `Serialize` and `Deserialize`**

### Fixed

- **Process name matching**: commit `0.2.1` changed `parse_proc_entry()` to read full cmdline, but `is_descendant()` still compared against the full string — `touch what.txt` would not match target `touch`. Now uses `comm` (binary name) for matching.

### Migration Guide

**`ProcessInfo::new()`** (breaking):
```rust
// Before (0.4)
ProcessInfo::new("touch /tmp/foo".into(), "root".into(), 0, 1, 0)
// After (0.5)
ProcessInfo::new("touch".into(), "touch /tmp/foo".into(), "root".into(), 0, 1, 0)
```

**`ProcessLink::new()`** (breaking):
```rust
// Before (0.4)
ProcessLink::new(1, "bash".into(), "root".into())
// After (0.5)
ProcessLink::new(1, "bash".into(), "bash -l".into(), "root".into())
```

**Chain string format** (breaking):
```rust
// Before: parse with string split
let parts: Vec<&str> = chain.split(';').collect();
// After: parse with serde
let links: Vec<ProcessLink> = serde_json::from_str(&chain).unwrap();
// Or use build_chain_links() directly for Vec<ProcessLink>
```

## [0.4.0] - 2026-06-29

### Added

- **`serde` feature**: `ProcessInfo` supports `Serialize` and `Deserialize` under the optional `serde` feature flag
  ```toml
  proc-tree = { version = "0.4.0", features = ["serde"] }
  ```

### Changed

- **Struct fields are now private** ([C-STRUCT-PRIVATE](https://rust-lang.github.io/api-guidelines/interoperability.html#types-are-send-and-sync-where-possible-c-send-sync)):
  - `ProcessInfo`: fields `cmd`, `user`, `ppid`, `tgid`, `start_time_ns` are now private
  - `ProcessLink`: fields `pid`, `cmd`, `user` are now private
  - Added `ProcessInfo::new()` constructor and getter methods: `cmd()`, `user()`, `ppid()`, `tgid()`, `start_time_ns()`
  - Added `ProcessLink::new()` constructor and getter methods: `pid()`, `cmd()`, `user()`

- **`snapshot()` now returns `Result`** ([C-FAILURE](https://rust-lang.github.io/api-guidelines/development.html#all-types-have-good-developer-experience-c-good-dev-experience)):
  ```rust
  // Before (0.3.0)
  snapshot(&store);
  
  // After (0.4.0)
  snapshot(&store).expect("failed to read /proc");
  ```

### Fixed

- **`DefaultStore` implements `Debug`** ([C-DEBUG](https://rust-lang.github.io/api-guidelines/development.html#all-types-have-good-developer-experience-c-good-dev-experience)):
  ```rust
  let store = DefaultStore::new(600);
  println!("{:?}", store); // DefaultStore { len: 42, ttl: 600s }
  ```

- **`ProcessInfo` implements `PartialEq` and `Eq`** ([C-COMMON-TRAITS](https://rust-lang.github.io/api-guidelines/interoperability.html#commonly-used-types-should-be-the-same-c-common-traits)):
  ```rust
  let info1 = store.get_process(1).unwrap();
  let info2 = store.get_process(1).unwrap();
  assert_eq!(info1, info2); // Works now!
  ```

- **`ProcessLink` implements `PartialEq` and `Eq`** ([C-COMMON-TRAITS](https://rust-lang.github.io/api-guidelines/interoperability.html#commonly-used-types-should-be-the-same-c-common-traits))

### Migration Guide

**Struct literal syntax** (breaking):
```rust
// Before (0.3.0)
let info = ProcessInfo {
    ppid: 0,
    cmd: "init".into(),
    user: "root".into(),
    tgid: 1,
    start_time_ns: 0,
};

// After (0.4.0)
let info = ProcessInfo::new(
    "init".into(),
    "root".into(),
    0,
    1,
    0,
);
```

**Field access** (breaking):
```rust
// Before (0.3.0)
println!("{}", info.cmd);

// After (0.4.0)
println!("{}", info.cmd());
```

**snapshot error handling** (breaking):
```rust
// Before (0.3.0)
snapshot(&store);

// After (0.4.0)
snapshot(&store).expect("failed to read /proc");
// or
if let Err(e) = snapshot(&store) {
    eprintln!("Failed to snapshot: {}", e);
}
```

## [0.3.0] - 2026-06-14

### Changed

- **Zero-allocation child iteration**: added `for_each_child(&self, pid, f)` to `ProcessStore` trait — iterates children via callback without allocating a return `Vec`
- **`children_of` is now a default method**: convenience wrapper around `for_each_child`, backward compatible
- **`uid_to_username` returns `&'static str`**: no longer clones from the static `/etc/passwd` cache on every call
- **`handle_event` Exit handler**: uses `for_each_child` directly for zero-allocation child orphaning
- **`build_chain_string`**: eliminated intermediate `Vec<String>` allocation, writes directly via `fmt::Write`
- **`render_tree`**: eliminated `Vec<&str>` allocation for line iteration, replaced `format!()` with `push`/`push_str`
- **`UNKNOWN` constant**: shared across `ops.rs` and `proc.rs` to avoid repeated `"unknown".to_string()` heap allocations

### Fixed

- **Variable naming**: `_shell` renamed to `_password` (was actually the password field in `/etc/passwd`, not the shell field)

## [0.2.1] - 2026-06-11

### Fixed

- **Process name truncation**: `parse_proc_entry()` now reads full command from `/proc/PID/cmdline` instead of the truncated `Name:` field (15 char limit on Linux), fixing `is_descendant()` failures for long command names like `bun /home/user/.bun/bin/pi`
  - Falls back to `Name:` for kernel threads where `cmdline` is empty
  - Added `read_proc_cmdline(pid)` helper function
  - Added 4 unit tests for the new function

## [0.2.0] - 2026-06-09

### Added

- **`ExitedProcess` handle**: explicit removal mechanism for exited processes
  - `handle_event()` returns `Option<ExitedProcess>` instead of `Option<u32>`
  - `handle_events()` returns `Vec<ExitedProcess>` instead of `Vec<u32>`
  - `ExitedProcess::remove(store)` explicitly removes process from store
  - `ExitedProcess::pid()` getter for reading PID without consuming
  - `#[must_use]` on `ExitedProcess`, `handle_event`, `handle_events` to prevent accidental ignoring

### Changed

- **Unified storage interface**: merged `TreeStore` and `CacheStore` into single `ProcessStore` trait
- **Unified data type**: merged `PidNode` and `ProcInfo` into single `ProcessInfo` struct
- **Unified store**: `DefaultStore` replaces `DefaultTree` and `DefaultCache`
- **O(1) child lookups**: added `children_index` for efficient child process queries
- **Removed capacity parameter**: `DefaultStore::new(ttl_secs)` instead of `DefaultStore::new(capacity, ttl_secs)`
- **Private proc module**: internal `/proc` reading functions no longer exposed in public API
- **Improved documentation**: added internal usage notes for proc module functions
- **Process tree semantics**: Exit event returns `ExitedProcess` handle; process stays in store until explicit removal

### Removed

- `TreeStore` trait (merged into `ProcessStore`)
- `CacheStore` trait (merged into `ProcessStore`)
- `PidNode` struct (merged into `ProcessInfo`)
- `ProcInfo` struct (merged into `ProcessInfo`)
- `DefaultTree` type alias (replaced by `DefaultStore`)
- `DefaultCache` type alias (replaced by `DefaultStore`)
- Capacity parameter from `DefaultStore::new()`

### Fixed

- **`children_index` consistency**: ppid changes now properly update both old and new parent's index
  - `insert_process` detects ppid change and removes from old parent's index
  - `insert_process` prevents duplicate entries when re-inserting same ppid
  - `remove_process` cleans up own children_index entry
  - TTL expiration cleans up own children_index entry
- **PID reuse detection**: Exec handler no longer overwrites `start_time_ns` from `/proc` with event timestamp
- **API consistency**: `contains_key()` no longer triggers TTL eviction, consistent with `len()`
- **Clippy warnings**: replaced `or_insert_with(Vec::new)` with `or_default()`

## [0.1.1] - 2026-06-06

### Changed

- **Unified `/proc` reading layer**: removed `read_proc_status_fields()`, all `/proc/status` parsing now goes through `parse_proc_entry()`
- **Simplified `display()`**: split into `display()` (root) and `display_subtree()` (recursive), eliminated `is_root` parameter
- **Extracted `get_cmd()` helper**: shared command name resolution with fallback chain (tree → /proc → "unknown")
- **Removed `cache.rs`**: eliminated unnecessary re-export module, `ProcInfo` now exported directly from `types`
- **Simplified `resolve()`**: uses `parse_proc_entry()` directly instead of separate field reads
- **Simplified `handle_event()`**: Exec handler uses `parse_proc_entry()` for cleaner code
- **Simplified `build_chain_links()`**: uses `parse_proc_entry()` instead of separate `/proc` reads

### Removed

- **`read_proc_status_fields()`**: redundant with `parse_proc_entry()`, removed to eliminate duplicate parsing
- **`cache` module**: was only a re-export of `ProcInfo` from `types`

### Fixed

- **Code quality**: thermo-nuclear review fixes for structural simplification

## [0.1.0] - 2026-06-05

### Added

- **Trait-based storage**: `TreeStore` and `CacheStore` traits for pluggable backends
- **Default implementations**: `DefaultStore<V>` generic store, with `DefaultTree` and `DefaultCache` as type aliases, backed by `HashMap<Mutex>` with TTL
- **`Default` impl**: `DefaultStore::default()` creates a store with capacity 100 and no TTL
- **Snapshot**: one-shot `/proc` scan via `snapshot()` to seed tree and cache
- **`parse_proc_entry()`**: reusable function to parse a single `/proc/{pid}/status` into `(PidNode, ProcInfo)`
- **Incremental updates**: `handle_event()` / `handle_events()` for fork/exec/exit events
- **Ancestry queries**: `build_chain_links()`, `build_chain_string()`, `is_descendant()`
- **Tree queries**: `children()`, `descendants()`, `siblings()`
- **Search**: `find_by_cmd()`, `find_by_user()`
- **Display**: `display()` for pstree-style output
- **PID reuse detection**: via `start_time_ns` comparison in `resolve()`
- **Public `proc` module**: `read_proc_comm()`, `read_proc_status_fields()`, `uid_to_username()`, `read_proc_start_time_ns()` for direct `/proc` access
- **Zero heap allocation** for `/proc` path formatting (`ArrayString`)
- **Thread safety**: trait methods accept `&self` for interior mutability
- **Test suite**: 92 tests (21 unit + 52 integration + 19 doc-tests)
- **Documentation**: README, doc-tests for all public APIs

### Changed

- **Module restructure**: `traits.rs` (569 lines) split into `types.rs` (data types), `traits.rs` (trait definitions only), `ops.rs` (all algorithm implementations)
- **`DefaultTree`/`DefaultCache`**: now type aliases for `DefaultStore<PidNode>` and `DefaultStore<ProcInfo>`
- **`snapshot()`**: refactored to use `parse_proc_entry()` internally
- **`display()`**: merged duplicate `display`/`display_inner` into single recursive function

### Fixed

- **`build_chain_links` cycle detection**: `visited` check moved before `push` to prevent duplicate entries in cyclic chains
- **`clock_ticks_per_sec`**: cached with `OnceLock` instead of calling `sysconf` per PID
