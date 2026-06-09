# 任务清单

## 任务：修改 handle_events 返回待删除的 pid 列表

### 目标
修改 proc-tree 的 handle_events 函数，使其返回 Exit 事件的 pid 列表，而不是立即删除进程信息。

### 修改内容
1. 修改 `handle_events` 函数签名，返回 `Vec<u32>`
2. 修改 `handle_event` 函数签名，返回 `Option<u32>`
3. 在 `handle_event` 中，Exit 事件不调用 `remove_process`，只返回 pid
4. 在 `handle_events` 中，收集所有返回的 pid 并返回

### 影响范围
- src/ops.rs：修改 handle_events 和 handle_event 函数
- 测试文件：更新测试以使用新的返回值
- fsmon：更新调用方式

### 验证
- 所有测试通过
- clippy 检查通过
- fsmon 编译通过
