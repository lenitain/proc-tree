# proc-tree 实施计划

## Phase 2: 扩展 /proc 读取

### Task 2.1: cmdline 解析
- 读 `/proc/{pid}/cmdline`（NUL 分隔）→ `Vec<String>`
- 加入 `ProcInfo` 的 `cmdline` 字段（`Option<Vec<String>>`）
- 测试：PID 1 的 cmdline、不存在的 PID

### Task 2.2: statm 解析
- 读 `/proc/{pid}/statm` → `ProcStatm { size, resident, shared, text, lib, data, dt }`
- 单位是页，提供 `to_bytes(page_size)` 方法
- 测试：PID 1 的内存、页大小转换

## Phase 3: 高级查询

### Task 3.1: children / descendants / siblings
- `ProcTree::children(pid)` → `Vec<u32>`（直接子进程）
- `ProcTree::siblings(pid)` → `Vec<u32>`（同父进程的其他子进程）
- `ProcTree::descendants(pid)` → `Vec<u32>`（所有后代，BFS）
- 测试：手动构建树验证关系

### Task 3.2: find_by_cmd / find_by_user
- `ProcTree::find_by_cmd(cmd)` → `Vec<u32>`
- `ProcTree::find_by_user(user)` → `Vec<u32>`
- 遍历树 + 缓存匹配
- 测试：查找已知进程

### Task 3.3: tree_display
- `ProcTree::tree_display(root_pid)` → `String`（pstree 格式）
- 格式：`systemd─┬─bash───vim\n       └─nginx───worker`
- 测试：手动构建树验证输出格式

## Phase 4: 容器感知

### Task 4.1: cgroup 解析
- 读 `/proc/{pid}/cgroup` → `CgroupInfo { hierarchy, controllers, path }`
- 提取容器 ID（从 cgroup path 中的 docker/k8s container ID）
- `ProcInfo` 增加 `cgroup: Option<String>` 字段
- 测试：当前进程的 cgroup

### Task 4.2: namespace 检测
- 读 `/proc/{pid}/ns/*` → `ProcNamespaces { pid, net, mnt, user, ... }`
- `ProcTree::same_namespace(pid1, pid2, ns_type)` → `bool`
- 测试：当前进程的 namespace

## Phase 5: 资源监控

### Task 5.1: ResourceDelta
- `ProcTree::resource_delta(pid, prev: &ProcInfo)` → `ResourceDelta`
- 计算 cpu_time、rss、io 的差值
- 测试：两次快照之间的差值

### Task 5.2: subtree_resources
- `ProcTree::subtree_resources(pid)` → `ResourceUsage`
- 递归汇总子树的 cpu_time、rss、fd_count
- 测试：手动构建树验证汇总

## Phase 6: /proc 目录监控

### Task 6.1: ProcMonitor
- `ProcMonitor::new()` → inotify 监控 /proc
- `ProcMonitor::poll_events()` → `Vec<ProcLifecycle>`
- `ProcLifecycle::Created(pid)` / `Exited(pid)`
- 测试：启动/停止子进程触发事件
