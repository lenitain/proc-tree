# 重构规划

## 问题分析

当前设计错误地引入了"活跃进程"和"历史进程"的区分，不符合 Linux 实际行为。

## 正确设计

- 进程树只包含**活着的**进程
- Exit 时，从进程树中**删除**
- 子进程托孤给 init（ppid = 1）

## 修改范围

### 1. traits.rs
- 移除 `active_pids()` 方法
- 保留：`get_node`, `insert_node`, `remove_node`, `all_pids`, `children_of`

### 2. default_store.rs
- 移除 `active_pids` 字段
- 修改 `remove_node()`：从 `inner` 中删除节点
- 新增逻辑：删除节点时，将其子进程的 ppid 更新为 1

### 3. ops.rs
- 修改 `handle_event` Exit 处理：
  1. 获取该节点的所有子进程
  2. 将子进程的 ppid 更新为 1
  3. 从进程树中删除该节点

### 4. lib.rs
- 移除 `active_pids` 相关导出

## 不需要修改的部分

- `children_of()` 逻辑不变
- `build_chain_links()` 逻辑不变（遍历当前进程树）
- `is_descendant()` 逻辑不变（遍历当前进程树）
- `snapshot()` 逻辑不变
- `resolve()` 逻辑不变

## 预估工作量

- 修改 4 个文件
- 移除 `active_pids` 相关代码
- 添加子进程托孤逻辑
- 更新测试
