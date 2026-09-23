# whisperc —— 编译器源码布局

这个目录同时装着**两条**自举路径的输入。它们不是重复品，但必须分清谁权威。

## 权威文件（硬自举链）

| 文件 | 角色 |
|------|------|
| `compiler.all.ws` | **唯一的编译器源码**。`bootstrap.sh` / `build-wd.sh` / `verify-fixedpoint.sh` / `-t c2` 全部用它 |
| `runtime.c` | C 运行时，被生成的 C `#include`（**改完必须 `cargo build -p whisper-cli`**：它被 `include_str!` 烘进二进制） |
| `wd.ws` | Whisper 自己写的驱动（`wd build|run`），追加在 `compiler.all.ws` 之后编译 |
| `seed/compiler_seed.c` | **冻结种子**：自举链的起点。改 `compiler.all.ws` 不需要重生它（它是链条起点，不是编译器副本） |

`compiler.all.ws` 的末尾是一行裸的 `driver` 调用（读 stdin → 编译 → 输出 C）。
`build-wd.sh` / `bootstrap.sh` 用 `sed '$d'` 去掉它再接 `wd.ws`——**只能按这个
结构切除，不能按行号**。

## 软自举链的模块（`whisper bootstrap` 用）

`lexer.ws` / `classify.ws` / `parser.ws` / `main.ws` / `src/compile.ws` 由
`crates/whisper-cli/src/commands/bootstrap.rs` 用 `include_str!` 拼起来，在
Rust VM 上跑一个模块化的编译器。它们**不是** `compiler.all.ws` 的副本，
但两者会漂移——同一条流水线的两种写法。

## 2026-09-21 清理

删掉了四个**零引用**的历史快照：`compiler.ws`（早期手工合并版）、
`src/cgen.ws`、`src/classify.ws`、`src/lexer.ws`。判定是机械的：全仓库
`include_str!` 与 `import` 两种引用方式都数过，四个文件均为 0；模块之间也没有
互相 `import`（Rust 侧拼接）。删掉的版本在 git 历史里。
