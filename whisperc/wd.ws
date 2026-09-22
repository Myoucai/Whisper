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

// ── 限定名克隆（里程碑 B）────────────────────────────────────────────
// 把模块里每个**单行**定义 `: name { … } ;` 再发一行 `: ns.name { … } ;`，
// 使 `std.math.sq` 这类限定引用在文本内联路径上也能解析（对齐 c2.rs 的
// append_qualified_clones）。多行定义不克隆——见文件末尾的已知缺口。
//
// 用「索引状态机」而不是 @fold：本文件其余循环也是这个形状，栈纪律只需
// 一条——本地状态永远只有一件，读它一律 `dup N @nth`。
: wd-clones {                          // [src ns] → text
    "wd.ns" swap kv-set                // ns 存 KV，本函数只折叠输出串
    "\n" strsplit                      // lines
    "" 0 swap                          // lines 0 ""
    3 PushList                         // [lines, 0, ""]
    wd-clone-loop
} ;
: wd-clone-loop {                      // [state] → text
    dup 0 @nth len over 1 @nth swap >= ??
        2 @nth return ]
    dup 0 @nth over 1 @nth @nth        // state line
    wd-clone-piece                     // state piece
    over 2 @nth swap strcat            // state out'
    swap dup 0 @nth swap 1 @nth 1 + rot 3 PushList
    wd-clone-loop
} ;
: wd-clone-piece {                     // [line] → 克隆行（不是单行定义则 ""）
    dup wd-def-name                    // line name
    dup "" str-eq? ??
        drop drop "" return ]          // 不是定义行
    dup "." wd-contains? ??
        drop drop "" return ]          // 已带限定名
    over wd-code wd-last 59 = ! ??
        drop drop "" return ]          // 多行定义（未以 ';' 收尾）
    drop                               // line
    str-strip 2 9999 strslice          // rest = name + after
    "wd.ns" kv-get "." strcat swap strcat    // ns.rest
    ": " swap strcat "\n" strcat       // ": ns.rest\n"
} ;

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
        "usage: wd build|run <src.ws> <out>\n" .
        [] return ]
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

// [s] → 去掉末尾一个换行（若有）
: wd-chomp {
    dup strlen 0 = ?? return ]
    dup strlen 1 - strnth 10 = ??
        dup strlen 1 - 0 swap strslice return ]
} ;

// [out] → 实际产物路径，先试原名再试 +".exe"（不同 gcc 落盘策略不同），
// 都没有则返回空串
: wd-real {
    dup file-exists ?? return ]
    dup ".exe" strcat
    dup file-exists ?? return ]
    drop ""
} ;

// [s] → 转发子进程输出。
// 注意 `striter` 是字符串**迭代器**（推 [字符码, 余串]），不是打印词；
// 打印只有 `.`，它会补一个换行。所以只转发非空输出，且文本末尾会多一个
// 换行——本轮如实登记为缺口（逐字节转发需要语言侧给一个裸打印词）。
: wd-emit { dup strlen 0 = ?? drop | . ] } ;

// ── 编译（并可跑）一个文件 ──────────────────────────────────────────
// S = [verb src out]
: wd-build {
    dup 1 @nth file-exists 0 = ??
        "wd: cannot read source: " over 1 @nth strcat "\n" strcat .
        drop return ]

    dup 1 @nth                        // S src
    read-file                         // S text
    wd-source                         // S expanded   （std/core + import 展开）
    compile-full                      // S main_bc defs
    cgen                              // S csrc
    header swap strcat                // S full_c

    over 2 @nth ".c" strcat           // S full_c cpath
    swap                              // S cpath full_c
    write-file                        // S nwritten
    drop                              // S

    dup 2 @nth                        // S out
    dup ".c" strcat                   // S out cpath
    wd-cc                             // S out cpath cc
    rot rot                           // S cc out cpath
    wd-cc-cmd                         // S cmd
    @5                                // S [status out err]

    dup 0 @nth 0 = ??
        drop                          // S           —— 编译成功
        dup 2 @nth "wd: compiled -> " swap strcat "\n" strcat .
    |   dup 2 @nth "wd: C compiler failed:\n" swap strcat .
        drop return
    ]

    dup 0 @nth "run" str-eq? ??
        // 不同 gcc 的平台策略不同（有的原样落 `-o`，有的补 .exe），探测
        // 清楚再执行；否则 popen 拿一个不存在的路径，失败信息离根因很远。
        dup 2 @nth wd-real           // S realpath
        dup strlen 0 = ??
            drop drop
            "wd: cannot find the built artifact\n" .
            return ]
        @5                            // S [status out err]
        dup 1 @nth wd-emit            // S res
        dup 2 @nth wd-emit            // S res
        drop                          // S
    | ]
    drop
} ;

: wd-main {
    wd-argv                           // S | []
    dup len 0 = ?? drop return ]
    wd-build
} ;

wd-main
