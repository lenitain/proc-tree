# TreeStore 与 CacheStore 合并分析

## 当前设计

| Trait | 数据结构 | 字段 | 用途 |
|-------|----------|------|------|
| TreeStore | PidNode | ppid, cmd | 树结构遍历 |
| CacheStore | ProcInfo | cmd, user, ppid, tgid, start_time_ns | 信息查询 |

## 数据重叠

- ppid：两个都存储
- cmd：两个都存储

## 使用场景分析

### TreeStore 使用场景
- children_of(pid)：需要 ppid
- build_chain_links(pid)：需要 ppid, cmd
- is_descendant(pid, cmd)：需要 ppid, cmd
- display(pid)：需要 cmd

### CacheStore 使用场景
- resolve(pid)：需要 cmd, user, ppid, tgid, start_time_ns
- find_by_user(user)：需要 user

### 两者都需要的场景
- build_chain_links(pid)：需要 ppid, cmd, user

## 合并方案

### 方案 1：合并为单一 trait

```rust
pub trait ProcessStore {
    fn get_process(&self, pid: u32) -> Option<ProcessInfo>;
    fn insert_process(&self, pid: u32, info: ProcessInfo);
    fn remove_process(&self, pid: u32) -> Option<ProcessInfo>;
    fn all_pids(&self) -> Vec<u32>;
    fn children_of(&self, pid: u32) -> Vec<u32>;
}

pub struct ProcessInfo {
    pub ppid: u32,
    pub cmd: String,
    pub user: String,
    pub tgid: u32,
    pub start_time_ns: u64,
}
```

**优点**：
- 简化 API
- 消除数据重复
- Exit 时行为一致

**缺点**：
- TTL 过期逻辑需要重新设计
- 可能需要更复杂的存储结构

### 方案 2：保持分离，但统一数据源

```rust
pub trait TreeStore {
    fn get_node(&self, pid: u32) -> Option<PidNode>;
    fn insert_node(&self, pid: u32, node: PidNode);
    fn remove_node(&self, pid: u32) -> Option<PidNode>;
    fn all_pids(&self) -> Vec<u32>;
    fn children_of(&self, pid: u32) -> Vec<u32>;
}

pub trait CacheStore {
    fn get_info(&self, pid: u32) -> Option<ProcInfo>;
    fn insert_info(&self, pid: u32, info: ProcInfo);
    fn remove_info(&self, pid: u32) -> Option<ProcInfo>;
}
```

**优点**：
- 保持职责分离
- 支持不同的存储后端

**缺点**：
- 数据仍然重复
- 需要确保两个存储的数据同步

## 建议

**合并为单一 trait**，理由：
1. 简化 API
2. 消除数据重复
3. Exit 时行为一致
4. 更符合"进程树"的概念

## 影响分析

### 需要修改的函数
- resolve()：使用 ProcessStore
- find_by_user()：使用 ProcessStore
- build_chain_links()：使用 ProcessStore
- handle_event()：使用 ProcessStore

### 不需要修改的函数
- children()：使用 children_of()
- descendants()：使用 children()
- is_descendant()：使用 get_process()
- display()：使用 get_process()
