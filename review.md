# 项目审查报告（已解决）

## 原始问题

children(), descendants(), find_by_cmd(), find_by_user() 等函数都调用 tree.all_pids()，导致每次都遍历所有进程（包括历史进程），性能较差。

## 解决方案

**分离索引和历史数据**：

```rust
struct DefaultStore {
    inner: HashMap<u32, Entry<PidNode>>,  // 所有节点（包括历史）
    children_index: HashMap<u32, Vec<u32>>,  // 只索引活跃节点
    active_pids: HashSet<u32>,               // 活跃节点集合
}
```

**事件处理**：
- Fork：insert_node() → 更新索引
- Exit：调用 remove_node() → 从索引移除，但保留历史数据

**语义修正**：
- children(pid) → 只返回活跃子进程
- descendants(pid) → 只返回活跃后代
- find_by_cmd(cmd) → 只返回活跃匹配进程
- find_by_user(user) → 只返回活跃匹配进程
- build_chain_links(pid) → 遍历历史数据，返回完整祖先链
- is_descendant(pid, cmd) → 遍历历史数据，判断祖先进程关系

## 测试结果

所有 73 个测试通过（包括 20 个单元测试、43 个集成测试、18 个文档测试）。
