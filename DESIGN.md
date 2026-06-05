# proc-tree API 设计

## 核心原则

1. **准确性 > 性能** — 返回过期数据比返回 None 更危险
2. **分层暴露** — 按需导入，不强制拉全部功能
3. **结构化输出** — 返回结构体而非字符串，用户自行格式化
4. **增量维护** — 通过 Fork/Exec/Exit 事件驱动，非轮询

## 模块分层

```
proc::         uid_to_username, read_proc_comm/status/start_time
               无状态函数，直接读 /proc，不缓存

ProcCache:     PID → ProcInfo 的 TTL 缓存
               用户只想要 PID→cmd 映射时使用

ProcTree:      进程树 + 缓存的组合体
               全功能：snapshot、增量维护、链查询、后代判断
```

## 关键 API 设计决策

### 1. ProcessLink（结构化链）

不返回 String，返回结构体。原因：
- 用户可能按 cmd 过滤、按 pid 搜索、按 user 聚合
- Display trait 提供 `"102|touch|root;101|sh|root"` 格式化
- 零成本：用户不打印就不分配 String

```rust
pub struct ProcessLink {
    pub pid: u32,
    pub cmd: String,
    pub user: String,
}
// Display → "102|touch|root"
// 链 → Vec<ProcessLink>，从子到祖
```

### 2. PID 复用检测

场景：PID 1234 原先是 nginx，nginx 退出后 PID 被 vim 复用。
- Exec 事件更新缓存 → 新 cmd 是 vim → 正确
- 如果 Exec 事件丢失（proc connector overrun）→ 缓存仍是 nginx → 错误
- 解决：`get()` 时比对 start_time_ns，不匹配则重新读 /proc

### 3. Exit 事件保留节点

Exit 后不清除树节点。原因：
- 文件事件可能在进程退出后才被处理（fanotify 批量读取）
- build_chain 需要查退出进程的祖先链
- 内存由 moka TTL 自动管理

### 4. snapshot 与 handle_event 的原子性

snapshot 是一次性全量填充。handle_event 是增量更新。
两者不混用 —— snapshot 后才调用 handle_event。
API 设计上，snapshot 在 &mut self 上，handle_event 在 &self 上。

### 5. 用户不直接操作 moka Cache

ProcCache 和 PidTree 的内部 cache 是 pub(crate)。
用户通过方法访问，保证 PID 复用检测等逻辑不被绕过。

## 用户场景

### 场景 A：文件监控守护进程
```rust
let mut tree = ProcTree::builder().build();
tree.snapshot();                          // 启动快照
loop {
    select! {
        events = proc_conn.recv() => {
            tree.handle_events(&events);  // 增量维护
        }
        file_event = fanotify.recv() => {
            let info = tree.resolve(file_event.pid);  // PID→进程信息
            let chain = tree.build_chain(file_event.pid);  // 祖先链
        }
    }
}
```

### 场景 B：只想要 PID→cmd 缓存
```rust
let cache = ProcCache::new(65536, Duration::from_secs(600));
cache.update_from_proc(pid);
let info = cache.get(pid);  // → Some(ProcInfo { cmd: "nginx", ... })
```

### 场景 C：轻量 /proc 读取
```rust
let cmd = proc_tree::proc::read_proc_comm(pid);
let user = proc_tree::proc::uid_to_username(uid);
```
