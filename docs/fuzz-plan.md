# Fuzz 计划与当前覆盖

## 当前覆盖

CI job `HTTP parser fuzz (libFuzzer)` 使用 clang + ASAN + libFuzzer：

```bash
bash scripts/fuzz-gate.sh
```

目标：`fuzz/http_fuzz.c`

同一输入会进入：

- `@6` 响应 header 解析（`w_headers_json`）
- `Transfer-Encoding: chunked` 解码（`w_dechunk`）
- 用户 headers JSON（`w_user_headers`）
- `@8` hex body 解码（`w_hex_decode`）
- URL 解析（`w_url_parse`）

参数：

```text
-runs=20000 -max_len=4096
```

种子目录：`fuzz/corpus/http/`。

## 已发现并修复

libFuzzer 在 header/key 长度超过字面量时发现越界读：

```text
w_ci_eq_n(a, "content-type", n)  // n > 12
w_ci_eq_n(ks, "host", n)         // n > 4
```

修复：`w_ci_eq_n` 在任一侧遇到 NUL 时返回不匹配，不再越过字符串结尾。

## WASM import mutation smoke

`scripts/wasm-fuzz-gate.sh` 是无依赖变异测试：

- 构造含 `wasi_snapshot_preview1.fd_write` 的合法 import section
- 随机翻转/截断/插入/覆盖
- 调用 `scripts/wasm-imports.py`
- 只允许退出码 0/1；崩溃、其他码、2s 超时都失败
- 默认 300 次；`WASM_FUZZ_ITERS` 可调

这不是覆盖引导 fuzz；下一步可接 atheris/libFuzzer。

## preopen 路径 property gate

`wrun-gate` W-11 覆盖：

- guest preopen 必须是绝对路径
- 拒绝 `.`/`..`/`//`/`:`/换行
- 拒绝宿主 preopen 根 symlink
- 拒绝 preopen 内部 symlink
- 合法绝对路径仍可运行

这不是随机路径 fuzz；它是确定性的 fail-closed 回归，symlink 不可创建时 SKIP。

## `.wbin` 解码 property fuzz

`crates/whisper-codegen/src/wbin.rs` 单元测试：

- `fuzz_wbin_decode_never_panics`：4096 个随机/带 magic 的输入
- `test_wbin_malformed_lengths_are_rejected`：
  - v2 name length 声称 4 GiB，不分配
  - 200 层嵌套 PushRef，在深度上限处失败

同时加固：

- 所有 name/string/count 长度必须 ≤ 剩余字节
- 单个分配上限 64 MiB
- PushRef 嵌套深度上限 128
- v2 entry/jump/branch 目标必须 < block_count

这不是覆盖引导 fuzz；CI 由 `cargo test --workspace` 执行。

回归：

```text
fuzz-gate: PASS (20000 runs)
```

## 下一阶段

- [x] preopen/symlink 路径 property gate（W-11）
- [ ] preopen 随机路径 fuzz
- [x] WASM import 白名单 mutation smoke
- [ ] WASM import 覆盖引导 fuzz（atheris/libFuzzer）
- [x] `wd` `.wbin` mutation/property fuzz（4096 + malformed）
- [ ] `.wbin` 覆盖引导 fuzz / fuzz target
- [ ] 更长 corpus + 定期 nightly
- [ ] OSS-Fuzz 集成评估

## 运行

```bash
# 本地需要 clang；CI 安装 clang
FUZZ_RUNS=100000 bash scripts/fuzz-gate.sh
WASM_FUZZ_ITERS=1000 bash scripts/wasm-fuzz-gate.sh
```

ASAN 报错时 fuzzer 会写 crash 文件；CI 日志保留崩溃输入摘要。

## 限制

- 当前只覆盖 native C HTTP/URL 解析器
- WASM import 是 mutation smoke，不是覆盖引导 fuzz
- 不覆盖 VM Rust 解析器
- 不覆盖 WAMR 内部
- 20k runs 是烟雾级，不是完整覆盖证明
