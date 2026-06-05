# Thermo-Nuclear 代码质量改进任务

## 任务清单

- [ ] 1. 删除 cache.rs（无用间接层）
- [ ] 2. 统一 /proc 读取层 - 消除重复解析
- [ ] 3. 提取通用查询函数 - 合并 find_by_cmd/find_by_user 重复逻辑
- [ ] 4. 简化 display_inner - 重构 is_root 处理
- [ ] 5. 统一错误处理 - 一致的返回类型

## 注意事项

- 不要修改链式语法（let ... && let ...）
- 每个任务完成后运行测试确认
- 完成后删除此文件
