# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-06-05

### Added

- **Core types**: `ProcTree`, `ProcTreeBuilder`, `ProcEvent`, `ProcessLink`, `ProcInfo`
- **Snapshot**: one-shot `/proc` scan via `snapshot()`
- **Incremental updates**: `handle_event()` / `handle_events()` for fork/exec/exit events
- **Ancestry queries**: `build_chain()`, `build_chain_string()`, `is_descendant()`
- **Tree queries**: `children()`, `descendants()`, `siblings()`
- **Search**: `find_by_cmd()`, `find_by_user()`
- **Display**: `display()` for pstree-style output
- **PID reuse detection**: via `start_time_ns` comparison in cache
- **Cache**: TTL-based `ProcCache` with capacity eviction
- **Short string optimization**: `CompactString` for cmd/user fields
- **Stack path formatting**: `ArrayString` for `/proc/{pid}/...` paths
- **Thread safety**: all operations protected by `Mutex`
- **Builder pattern**: configurable tree/cache capacity and TTL
- **Test suite**: 95 tests (26 unit + 59 integration + 10 doc-tests)
- **Documentation**: README, doc-tests for all public APIs
