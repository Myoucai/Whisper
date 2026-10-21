// ══════════════════════════════════════════════════════════════════════
// wd.ws — the Whisper driver (zero Rust)
//
// Appended to whisperc/compiler.all.ws with that file's trailing `driver`
// call removed, then compiled by the self-hosted chain — so compile-full /
// cgen / header are already in scope, and scripts/build-wd.sh needs nothing
// but gcc and the frozen seed.
//
//   wd build <src.ws> <out>     compile to <out>, leaving <out>.c behind
//   wd run   <src.ws> <out>     same, then run <out>
//   wd c     <src.ws> <out.c>   emit C only (no cc, no cache; for wasm/zig)
//   wd wasm  <src.ws> <out.wasm> emit C, then zig cc to wasm
//
// Exit codes: 0 success, 1 build/IO error (missing source, C compiler
// failure, artifact not produced), 2 usage error. Being able to report
// failure at all needs the `exit` primitive (opcode 148).
//
//   WHISPER_CC     C compiler to invoke        (default gcc)
//   WHISPER_ROOT   repo root holding runtime.c (default .)
//
// Stack discipline: this file is appended to the compiler, whose internals
// rely on a clean stack beneath their state lists. Everything here keeps a
// single state list on top of the caller's stack and reaches into it with
// `dup N @nth` / `over` only.
// ══════════════════════════════════════════════════════════════════════

// ── 字符串小工具 ────────────────────────────────────────────────────
// [hay needle] → needle 是 hay 的前缀？
: wd-prefix?  { strfind 0 = } ;
// [hay needle] → 是否出现
: wd-contains? { strfind 0 >= } ;
// [s] → 最后一个字符码；空串 → -1
: wd-last { dup strlen dup 0 = ?? drop drop -1 return ] 1 - strnth } ;
// [s] → 剥掉行尾 "//" 注释
: wd-code {
    dup "//" strfind dup 0 < ??
        drop return ]
    0 swap strslice
} ;

// [line] → 行首 ": name" 里的 name；不是定义行则为 ""
: wd-def-name {
    str-strip                          // t
    dup ": " wd-prefix? ??
        2 9999 strslice                // rest = name + after
        dup " " strfind dup 0 < ??
            drop return ]              // 名字后没有空格 → 整段就是名字
        0 swap strslice return ]
    drop ""
} ;

// ── 限定名克隆（里程碑 B / 收口 W2）─────────────────────────────────
// 把模块里每个定义 `: name { … } ;` 再发一份 `: ns.name { … } ;`，使
// `std.math.sq` 这类限定引用在文本内联路径上也能解析（对齐 c2.rs 的
// append_qualified_clones）。
//
// **多行定义必须连体一起克隆**：只发首行会得到一个没有体的定义。这正是
// A3 残差的来源——带 import 的程序 wd 17451B vs c2 17875B，少的就是
// `math.clamp` 这类多行定义的克隆（连带 `W_math_clamp` 整个函数）。
//
// 栈纪律：在调用者栈之上**只保留一个状态列表** [lines, idx, out, copying]，
// 临时值一律走 KV。这不是洁癖——这个文件历史上两次栈事故都出在"同时有两个
// 本地量"，而 `over`/`@nth` 一旦叠起来就会读到错误的槽。
: wd-clones {                          // [src ns] → text
    "wd.ns" swap kv-set                // 模块名进 KV，循环里只读它
    "\n" strsplit                      // lines
    0 "" 0                             // idx out copying —— 槽序必须与
    4 PushList                         // [lines, idx, out, copying] 一致
    wd-clone-loop
} ;

// bool → "1"/"0"。KV 的键与值都按 `const char*` 走（cgen 把两边都
// `w_as_ptr` 之后调用 w_kv_set/w_kv_get），所以**存不了布尔**——存进去会
// 变成 "expected ptr"。标志位一律走字符串。
: wd-flag { ?? "1" | "0" ] } ;

: wd-clone-loop {                      // [state] → text
    dup 0 @nth len over 1 @nth swap >= ??
        2 @nth return ]
    dup 3 @nth ??
        wd-clone-body-line             // 正在拷多行定义的体
        wd-clone-loop return ]
    wd-clone-next
    wd-clone-loop
} ;

// 体的一行：原样追加，并按本行是否闭合决定是否退出拷贝模式
: wd-clone-body-line {                 // [state] → state'
    dup 0 @nth over 1 @nth @nth        // state line
    "wd.l" swap kv-set                 // state
    dup 2 @nth "wd.l" kv-get strcat "\n" strcat   // state out'
    "wd.o" swap kv-set                 // state
    "wd.l" kv-get wd-closes? ! wd-flag // state flag'（还闭合不了就继续拷）
    "wd.c" swap kv-set                 // state
    dup 0 @nth                         // state lines
    over 1 @nth 1 +                    // state lines idx+1
    "wd.o" kv-get                      // state lines idx+1 out'
    "wd.c" kv-get "0" str-eq? !        // state lines idx+1 out' copying'
    4 PushList swap drop
} ;

// 非续行：本行若是可克隆的定义首行就发首行，并可能进入拷贝模式
: wd-clone-next {                      // [state] → state'
    dup 0 @nth over 1 @nth @nth        // state line
    wd-clone-of                        // state piece copying
    wd-flag "wd.c" swap kv-set         // state piece
    "wd.p" swap kv-set                 // state
    dup 2 @nth "wd.p" kv-get strcat    // state out'
    "wd.o" swap kv-set                 // state
    dup 0 @nth                         // state lines
    over 1 @nth 1 +                    // state lines idx+1
    "wd.o" kv-get                      // state lines idx+1 out'
    "wd.c" kv-get "0" str-eq? !        // state lines idx+1 out' copying'
    4 PushList swap drop
} ;

// [line] → 克隆首行（含换行）与"是否需要继续拷贝体"
: wd-clone-of {
    dup wd-def-name                    // line name
    dup "" str-eq? ??
        drop drop "" 0 return ]        // 不是定义行
    dup "." wd-contains? ??
        drop drop "" 0 return ]        // 已带限定名
    drop                               // line
    dup wd-closes?                     // line closed?
    over                               // line closed? line
    str-strip 2 9999 strslice          // line closed? rest
    // 闭合判定必须在**本行还在栈上**时做。曾经把它排到 drop/切片之后，
    // 于是 `over` 读到的是调用者的栈（裁到的是 state 列表），strfind 拿
    // 列表当字符串 → "expected ptr"。同一个坑这个文件栽过两次。
    rot                                // closed? rest line
    drop                               // closed? rest
    "wd.ns" kv-get "." strcat swap strcat   // closed? ns.rest
    ": " swap strcat                   // closed? header
    swap                               // header closed?  ← ?? 弹的是栈顶
    ??                                 // header
        "\n" strcat 0 return ]         // 单行定义：一行就完
    "\n" strcat 1                      // 多行定义：进拷贝模式
} ;

// [s] → 剥掉行尾注释与空白后，是否以 ';' 收尾（= 定义已闭合）
// 与 c2.rs 的 code_only(after).ends_with(";") 同判定。
: wd-closes? { wd-code str-strip wd-last 59 = } ;

// ── import 展开（里程碑 B）─────────────────────────────────────────
// [line] → 这一行里的模块名；不是 import 行则为 ""
: wd-import-spec {
    str-strip
    dup "import " wd-prefix? ! ??
        drop "" return ]
    7 9999 strslice                    // rest
    dup "\"" strfind dup 0 < ??
        drop drop "" return ]
    1 + 9999 strslice                  // after first quote
    dup "\"" strfind dup 0 < ??
        drop drop "" return ]
    0 swap strslice
} ;

// [line] → import 之后（引号之外）的尾巴，已 trim；不是标准形态则 ""
: wd-import-tail {
    str-strip
    dup "import \"" wd-prefix? ! ?? drop "" return ]
    8 9999 strslice                    // 跳过 `import "`
    dup "\"" strfind dup 0 < ??
        drop drop "" return ]
    1 + 9999 strslice str-strip
} ;

// 已访问集合放 KV（一次性驱动进程，无需可重入）
: wd-seen { "wd.seen" kv-get } ;
: wd-seen? { "\n" strcat "\n" strcat wd-seen swap wd-contains? } ;
: wd-seen-add { "\n" strcat "\n" strcat wd-seen swap strcat "wd.seen" swap kv-set } ;

// 尾巴 + 换行；尾巴为空则空串
: wd-tail-nl { "wd.tail" kv-get dup strlen 0 = ?? drop "" return ] "\n" strcat } ;

// [line] → 这一行展开成什么文本
: wd-expand-piece {
    dup wd-import-spec                 // line spec
    dup "" str-eq? ??
        drop "\n" strcat return ]      // 非 import 行：原样 + 换行
    // 尾巴先存 KV，省得后面在栈上搬
    over wd-import-tail "wd.tail" swap kv-set
    dup wd-ns                          // line spec ns
    over wd-path                       // line spec ns path
    dup wd-seen? ??
        drop drop drop drop            // 已展开过：只留尾巴
        wd-tail-nl return ]
    dup wd-seen-add                    // line spec ns path
    dup read-file                      // line spec ns path sub1
    rot                                // line spec path sub1 ns
    wd-clones                          // line spec path clones
    over read-file                     // line spec path clones sub2
    wd-expand                          // line spec path clones body
    swap strcat                        // line spec path (body ++ clones)
    wd-tail-nl                         // ... tail'
    strcat                             // ... (body ++ clones ++ tail')
    rot drop rot drop swap drop        // 只留结果
} ;

// [src] → 展开后的源码：逐行把 import 换成「模块全文 + 限定名克隆」。
// 与 c2.rs::expand_imports 同构；递归由 wd-expand-piece 里的 wd-expand 完成。
: wd-expand {
    "\n" strsplit                      // lines
    "" 0 swap                          // lines 0 ""
    3 PushList                         // [lines, 0, ""]
    wd-exp-loop
} ;
: wd-exp-loop {                        // [state] → text
    dup 0 @nth len over 1 @nth swap >= ??
        2 @nth return ]
    dup 0 @nth over 1 @nth @nth        // state line
    wd-expand-piece                    // state piece
    over 2 @nth swap strcat            // state out'
    swap dup 0 @nth swap 1 @nth 1 + rot 3 PushList
    wd-exp-loop
} ;

// [src] → 完整源码：与 c2.rs 一致，未提及 std/core 时先前置一行
// `import "std/core"`，再统一走展开——于是 std/core 的正文与限定名克隆都由
// 上面同一条通路产生，没有特例分支。
: wd-source {
    "wd.seen" "" swap kv-set           // 清空已访问集合
    dup "std/core" wd-contains? ??
        wd-expand return ]
    "import \"std/core\"\n" swap strcat
    wd-expand
} ;

// [spec] → 命名空间（最后一段）："std/math" → "math"
// （import 的写法都不带 .ws——wd-path 才补后缀，所以这里不必剥）
: wd-ns { "/" strsplit dup len 1 - @nth } ;

// ── 路径解析 ────────────────────────────────────────────────────────
// 与 c2.rs 的 resolve_module 对齐：std/X → <root>/stdlib/X.ws，
// 其它 → <root>/<spec>.ws。（"相对引用它的文件"那一层见文件末尾的已知缺口）
: wd-path {                            // [spec] → path
    dup "std/" wd-prefix? ??
        dup 4 9999 strslice            // spec tail
        swap drop                      // tail
        wd-root "/stdlib/" strcat swap strcat
        ".ws" strcat return ]
    wd-root "/" strcat swap strcat ".ws" strcat
} ;

// ── import 展开（里程碑 B，尚未接入）────────────────────────────────
// 下面这组词是本轮做到一半的部分：限定名克隆的折叠在某个环节上还会
// 抛 "type error: expected ptr"，尚未定位；在查清之前**不接进 wd-build**，
// 以免把能用的部分一起带坏。判据见 tests/step3_preregistered.md 的里程碑 B。
//
// 已知已修的两处：`wd-path` 少过 "/" 分隔符（"mmod" → ".mmod.ws"）；
// `wd-suffix?` 用字符反转实现不可靠，改用尾部切片——但最终都没用上，
// 因为 `;` 判定换成了单字符检查 `wd-last 59 =`。

// ── env helpers ─────────────────────────────────────────────────────
// [name default] → value（未设置或空串则取 default）
: wd-env {
    over getenv                       // name default val
    dup strlen 0 = ??
        drop swap drop return ]
    rot drop swap drop
} ;

: wd-cc   { "WHISPER_CC"   "gcc" wd-env } ;
: wd-root { "WHISPER_ROOT" "."   wd-env } ;
: wd-runtime-path { wd-root "/whisperc/runtime.c" strcat } ;

// ── argv ────────────────────────────────────────────────────────────
// args → [verb src out]，参数不足时打印用法并返回空列表
//
// 读法：每次 `dup N @nth` 之后 `swap` 把 argv 换回栈顶——取出的值沉到它
// 下面，argv 因此始终可再取，最后 drop 掉即得到按读取顺序排好的三项。
// （不能用 `over`：栈上只有一件本地值时 `over` 越界读调用者的东西。）
: wd-argv {
    args                              // A
    dup len 4 < ??
        drop
        "usage: wd build|run|c|wasm <src.ws> <out>" .
        2 exit ]
    dup 1 @nth swap                   // verb A
    dup 2 @nth swap                   // verb src A
    dup 3 @nth swap                   // verb src out A
    drop                              // verb src out
    3 PushList                        // S
} ;

// ── C 编译器命令行 ──────────────────────────────────────────────────
// [cc out cpath] → command
// 拼装全程让累加器停在栈顶：`acc X strcat` = acc ++ X（strcat(a,b) 拼 a 在前）。
: wd-cc-cmd {
    rot                               // out cpath cc
    " -O2 -w -o " strcat              // out cpath acc
    rot                               // cpath acc out
    strcat                            // cpath acc'
    " " strcat                        // cpath acc''
    swap strcat                       // acc'''
    " -I " strcat
    wd-root strcat "/whisperc" strcat
    " -lm" strcat
    "OS" "" wd-env "Windows_NT" str-eq? ?? " -lws2_32" strcat ]
} ;

// [cpath out.wasm] → zig wasm 命令行。直接调 zig（不经过 bash）。
: wd-wasm-cmd {
    "wd.wasm.out" swap kv-set
    "wd.wasm.cpath" swap kv-set
    "zig cc -target wasm32-wasi -O2 -mno-simd128 "
    "-D_WASI_EMULATED_MMAN -D_WASI_EMULATED_SIGNAL -I " strcat
    wd-root "/whisperc" strcat wd-q strcat
    " " strcat
    "wd.wasm.cpath" kv-get wd-q strcat
    " -o " strcat
    "wd.wasm.out" kv-get wd-q strcat
    " -lwasi-emulated-mman -lwasi-emulated-signal" strcat
} ;

// [s] → 去掉末尾一个换行（若有）
: wd-chomp {
    dup strlen 0 = ?? return ]
    dup strlen 1 - strnth 10 = ??
        dup strlen 1 - 0 swap strslice return ]
} ;

// [out] → 实际产物路径，先试原名再试 +".exe"（不同 gcc 落盘策略不同），
// 都没有则返回空串。
//
// ⚠️ 必须**恰好返回一个值**。原写法在"要试 .exe"的两条路径上都会把原名留在
// 栈上（`dup ".exe" strcat` 之后是 [原名, 原名.exe]，`??` 的 true 分支直接
// `return`、false 分支只 `drop` 一个），于是返回两个值。旧调用处只按**栈顶**
// 判断、不按深度取槽，所以这个泄漏一直没暴露；一旦调用方用 `dup 2 @nth` 取
// 自己的槽就会取错对象（收口 ③ 接线时撞上：`type error: expected ptr`）。
: wd-real {
    dup file-exists ?? return ]
    dup ".exe" strcat
    dup file-exists ?? swap drop return ]
    drop drop ""
} ;

// [s] → 转发子进程输出。用 `print`（opcode 147）：它把字符串**逐字节**写
// stdout，不补换行、不改写引号。曾经这里只能用 `.`，而 `.` 会补一个换行、
// 两链对字符串的引号处理还不一致——`wd run` 因此做不到逐字节转发。
//
// 取输出用 `@10`（exec-raw）而不是 `@5`：`@5` 按设计对两个流做 trim，子进程
// 末尾的换行在 wd 看到它之前就没了。`@10` 是同一个能力、同一份授权
// （CLI 绑 @5 时一起绑 @10），只是两个流原样返回。
: wd-emit { dup strlen 0 = ?? drop | print ] } ;

// ── 内容寻址缓存（收口 ③）───────────────────────────────────────────
// 键 = hash(生成的 C ++ runtime.c 内容 ++ cc 名)。不把"编译器版本"放进键：
// 同一份 C 经同一个 cc 出来就是同一个二进制；把编译器身份放进键反而多一条
// 忘 bump 就吐旧产物的脆弱不变式。runtime.c 必须进键，因为它以
// `#include "runtime.c"` 的形式被引用，并不在 C 文本里。
//
// 哈希是多项式 64 位（i64 自然回绕），结果拼上原文长度。不是密码学哈希：
// 输入只有本机构建产物，非对抗。长度入键让"内容不同但哈希撞车"先分道。
: wd-hash {                            // [s] → "哈希-长度"
    dup strlen swap                                   // len s
    dup strchars 0 { swap 1000003 * swap + } @fold    // len s h
    swap drop                                         // len h
    i64tostr swap i64tostr "-" swap strcat strcat     // "h-len"
} ;

// [s] → shell 双引号形式
: wd-q { "\"" swap strcat "\"" strcat } ;

// [out] → 本次平台的实际产物名：Windows 补 .exe，其它平台原样。
// 命中缓存时按它落盘，后续 wd-real / @10 才找得到。
: wd-out-target {
    "OS" "" wd-env "Windows_NT" str-eq? ?? ".exe" strcat ]
} ;

: wd-cache-dir  { wd-root "/target/whisper-cache" strcat } ;
: wd-cache-path { wd-cache-dir swap "/" swap strcat strcat } ;  // [key] → dir/key

// [src dst] → shell 真实 status。Windows 的 copy 不认正斜杠（会当开关），
// 而 wd 内部路径全是 D:/…，所以两边都先换成反斜杠再交给 cmd。
: wd-cp {
    swap
    "OS" "" wd-env "Windows_NT" str-eq? ?? "/" "\\" strreplace ]
    swap
    "OS" "" wd-env "Windows_NT" str-eq? ?? "/" "\\" strreplace ]
    swap
    wd-q
    "OS" "" wd-env "Windows_NT" str-eq? ?? "copy /Y " | "cp " ]
    swap strcat " " strcat
    swap wd-q strcat
    "OS" "" wd-env "Windows_NT" str-eq? ?? " > NUL" strcat ]
    @10 dup 0 @nth swap drop
} ;

// [S] → S hit?：命中则把缓存条目拷到 <out>[.exe]，返回真；否则只返回假。
// `wd-cp` 失败（例如缓存条目损坏）也按未命中处理，交给下面的正常编译兜底。
: wd-hit? {
    "wd.key" kv-get wd-cache-path       // S cachepath
    dup file-exists ??
        over 2 @nth wd-out-target       // S cachepath target
        wd-cp                           // S status
        0 =
    |   drop 0 ]
} ;

// [S] → S：把本次编译产物回填缓存。缓存失败不影响构建成功。
: wd-store {
    wd-root "/target" strcat mkdir drop
    wd-cache-dir mkdir drop
    dup 2 @nth wd-real                  // S realout
    dup strlen 0 = ??
        drop
    |   "wd.key" kv-get wd-cache-path   // S realout cachepath
        wd-cp                           // S status  [src=realout, dst=cachepath]
        drop
    ]
} ;

// [S] → S [status out err]：调 C 编译器，命令拼装与原来完全一致。
: wd-cc-run {
    dup 2 @nth dup ".c" strcat          // S out cpath
    wd-cc                               // S out cpath cc
    rot rot                             // S cc out cpath
    wd-cc-cmd                           // S cmd
    @5                                  // S [status out err]
} ;

// ── wasm：C → zig cc → .wasm ────────────────────────────────────────
// S = [verb src out.wasm]
: wd-wasm {
    dup 1 @nth file-exists 0 = ??
        "wd: cannot read source: " over 1 @nth strcat .
        1 exit ]

    dup 1 @nth read-file wd-source compile-full cgen header swap strcat
    over 2 @nth ".c" strcat           // S full_c cpath
    swap write-file drop              // S
    dup 2 @nth ".c" strcat            // S cpath
    over 2 @nth                       // S cpath out
    wd-wasm-cmd                       // S cmd
    @5                                // S [status out err]
    dup 0 @nth 0 = ??
        drop
        dup 2 @nth "wd: wasm -> " swap strcat .
        drop
        return
    |   dup 2 @nth "wd: wasm compiler failed: " swap strcat .
        1 exit
    ]
} ;

// ── 编译（并可跑）一个文件 ──────────────────────────────────────────
// S = [verb src out]
: wd-build {
    dup 1 @nth file-exists 0 = ??
        "wd: cannot read source: " over 1 @nth strcat .
        1 exit ]

    dup 1 @nth                        // S src
    read-file                         // S text
    wd-source                         // S expanded   （std/core + import 展开）
    compile-full                      // S main_bc defs
    cgen                              // S csrc
    header swap strcat                // S full_c

    // `wd c <src> <out.c>`：只生成 C，不调 cc、不碰缓存。
    over 0 @nth "c" str-eq? ??
        over 2 @nth swap write-file drop
        drop
        return ]

    // 缓存键：full_c ++ runtime.c ++ cc。full_c 留在栈上供写 .c。
    dup                               // S full_c full_c
    wd-runtime-path read-file         // S full_c full_c runtime
    strcat                            // S full_c full_c+runtime
    wd-cc strcat                      // S full_c material
    wd-hash                           // S full_c key
    "wd.key" swap kv-set              // S full_c

    over 2 @nth ".c" strcat           // S full_c cpath
    swap                              // S cpath full_c
    write-file                        // S nwritten
    drop                              // S

    wd-hit? ??
        // 命中：wd-hit? 已把条目拷到 <out>[.exe]；.c 仍留在磁盘上
        dup 2 @nth "wd: compiled -> " swap strcat .
    |   wd-cc-run                     // S [status out err]
        dup 0 @nth 0 = ??
            drop                      // S           —— 编译成功
            dup 2 @nth "wd: compiled -> " swap strcat .
            wd-store
        |   dup 2 @nth "wd: C compiler failed: " swap strcat .
            1 exit
        ]
    ]

    dup 0 @nth "run" str-eq? ??
        // 不同 gcc 的平台策略不同（有的原样落 `-o`，有的补 .exe），探测
        // 清楚再执行；否则 popen 拿一个不存在的路径，失败信息离根因很远。
        dup 2 @nth wd-real           // S realpath
        dup strlen 0 = ??
            drop drop
            "wd: cannot find the built artifact" .
            1 exit ]
        @10                           // S [status out err]（原样，供逐字节转发）
        dup 1 @nth wd-emit            // S res
        dup 2 @nth wd-emit            // S res
        drop                          // S
    | ]
    drop
} ;

: wd-main {
    wd-argv                           // S | []
    dup len 0 = ?? drop return ]
    dup 0 @nth "wasm" str-eq? ??
        wd-wasm
    |   wd-build ]
} ;

wd-main
