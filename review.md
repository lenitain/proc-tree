# proc-tree + fsmon 深度审查报告

审查日期：2026-06-09
审查范围：proc-tree 全部源码、测试、文档；fsmon 中与 proc-tree 的协调代码

---

## 一、项目理解

### proc-tree — Linux 进程树库

- 从 `/proc` 快照，通过 fork/exec/exit 事件增量维护
- 祖先链查询（`build_chain_string`）、PID 复用检测（`start_time_ns`）、pstree 风格展示（`display`）
- 泛型 `ProcessStore` trait，支持自定义后端（Redis、moka、dashmap 等）
- `DefaultStore` 基于 `Arc<Mutex<HashMap>>` + 可选 TTL 驱逐

### fsmon — 实时文件系统监控

- 基于 fanotify 捕获文件变更，归因到具体进程
- 依赖 proc-tree 做进程树过滤（`is_descendant`）和祖先链构建（`build_chain_string`）
- **关键协调模式：延迟移除（Deferred Removal）** — 进程退出后信息保留在 store，供晚到的 fanotify 事件查询

### 协调架构

fsmon 通过 `proc-tree = { path = "../proc-tree" }` 依赖 proc-tree，使用方式：

| fsmon 用途 | proc-tree API |
|-----------|---------------|
| 启动时种子进程数据 | `snapshot()` |
| 处理 proc-connector 事件 | `handle_events()` |
| 文件事件进程过滤 | `is_descendant()` |
| 构建祖先链 | `build_chain_string()` |
| 解析进程信息 | `DefaultStore::get_process()` |

fsmon 事件循环的两阶段 drain 设计：

```
drain_proc_events → process_event_batch → drain_proc_events → patch_pending_events
     (第一次)            (处理文件事件)         (第二次)          (修补未知字段)
```

这是为了确保 fanotify 事件到达时，proc store 已经包含了最新的进程信息。

---

## 二、内部逻辑问题

### 🔴 P0：TTL 过期不清理自身 children_index

**文件**：`src/default_store.rs` → `get_inner()`

```rust
fn get_inner(...) -> Option<ProcessInfo> {
    let mut map = inner.lock().unwrap();
    let entry = map.get(&pid)?;
    if !ttl.is_zero() && entry.inserted_at.elapsed() >= ttl {
        let info = map.remove(&pid).unwrap().value;
        // 只更新父进程的 children_index
        let mut index = children_index.lock().unwrap();
        if let Some(children) = index.get_mut(&info.ppid) {
            children.retain(|&c| c != pid);
        }
        // ❌ 没有清理 index 中 pid 自己的子列表
        return None;
    }
    Some(entry.value.clone())
}
```

**后果**：
- 子进程先于父进程过期后，父进程的 `children_index` 仍引用已过期的子 PID
- `children(parent_pid)` 返回的 PID 在 store 中查不到（幽灵 PID）
- `descendants()` 遍历时也会遇到幽灵节点

**修复方向**：过期时应同时清理 `children_index[pid]` 本身：

```rust
// 移除 pid 自己的子列表
index.remove(&pid);
// 并将这些子进程重新挂到 init（或标记为孤儿）
```

---

### 🔴 P0：Exit 孤儿化导致 children_index 双重归属

**文件**：`src/ops.rs` → `handle_event()` — Exit 分支

```rust
ProcEvent::Exit { pid } => {
    let children = store.children_of(*pid);
    for child_pid in children {
        if let Some(mut info) = store.get_process(child_pid) {
            info.ppid = 1;
            store.insert_process(child_pid, info);  // 添加到 children_index[1]
        }
    }
    // ❌ 没有从 children_index[old_parent] 移除这些子进程
    return Some(*pid);
}
```

**后果**：
- 同一个子进程同时出现在 `children_index[old_parent]` 和 `children_index[1]` 中
- `children(old_parent)` 返回已经不属于它的子进程
- 虽然设计上父进程随后会被调用方 `remove_process()` 清理，但在延迟窗口内存在不一致

**修复方向**：在孤儿化时，主动从旧父的 children_index 中移除：

```rust
// 在 insert_process 之前，从旧父的 index 中移除
let mut index = store.children_index.lock().unwrap();  // 需要暴露或提供方法
if let Some(list) = index.get_mut(&pid) {  // pid = 旧父
    list.retain(|&c| c != child_pid);
}
```

---

### 🟡 P1：Exec 事件覆盖 start_time_ns

**文件**：`src/ops.rs` → `handle_event()` — Exec 分支

```rust
ProcEvent::Exec { pid, timestamp_ns } => {
    let mut info = crate::proc::parse_proc_entry(*pid).unwrap_or_else(|| ...);
    info.start_time_ns = *timestamp_ns;  // ❌ 覆盖了从 /proc 读取的真实值
    store.insert_process(*pid, info);
}
```

**问题**：
- `timestamp_ns` 是事件时间（proc connector 生成时间），不是进程启动时间
- `parse_proc_entry` 已经从 `/proc/{pid}/stat` 读取了正确的 `start_time_ns`
- 覆盖后 store 中的值与 `/proc` 不一致，破坏 PID 复用检测语义

**修复方向**：不要覆盖 `parse_proc_entry` 返回的 `start_time_ns`，或使用 `/proc` 值：

```rust
// 删除这行：
// info.start_time_ns = *timestamp_ns;
// 或者用 /proc 值（parse_proc_entry 已经做了）
```

---

### 🟡 P1：fsmon 不主动清理已退出进程

**文件**：`fsmon/src/common/monitor/reader.rs` → `drain_proc_conn()`

```rust
fn drain_proc_conn(&self, ...) -> Vec<u32> {
    let mut exited = Vec::new();
    loop {
        match conn.recv_raw(proc_buf) {
            Ok(n) => {
                exited.extend(proc_cache::handle_proc_events(proc_store, proc_buf, n));
            }
            ...
        }
    }
    exited  // 返回值从未被调用方使用
}
```

**后果**：
- 进程条目完全依赖 TTL（600秒）自然过期
- 高 churn 环境（大量短生命周期进程）下 store 可能积累大量僵尸条目
- 增加内存占用和查询延迟

**修复方向**：
- fsmon 应在第二次 drain 后，将 exited PIDs 加入延迟移除队列
- 延迟 N 秒（如 5-10 秒）后调用 `store.remove_process(pid)`
- 或提供一个带 TTL 的"待移除"队列机制

---

## 三、文档不一致

| 位置 | 文档说法 | 实际行为 |
|------|----------|----------|
| README.md → About | "Exit removes the node, children are orphaned to init" | Exit **保留**节点，返回 PID 供调用方决定 |
| README.md → ProcEvent 表格 | "Exit: Removes process, orphans children to init" | 同上 |
| lib.rs 注释 | "caller decides when to remove" | ✅ 正确 |
| ProcessStore trait | 未提及延迟移除语义 | 缺失关键设计说明 |
| default_store.rs 注释 | "HashMap<Mutex> store" | 实际是 `Arc<Mutex<HashMap>>` |

---

## 四、代码风格 & 结构评估

### 优点

- **模块划分清晰**：types/traits/proc/ops/store/tree，职责单一
- **全部 ops 函数泛型化**：可替换后端，fsmon 正是利用了这一点
- **测试覆盖充分**：20 个单元测试 + 16 个文档测试 + 6 个集成测试文件
- **CI 配置完整**：test + fmt + clippy，RUSTFLAGS="-D warnings"
- **优雅的延迟移除设计**：Exit 返回 PID，调用方决定何时清理
- **循环检测**：`walk_ancestors` 和 `build_chain_links` 都有 visited 集合

### 小问题

- `contains_key()` 会静默驱逐过期条目（副作用），但 `len()` 不会 — 行为不一致
- `uid_passwd_map` 用 `OnceLock` 缓存，`/etc/passwd` 变更不会反映（容器环境可能有问题）
- `ProcessInfo` 的 `cmd` 和 `user` 是 `String` 而非 `&str`，对高频查询有分配开销（可接受）
- `children_index` 是独立的 `Arc<Mutex<HashMap>>`，与 `inner` 分别加锁，理论上存在 TOCTOU 窗口

---

## 五、与 fsmon 的协调深度分析

### 延迟移除设计 — 语义正确但实现有缺口

**设计意图**：
fanotify 事件可能在 proc connector exit 事件之后到达，需要查询已退出进程的信息。因此 `handle_event` 对 Exit 事件不移除节点。

**fsmon 实现**：
```rust
// reader.rs drain_proc_conn
exited.extend(proc_cache::handle_proc_events(proc_store, proc_buf, n));
// exited 从未被使用 — 进程永远留在 store 中直到 TTL 过期
```

**缺口**：
1. exited PIDs 从未被移除 — 完全依赖 600 秒 TTL
2. 没有"确认处理完所有相关事件后主动清理"的机制
3. `patch_pending_events` 只查 store，如果进程已过期就查不到

**建议**：
fsmon 应该实现一个延迟移除机制：
```rust
// 概念示意
struct DelayedRemoval {
    pid: u32,
    remove_at: Instant,
}

// 在第二次 drain 后
for pid in exited {
    delayed_queue.push(DelayedRemoval {
        pid,
        remove_at: Instant::now() + Duration::from_secs(5),
    });
}

// 定期检查
while let Some(entry) = delayed_queue.front() {
    if entry.remove_at <= Instant::now() {
        store.remove_process(entry.pid);
        delayed_queue.pop_front();
    } else {
        break;
    }
}
```

### proc-tree 对 fsmon 的适配性

| fsmon 需求 | proc-tree 支持 | 评估 |
|-----------|---------------|------|
| 进程树过滤 | `is_descendant()` | ✅ 正确 |
| 祖先链构建 | `build_chain_string()` | ✅ 正确 |
| 延迟查询已退出进程 | Exit 不移除 | ✅ 设计正确 |
| 批量事件处理 | `handle_events()` | ✅ 正确 |
| 进程信息解析 | `parse_proc_entry()` | ✅ 正确 |
| 高并发读取 | `Arc<Mutex<>>` | ⚠️ 可能成为瓶颈 |

---

## 六、总结

| 优先级 | 问题 | 影响 | 修复难度 |
|--------|------|------|----------|
| 🔴 P0 | TTL 过期不清理自身 children_index | 查询返回幽灵 PID | 中 |
| 🔴 P0 | Exit 孤儿化导致 children_index 双重归属 | children() 返回错误结果 | 中 |
| 🟡 P1 | Exec 覆盖 start_time_ns | PID 复用检测失效 | 低 |
| 🟡 P1 | fsmon 不主动清理已退出进程 | 高 churn 下 store 膨胀 | 中 |
| 🟢 P2 | 文档与实现不一致 | 用户困惑 | 低 |
| 🟢 P2 | contains_key/len 行为不一致 | API 可预测性 | 低 |
| 🟢 P2 | uid_passwd_map 不可刷新 | 容器环境用户名陈旧 | 低 |

---

## 七、测试运行结果

```
单元测试：20 passed, 0 failed
文档测试：16 passed, 0 failed
集成测试：全部通过（cache/display/edge_cases/proc/tree）
```

CI 配置完整，所有测试通过。问题主要是**设计层面的逻辑缺口**，而非代码错误。
