# Whisper 1.3.14 发布说明

## 主题

**`.wbin` 解码加固：恶意长度、深嵌套、越界跳转。**

## 修复/加固

`WbinReader` 现在：

- 所有 name/string/count 长度必须 ≤ 剩余输入
- 单次分配上限 64 MiB
- `PushRef` 嵌套深度上限 128
- v2 `entry` 必须 `< block_count`
- v2 `Jump`/`Branch` 目标必须 `< block_count`
- 空 block program 拒绝

这些检查把潜在的 OOM、栈溢出和后续 linearize panic 变成明确的
`InvalidData` 错误。

## 测试

新增：

- `test_wbin_malformed_lengths_are_rejected`
  - v2 name length 声称 4 GiB，不发生分配
  - 200 层嵌套 PushRef，在 128 层错误返回
- `fuzz_wbin_decode_never_panics`
  - 4096 个随机/带 magic 的输入
  - v1/v2 各半

```text
cargo test --workspace: pass
wbin unit tests: 10 passed
```

## 兼容性

- 合法 v1/v2 文件不变
- 依赖旧“超长 name 也能解析”的畸形文件不再接受
- `.wbin` 仍是内部/可信构建产物；加固面向损坏或恶意输入

## 已知限制

- 这是 property/mutation fuzz，不是覆盖引导 fuzz
- 不覆盖 VM 执行阶段
- 不覆盖 `.wbin` 文件读取前的大小/配额（CLI 层）
- 64 MiB 单次分配仍是上限，不是总内存配额

## 下一步

- preopen 随机路径 fuzz
- `.wbin` 覆盖引导 fuzz target
- 完整安全审计
- cgroup 聚合配额模板
