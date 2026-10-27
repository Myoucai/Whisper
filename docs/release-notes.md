# Whisper 1.3.7 发布说明

## 主题

**VM `@7` arity 修正：http-audit 不再需要 dummy 实参。**

## 修复

此前 VM 的 `HttpAuditCap` 使用默认 arity 1，导致：

```whisper
@7 .
```

栈下溢，必须写成：

```whisper
"" @7 .
```

现在 `HttpAuditCap::arity()` 返回 0，标准写法恢复：

```whisper
@7 .
```

C/native 一直按 arity 0 实现，因此这次修正让 VM 与 C 链真正对齐。

## 测试

- `http-parity-gate` 移除 `"" @7` 绕过
- VM/C 的 @7 使用标准 `@7`
- 全量测试通过

```text
http-parity-gate: PASS (@6/@7 + @8 + @9 status/body/header + redirect 302/Location parity)
cargo test --workspace: pass
```

## 兼容性

- 旧写法 `"" @7 .` 会留下一个多余栈值，建议移除
- 标准 `@7 .` 现在在 VM/C 都正确
- 其他 capability 的 arity 不变

## 已知限制

- WASM 文件数/连接数配额仍需 OS/容器层
- 多地址连接超时仍逐个计时
- 严格只读 preopen 仅 Linux

## 验证

- CI（待推送后确认）
- integration tests: 123/123
- http-parity-gate: PASS
