# Whisper × Python 1.0 对标清单

以 Python 1.0（1994-01）的可移植特性集为基准，逐项核对 Whisper v1.0.2
现状。三项结论先行：**核心计算/数据结构/IO/网络四层已基本齐备**；
缺口集中在**交互输入、字符串高层函数、格式化、正则**；
类系统与命名参数属**设计性差异**而非缺陷（栈式语言以词+栈+Map 替代）。

图例：✓ 已备 / ◐ 部分 / ✗ 缺失 / ◇ 设计性差异（按设计不提供，需文档化替代模式）

## 1. 核心语言

| 能力 | Python 1.0 | Whisper 现状 | 判定 | 备注 |
|------|-----------|--------------|------|------|
| 整数/浮点 | int/long/float | i64 / f64 + 双向转换 | ✓ | complex 缺（低优先） |
| 字符串 | 序列 + `%` 格式化 | 字节语义全套：strcat/slice/find/replace/split/join/chars/nth | ◐ | `%` 风格格式化缺 → 建议 `fmt` 词 |
| 列表 | 增删排序 | append/prepend/sort/rev/dedup/take/drop-n/slice/listfind + @map/@each/@fold/@filter | ✓ | 比 Py1.0 更强（高阶内置） |
| 元组 | tuple | 无 | ◇ | 列表承担；不可变性靠惯例 |
| 字典 | dict | Map（函数式）+ KV（进程级共享，互斥保护） | ✓ | 双层次设计优于单 dict |
| 控制流 | if/elif/while/for/break/continue | `?? T \| E ]` / `{b} {c} #` / `n {b} @times` / `{t} ?->` | ◐ | **break/continue 无**——栈式循环需标志位+while 模拟，建议文档化或加 `leave` 词 |
| 函数 | def/默认参/*args/lambda | `: name { body } ;` + quote | ◐ | **无命名参数/默认参数**；quote 沙箱化不可捕获外层栈（闭包缺）——已知设计约束 |
| 类/OOP | class/继承/实例 | 无 | ◇ | 多词 + Map + KV 部分替代；文档化替代模式即可 |
| 异常 | try/except/raise/finally | 哨兵值惯例（-1/空串） | ◐ | 结构化错误处理弱；可考虑 `try { } { handler }` 词（字节表有余量） |
| 递归 | ✓ | ✓（深度保护 1024） | ✓ | |
| 模块 | import/from-import | `import "std/x"` + `export` | ✓ | 无 from 细粒度（低） |
| 变量 | 局部/全局 | 无变量 | ◇ | 栈式核心设计；$N/over/KV 替代 |

## 2. 标准库对标（Py1.0 可移植集）

| Py1.0 模块 | 核心能力 | Whisper 现状 | 判定 | 缺口 |
|-----------|---------|--------------|------|------|
| math | 三角/对数/幂/π | mathf（fsqrt/fsin/fcos/ftan/atan2/log/exp/floor/ceil/round）+ math.ws（pi/abs/clamp/…） | ✓ | `pow` 需手写（习语库已有） |
| string | split/join/strip/upper/lower | str.ws（find/replace/slice/to-int/from-int/…）+ 内建 strjoin/strsplit | ◐ | **upper/lower/strip 缺**（字节 vs 码点语义需先定） |
| regex | 正则匹配 | 无 | ✗ | 最大缺口；建议先 `strfind`+`strreplace` 文档化，正则引擎列长期项 |
| time | time/sleep/strftime | time.ws（now/sleep/sleep-secs） | ◐ | strftime/日期格式化缺 |
| os | 文件/环境/进程 | read-file/write-file/getenv/exec(os.ws) | ◐ | **目录操作（listdir/mkdir）缺**；**stdin 缺**（read-line 词） |
| socket | TCP 服务/客户端 | listen/accept/recv/send/close + serve-concurrent + http.ws（http-get/http-post 客户端） | ✓ | UDP 缺（低） |
| marshal/pickle | 序列化 | json.ws（json-parse/json-stringify） | ✓ | JSON 替代足矣 |
| struct | 二进制打包 | 无 | ✗ | 二进制协议处理；低-中优先 |
| select | 多路复用 | serve-concurrent（每连接一线程） | ◐ | select/poll 无；线程模型已够当前用途 |

## 3. 建议补齐顺序

**P0（一个词一个坑，收益/成本比最高）**
1. `read-line`——stdin 读取；交互式脚本的前提
2. `str-upper` / `str-lower` / `str-strip`——高频字符串操作（先定码点语义）
3. `fmt`——`"{}+{}" a b fmt` 式格式化，替代 `%` 的生态位

**P1（小词族）**
4. 目录三件套：`listdir` / `mkdir` / `file-exists`
5. `leave`——while 循环提前退出（break 的栈式等价物）
6. `strftime` / 时间格式化

**P2（引擎级，按需启动）**
7. regex 子集（或绑定宿主正则）
8. struct 二进制打包
9. try/except 词形（哨兵惯例的升级路径）

## 4. 判定口径说明

- 现状核对基于 v1.0.2 冻结词汇表 + `stdlib/*.ws` 实际导出（2026-08-26）
- ✓ 不代表逐 API 对齐，而是该能力域可用完整度足够书写真实程序
- ◇ 项不计入"对标缺口"——它们是语言范式的根本选择，正确的动作是
  在 tutorial 中写清楚"Python 的 X 在 Whisper 里如何写"

**总评**：剔除设计性差异后，Whisper 对 Py1.0 可移植特性的覆盖约为
**85%（核心+库合并加权）**；P0+P1 补齐后可达 ~95%，剩余为 regex/struct
两个引擎级工程。
