# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-05

### Added

- **Trait-based storage**: `TreeStore` and `CacheStore` traits for pluggable backends
- **Default implementations**: `DefaultTree` and `DefaultCache` backed by `HashMap<Mutex>` with TTL
- **Snapshot**: one-shot `/proc` scan via `snapshot()` to seed tree and cache
- **Incremental updates**: `handle_event()` / `handle_events()` for fork/exec/exit events
- **Ancestry queries**: `build_chain_links()`, `build_chain_string()`, `is_descendant()`
- **Tree queries**: `children()`, `descendants()`, `siblings()`
- **Search**: `find_by_cmd()`, `find_by_user()`
- **Display**: `display()` for pstree-style output
- **PID reuse detection**: via `start_time_ns` comparison in `resolve()`
- **Public `proc` module**: `read_proc_comm()`, `read_proc_status_fields()`, `uid_to_username()`, `read_proc_start_time_ns()` for direct `/proc` access
- **Zero heap allocation** for `/proc` path formatting (`ArrayString`)
- **Thread safety**: trait methods accept `&self` for interior mutability
- **Test suite**: 90 tests (19 unit + 52 integration + 19 doc-tests)
- **Documentation**: README, doc-tests for all public APIs
