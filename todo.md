# Todo: 减少堆分配

- [x] 1. 添加依赖 compact-str, arrayvec
- [x] 2. 修改 ProcInfo: String → CompactString
- [x] 3. 修改 PidNode: String → CompactString
- [x] 4. 修改 ProcessLink: String → CompactString
- [x] 5. 修改 proc.rs: 栈缓冲区读取 + CompactString
- [x] 6. 修改 tree.rs: 适配新类型
- [x] 7. 更新测试和 doc-tests
- [x] 8. 运行测试验证
- [ ] 9. 提交代码
