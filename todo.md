# 测试完善计划

## 目标
参考 fanotify-fid/proc-connector/sizefilter/timefilter，为 proc-tree 建立全面测试覆盖。

## 当前状态
- 内联 `#[cfg(test)]` 测试：26 个
- Doc-tests：3 个
- 无独立 `tests/` 目录

## 步骤

- [x] 1. 创建 tests/ 目录结构
- [x] 2. 添加 tests/helpers.rs 共享工具
- [x] 3. 添加 tests/proc.rs (proc 模块函数测试)
- [x] 4. 添加 tests/cache.rs (缓存操作测试)
- [x] 5. 添加 tests/tree.rs (树操作测试)
- [x] 6. 添加 tests/display.rs (格式化输出测试)
- [x] 7. 添加 tests/edge_cases.rs (边界/错误测试)
- [x] 8. 为 lib.rs 公共 API 添加 doc-tests
- [x] 9. 运行测试验证全部通过
- [ ] 10. 提交代码

## 参考模式
- sizefilter: parse.rs 测试各种输入变体、边界值、错误
- proc-connector: edge_cases.rs 测试截断数据、错误格式
- 每个公共函数都有对应测试用例
