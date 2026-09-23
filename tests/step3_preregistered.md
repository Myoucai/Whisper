# Step 3 预注册判据（驱动 `wd`，零 Rust）

2026-09-21 立项。仿前几份预注册：**先把验收判据写死，再改代码**。

## 背景

设计文档 Step 3 的目标是「**gcc → whisper2 → wd → 跑任何 Whisper 程序，
全链路零 Rust**」，并判断其前置（Step 2 的 `exec` 能力）已完成——本轮复核
**成立**：`@5` 已在 C 链可用（`tests/step2b_preregistered.md` 之外另有
`opcode_coverage.sh` 的 `cap-exec-*` 三例守着）。

现状（2026-09-21 实测）：`scripts/bootstrap.sh` 已经能从冻结种子产出
`whisper2`，而 `whisper2 -c < src.ws > src.c` 也能用。**缺的是中间那层胶水**：
用户得自己把 `import` 内联、自己调 gcc、自己跑产物。`wd` 就是这层胶水。

## 分里程碑（本文件按里程碑 A 立判据）

`wd` 的工作量集中在 **import 内联**（把 `crates/whisper-cli/src/commands/c2.rs`
里 `expand_imports` + `append_qualified_clones` 那套用 Whisper 重写）。为了
不把"零 Rust 能编出可执行文件"这个可以立即验证的成果，和"内联做到与 Rust
逐字节等价"这个更大的目标捆在一起，分两步：

- **里程碑 A（本文件）**：`wd` 能零 Rust 构建，并能编译+运行**自包含**程序。
- **里程碑 B（另立）**：import 内联（含命名空间限定克隆），使 `wd` 能编
  任何用 stdlib 的程序，并与 `-t c2` 产物逐字节一致。

## 判据（里程碑 A，全部为机器可验的断言）

### A1 —— `wd` 用零 Rust 构建

`bash scripts/build-wd.sh` 只用 **gcc + 冻结种子**
（`whisperc/seed/compiler_seed.c`）产出 `target/wd/wd`。判据：
脚本里不出现 `cargo`，且把 `cargo` 从 PATH 里藏掉后仍能成功构建。

### A2 —— `wd` 能编译并运行自包含程序

对一个不含 `import` 的程序：

    wd build <src.ws> <out>     产出 <out> 与 <out>.c，退出正常
    wd run   <src.ws> <out>     同上，并把产物 stdout 原样转发

产物输出与 `whisper build -t c2` 产物**逐行一致**。

### A3 —— 产物与 `-t c2` 的 C 同源

对同一份源码，`wd` 写出的 `<out>.c` 与 `whisper build -t c2` 写出的 `.c`
**逐字节一致**（两者都走 compile-full + cgen + 同一个 header）。

这条是里程碑 A 里最强的一条：它把 `wd` 钉在"Rust 驱动器的等价替代"上，
而不是"另一个能凑合用的东西"。

### A4 —— 错误信息可定位

- 源文件不存在 → 明确说明是哪个文件，而不是空文件编译后再由 gcc 报错；
- C 编译器失败 → 把 gcc 的 stderr 原样转出。

### A5 —— 既有门禁零回归

    cargo test --workspace       ≥ 479 / 0
    cargo clippy -D warnings     0
    cargo fmt --all --check      0
    syntax / stdlib              26 / 0 · 58 / 0
    typecheck-gate               FP=0
    opcode_coverage              PASS 不降（当前 80）
    net-gate / app-gate          PASS / PASS（13 路由）

### A6 —— 固定点不受影响

`scripts/verify-fixedpoint.sh` 报 `gen2 == gen3`。本轮**不改
`compiler.all.ws`**（`wd.ws` 是并列的新文件，构建时截掉其末尾的 `driver`
调用后拼接），所以固定点必须原样成立。

## 反向判据（什么算失败，提前写明）

- 为让构建通过而把 `wd` 改成需要 cargo/rustc → **失败**（那是把目标本身
  取消）。
- 为让 A3 通过而修改 `-t c2` 的输出以迁就 `wd` → **失败**（应当反过来）。
- 把 `wd` 做成对 `compiler.all.ws` 的**不可复现的文本手术**（例如按行号切）
  → **失败**：只能按其结构（末尾那一行 `driver` 调用）切。

## 已知缺口（本轮先记录，不在里程碑 A 内）

- **import 内联**（里程碑 B）。
- **C 链程序没有退出码**：cgen 固定发射 `return 0;`，Whisper 层没有
  `exit`/`quit` 词，所以 `wd` 的错误路径无法让 shell 看到非零退出码。
  这是语言侧的一个真实缺口，另立。
- **没有裸打印词**：`striter` 是字符串迭代器而不是打印词，打印只有 `.`（自带
  换行）。所以 `wd run` 转发的子进程输出末尾会多一个换行，不是逐字节转发。
  同样需要语言侧补一个裸打印词。
- 缓存（设计文档提到按内容寻址复用 `-t native` 的模型）未做。

## 执行记录

### 逐条对照

| 判据 | 结果 | 说明 |
|------|------|------|
| **A1** 零 Rust 构建 | ✅ | `bash scripts/build-wd.sh` 只用 gcc + 冻结种子产出 `target/wd/wd`；脚本内无 `cargo` |
| **A2** 编+跑自包含程序 | ✅ | `wd build a.ws a` 产出 `a.exe`/`a.c`；`wd run a.ws b` 编译并转发产物输出（`1 2 + .` → `3`） |
| **A3** 与 `-t c2` 的 C 逐字节一致 | ✅ **已达成（收口战役 W1+W2，2026-09-21o）** | 里程碑 B 后残差为自包含 1601 vs 1604、带 import 17451 vs 17878。收口时定位到**两个独立根因**：① Rust lexer 不折叠 CRLF，`include_str!` 进来的编译器源码在工作树是 CRLF，于是 `-t c2` 的 C 里混进 3 个杂散 `\r`；② `wd` 只克隆单行定义，多行定义（如 `math.clamp`）没有限定名克隆。两者都修掉后，四个程序（自包含 / `std/math` / `std/list` / 多行限定名）**逐字节相同**。常驻断言：集成测试 `w2_wd_output_matches_c2_byte_for_byte` |
| **A4** 错误可定位 | ✅ | 源文件不存在 → `wd: cannot read source: <路径>`；C 编译器失败 → 转出 gcc 的 stderr |
| **A5** 既有门禁零回归 | ✅ | 见 CHANGELOG |
| **A6** 固定点不受影响 | ✅ | 未改 `compiler.all.ws`；`gen2 == gen3` |

### 里程碑 B（import 展开）——已落地

`wd` 现在与 `c2.rs::expand_imports` 同构：源码未提及 `std/core` 时**先补一行
`import "std/core"`**，再统一逐行展开——于是 std/core 的正文与限定名克隆都
由同一条通路产生，**没有特例分支**。递归由 `wd-expand-piece` 里的
`wd-expand` 完成，已访问集合放 KV 防环。

实测：

| 程序 | wd run |
|------|--------|
| `1 2 + .` | `3` ✓ |
| `import "std/math"` + `5 sq .` | `25` ✓ |
| `import "std/list"` + `[1 2 3] len .` | `3` ✓ |
| `import "std/math"` + `5 math.sq .`（限定名） | `25` ✓ |

**过程里定位到并修掉的两个真 bug**（这才是前一轮卡住的原因）：

1. **`over N @nth` 不是「取第 N 项」**，而是「复制 `$1`，再对这个副本取下标」。
   `wd-def-name` 之后 `over` 已经拿到了行文本，多写的 `1 @nth` 是**在对字符串
   取下标** → 拿到垃圾指针 → `type error: expected ptr`。四处同样的写法，全改。
   前一轮一直没定位到，就是因为把它当成了一个原子取值操作。
2. **`>=` 的参数顺序**：`a b >=` 算的是 `a >= b`（`a` 在栈下方）。循环终止条件
   写成 `len ... idx >=`，实际算的是 `n >= idx`——**idx=0 时立即判定完成**，
   于是展开结果恒为空串（表现为 .c 只有 185 字节）。

另外把 `@fold` 换成了本文件其余循环同款的「索引状态机」：栈纪律只剩一条——
本地状态永远一件，读它一律 `dup N @nth`。

**遗留**：多行定义不做限定名克隆（c2 会做），这是 A3 残差的主要来源之一。

### 上一轮（里程碑 A）的踩坑记录

里程碑 A 那一轮**没能**定位到 `expected ptr`——当时的排查排除了 `wd-path` 少
`/` 分隔符（已修）、`wd-suffix?` 用字符反转实现不可靠（已改为尾部切片并最终
弃用）、`kv-get` 缺键返回空串（不是它），但没找到真凶，于是把半成品撤出
`wd-build` 只留注释。本轮定位到的正是上面那两条（`over N @nth` 的误用、
`>=` 的参数顺序）。

### 里程碑 A 那一轮踩到并修掉的坑

1. **`over` 越界读调用者的栈**。这些折叠体压在调用者栈上，本地值只有一件时
   `over`（= $1）拿到的是调用者的东西，表现为 `type error: expected ptr`。
   凡「只有一件状态」的场合一律用 `dup N @nth`。
2. **`strcat(a,b)` 是 a 在前**。拼命令行时按 b 在前写，整条命令错位。
3. **`striter` 是字符串迭代器**（推 `[字符码, 余串]`），不是打印词；当输出用
   会让栈上凭空多出元素。
4. **`file-exists` 的极性**：`wd-real` 初版写成 `file-exists 0 = ?? return`，
   把"不存在"当命中，于是永远返回不存在的路径。
5. **`@5` 执行不存在的路径**只得到 popen 失败，报错离根因很远——所以执行前
   先探测产物路径（`wd-real`：原名 → `.exe` → 明确报错）。
6. **`w_kv_get` 缺键返回空串**（与 VM 对齐），不是错误；不要把空串当失败。

### 实测记录（本机，gcc = msys64 mingw 16.1.0）

    $ wd build prog/a.ws prog/a
    wd: compiled -> D:/Whisper/_dbg/wd/prog/a
    $ ./prog/a
    3
    $ wd run prog/a.ws prog/b
    wd: compiled -> D:/Whisper/_dbg/wd/prog/b
    3

