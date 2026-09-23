# 语言侧缺口：`exit` 与 `print`（预注册）

2026-09-21 立项。仿前几份预注册：**先把验收判据写死，再改代码**。

## 为什么立项

Step 3 写 `wd`（Whisper 自己写的驱动）时撞到两个**语言本身的缺口**——它们不是
`wd` 的实现问题，换任何程序都会撞上：

1. **C 链程序没有退出码**。`cgen` 固定发射 `return 0;`，Whisper 层没有
   `exit`/`quit` 一类的词。于是 `wd` 的错误路径（源文件不存在、C 编译器失败）
   **无法让 shell 看到非零退出码**——一个不能报失败的 CLI 工具不是工具。
   实测：`wd build nope.ws out` 打印了 `cannot read source` 却仍然 `exit=0`。
2. **没有裸打印词**。打印只有 `.`（opcode 144 `OutputTop`），它自带换行且
   对字符串的引号处理在两链不同；而 `striter`（opcode 186）不是打印词，是
   字符串**迭代器**（推 `[字符码, 余串]`）。于是 `wd run` 转发子进程输出时
   只能靠 `.`，末尾多一个换行——**做不到逐字节转发**。

两者都是「原语缺失」，不是「实现 bug」。

## 设计决定（提前写明，避免事后合理化）

**走正规 opcode 路线，不借 `@n` 能力 ABI。**

- `@n` 是**能力令牌**，语义是"这个副作用需要显式授权"。而"退出进程"和
  "打印一个字符串"不是需要授权的事——把它们塞进能力 ABI 会让每个用到的
  程序都要 `--allow-...`，那是把设计错误固化下来。
- 代价是明确且机械的：新增一个 opcode 要动一串位置（清单见下）。本轮接受。

**词形**：`print`（裸打印字符串，不补换行）与 `exit`（取栈顶 i64 作为
退出码并终止；栈空或非整数则报错）。

**opcode 字节**：`0xC8` = `Print`、`0xC9` = `Exit`（0xC0-0xC7 已占：
BytesWriteFile / Try / MapNew..MapHas；0xCE = BytesToHex）。

**两链语义**：

| | VM | C 链 |
|---|---|---|
| `print` | 取栈顶字符串，写 stdout，**不加换行** | `fputs` + `fflush` |
| `exit` | 取栈顶 i64，以该码 `std::process::exit` | `exit((int)code)` |

## 判据（全部为机器可验的断言）

### E1 —— `print` 逐字节转发

对同一程序，VM 与 `-t c2` 的 stdout **逐字节相同**（不是"逐行"），且内容
**不因 `.` 的换行/引号规则被改写**。用例：

| 用例 | 源码 | 期望 stdout（字节） |
|------|------|--------------------|
| P-1 | `"hi" print` | `hi`（**无**换行） |
| P-2 | `"a" print "b" print` | `ab` |
| P-3 | `"x\n" print` | `x\n`（保留自带换行） |
| P-4 | `"" print` | （空） |

### E2 —— `exit` 给出真实退出码

| 用例 | 源码 | 期望 shell 退出码 |
|------|------|------------------|
| X-1 | `0 exit` | 0 |
| X-2 | `3 exit` | 3 |
| X-3 | `: f { 7 exit } ; f 1 .` | 7 且**不**打印 `1`（真终止） |

VM 与 `-t c2` 产物**退出码相同**。

### E3 —— 既有语义零回归

- `cargo test --workspace` ≥ 480 / 0；
- clippy / fmt 0；
- syntax / stdlib / typecheck / coverage / net / app 全绿不降；
- `opcode.rs` 的 `to_byte_is_injective`（自动覆盖新 opcode）通过；
- **固定点 gen2 == gen3**（本项必过：改 `compiler.all.ws`）。

### E4 —— `wd` 真正用上它们

- `wd build <不存在的文件> <out>` 的 shell 退出码 **非 0**；
- `wd run` 转发的输出与直接运行产物**逐字节相同**（不再多一个换行）。

## 反向判据（什么算失败，提前写明）

- 为省事把 `exit`/`print` 做成 `@n` 能力（要授权才能退出/打印） → **失败**：
  那是把设计错误固化。
- 为让 E2 通过而让 `exit` 只是"提前 return 到 main"（不终止） → **失败**：
  X-3 专门钉这一点。
- 改 `compiler.all.ws` 后固定点不成立且本战役内无法修复 → 回滚，如实登记。

## 触点清单（新增一个 opcode 要动的地方，实测清点）

Rust 侧（10）：
1. `crates/whisper-parser/src/ast.rs` —— `Operator` 变体
2. `crates/whisper-parser/src/parser.rs` —— 词形 → `Operator`
3. `crates/whisper-codegen/src/bytecode_gen.rs` —— `Operator` → `Opcode`
4. `crates/whisper-codegen/src/formatter.rs` —— 反查词形（`fmt` 用）
5. `crates/whisper-codegen/src/stack_effect.rs` —— (入, 出) 元数
6. `crates/whisper-codegen/src/wbin.rs` —— 字节 → `Opcode`（解码）
7. `crates/whisper-cli/src/commands/bootstrap.rs` —— `Operator` → 字节（编码）
   与 字节 → `Opcode`（解码）**两处**
8. `crates/whisper-core/src/opcode.rs` —— 变体、字节、名字、`ALL`（单射测试用）
9. `crates/whisper-core/src/vm.rs` —— 执行
10. `crates/whisper-codegen/src/cgen/transpiler.rs` —— `-t c`（Rust 转译器）后端
11. `crates/whisper-typecheck/src/checker.rs` —— 类型规则

自举链侧（2）：
12. `whisperc/compiler.all.ws` —— 词表 + `emit-bare` 分支
13. `whisperc/runtime.c` —— C 实现

## 执行记录

（实施后回填）
