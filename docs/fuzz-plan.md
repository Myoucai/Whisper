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

回归：

```text
fuzz-gate: PASS (20000 runs)
```

## 下一阶段

- [ ] preopen/symlink 路径 fuzz（wrun 前置检查）
- [x] WASM import 白名单 mutation smoke
- [ ] WASM import 覆盖引导 fuzz（atheris/libFuzzer）
- [ ] `wd` 二进制/`.wbin` 解码 fuzz
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
