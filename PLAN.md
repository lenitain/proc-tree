# proc-tree 实施计划

## Phase 1: 基础功能 ✅
- proc:: (comm, status, stat, uid_to_username)
- ProcCache (PID→ProcInfo, TTL, PID reuse detection)
- ProcTree (snapshot, fork/exec/exit, chain, is_descendant)

## Phase 2: 扩展 /proc 读取 ✅
- [x] cmdline 解析: `read_proc_cmdline` → `Vec<String>`
- [x] statm 解析: `read_proc_statm` → `ProcStatm`, `page_size`, `ProcStatmBytes`

## Phase 3: 高级查询 ✅
- [x] children(pid) → `Vec<u32>`
- [x] descendants(pid) → `Vec<u32>` (BFS)
- [x] siblings(pid) → `Vec<u32>`
- [x] find_by_cmd(cmd) → `Vec<u32>`
- [x] find_by_user(user) → `Vec<u32>`
- [x] tree_display(root_pid) → pstree 格式 String

## Phase 4: 容器感知 ✅
- [x] cgroup 解析: `read_proc_cgroup` → cgroup path
- [x] namespace 检测: `read_proc_namespaces` → `ProcNamespaces`

## Phase 5: 资源监控 ❌ 暂缓
- ResourceDelta (CPU time, RSS, IO 差值)
- subtree_resources (子树资源汇总)
- 理由: 监控工具专用，普通用户用不到

## Phase 6: /proc 目录监控 ❌ 暂缓
- ProcMonitor (inotify on /proc)
- 理由: 有 proc-connector 就不需要

## 测试状态
- 40 unit tests ✅
- 4 doc-tests ✅
