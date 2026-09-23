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

## 软自举链的模块（**2026-09-21 已删除**）

`lexer.ws` / `classify.ws` / `parser.ws` / `main.ws` / `src/compile.ws` 曾经是
`whisper bootstrap` 驱动的第二棵编译器。收口战役 D-1 把它删了：

- 实测它只是**部分可用**（基础算术/单行定义/引号/跨行字符串可以；
  `@map` 融合、import 展开、`print`/`exit`、CRLF 归一化都不行）；
- 它与 `compiler.all.ws` 各写一遍同一条流水线，而两者之间的漂移**没有任何
  机制把关**（`test_pipeline_compare` 只比对一个表达式的结果，对手还是 Rust
  前端）；
- `compiler.all.ws` **已自带全部同名词**（`tokenize`/`structify`/
  `classify-nested`/`compile-one`/`is-blank`/`read-str-acc`，已逐个核对），
  所以删掉它不丢任何能力。

现在 `whisper bootstrap <file>` 直接委托给权威流水线，把生成的 C 打到
stdout；`bootstrap_matches_c2_byte_for_byte` 逐字节钉住它与 `build -t c2`
一致。`--hard` 保留为历史别名（CI 曾用它），接受但不改变行为。

## 2026-09-21 清理

删掉了四个**零引用**的历史快照：`compiler.ws`（早期手工合并版）、
`src/cgen.ws`、`src/classify.ws`、`src/lexer.ws`，以及上面那棵模块化树。
判定是机械的：全仓库 `include_str!` 与 `import` 两种引用方式都数过。删掉的
版本在 git 历史里。
