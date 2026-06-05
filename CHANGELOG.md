# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
