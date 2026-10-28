# Whisper 1.3.13 发布说明

## 主题

**preopen 路径加固：绝对 guest 路径、拒绝 `.`/`..`/symlink 逃逸。**

## 修复/加固

`wrun` 现在对每个 preopen 校验：

- guest 路径必须非空且绝对（`/data`）
- 拒绝 `.`、`..`、`//` 组件
- 拒绝 `:`、换行、回车
- 宿主 preopen 根路径不能是 symlink
- preopen 内部不能含 symlink
- 合法绝对路径仍正常映射

覆盖：

- `--dir`
- `--dir-ro`
- `--dir-ro-strict`

## 测试

`wrun-gate` 新增 W-11：

```text
W-11 ✅ 相对 guest preopen 路径拒绝
W-11 ✅ preopen 内部 symlink 拒绝（Linux）
W-11 ✅ preopen 根 symlink 拒绝（Linux）
W-11 ✅ 绝对 guest preopen 路径接受
wrun-gate: 14/14
```

Windows 本机无法创建真实 symlink 时，symlink 子项明确 SKIP；Linux CI 会实际执行。

## 兼容性

- 正常绝对路径 preopen 不变
- 相对 guest 路径从“依赖 WAMR 行为”改为明确拒绝
- symlink 拒绝更严格；这是安全边界
- `--dir-ro` 仍是快照，`--dir-ro-strict` 仍是 Linux EROFS

## 已知限制

- 检查与实际挂载之间仍存在 TOCTOU 窗口
- 需要宿主目录在执行期间不被替换
- symlink 检查依赖 `find`
- 不覆盖 Windows junction/reparse point

## 验证

- wrun-gate: 14/14（本机）
- cargo check --workspace: pass
- Linux CI 将执行真实 symlink 用例
