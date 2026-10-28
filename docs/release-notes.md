# Whisper 1.3.11 发布说明

## 主题

**C HTTP/URL 解析器 libFuzzer + ASAN 首次接入；fuzz 发现并修复越界读。**

## 新增

- `fuzz/http_fuzz.c`：一个输入同时覆盖
  - 响应 header 解析
  - chunked 解码
  - 用户 headers JSON
  - hex body 解码
  - URL 解析
- `scripts/fuzz-gate.sh`：clang + `-fsanitize=fuzzer,address`
- CI job `HTTP parser fuzz (libFuzzer)`，20k runs
- 种子目录 `fuzz/corpus/http/`
- 文档 [fuzz-plan.md](fuzz-plan.md)

## 修复

libFuzzer 发现 `w_ci_eq_n` 在 key 长度超过字面量时越界读：

```text
w_ci_eq_n(a, "content-type", n)  // n > 12
w_ci_eq_n(ks, "host", n)         // n > 4
```

修复：任一侧遇到 NUL 立即返回不匹配，不再越过字符串结尾。

## 验证

```text
fuzz-gate: PASS (20000 runs)
CI: 1c8791d success
```

## 兼容性

- 无正常路径行为变化
- 仅修复非法/恶意 header 的越界读
- WASM v1 仍无网络

## 已知限制

- 只覆盖 native C HTTP/URL 解析器
- 不覆盖 VM Rust 解析器、WAMR 内部
- 20k runs 是烟雾级，不是完整覆盖证明
- WASM import/preopen/`wd` 二进制 fuzz 尚未接入

## 下一步

- preopen/symlink 路径 fuzz
- WASM import 白名单 fuzz
- `wd` `.wbin` 解码 fuzz
- nightly 长跑 / OSS-Fuzz 评估
