# Whisper 包标准（packages）

Whisper 的包 = **一个 git 仓库**（或本地目录）+ `package.ws` manifest + 若干
`.ws` 模块。安装到 `$WHISPER_HOME/packages/<name>/`（默认 `~/.whisper/packages/`），
消费方 `import "pkgname/module"` 直接解析。

## 安装流（端到端已验收）

```bash
whisper install github.com/user/repo        # git registry（浅克隆，curl/powershell 兜底）
whisper install ./my-pkg                    # 本地目录（包作者开发流，不下载不删源）
whisper install -l                          # 列出已装
whisper uninstall my-pkg                    # 卸载
```

安装动作：解析 spec → 下载（或本地直读）→ 解析 `package.ws` → **能力审查**
（capabilities 非空需 `-y` 或交互确认）→ 递归装依赖 → 拷贝 `.ws` 到缓存目录 →
写 `~/.whisper/whisper.lock`（含 SHA256 校验和）。

## package.ws manifest

```
# 注释行
name: "my-pkg"
version: "1.0.0"
capabilities: ["@http_post", "@file_read"]     # 需要的能力，安装时审查
exports: ["word1", "word2"]                    # 供校验（权威在模块的 export 行）
dependencies: ["std/json", "github.com/u/other@1.2.0"]
native: ["vendor/sqlite3.dll"]                 # 随包分发的原生库（ffi-open 加载）
```

## 模块约定

- 每个模块是普通 `.ws` 文件：词定义 + `export name` 行。
- **import 只取 export 的词**；模块内私有词要么 export（供同包其他模块用），
  要么不导出（包外不可见）。
- 命名空间靠模块路径：`import "my-pkg/base64"` 后用其导出词；两个模块导出
  同名词会得到编译警告（不中止）。
- **B2 栈合同必须过**：包内所有词在消费方编译时与本地词一视同仁地过栈
  效应检查——学坏的词进不了任何程序。
- **能力系统不可绕过**：包声明的能力只影响安装审查；运行时是否真的有
  HTTP/文件能力仍由 `whisper run --allow-*` 决定。纯计算包（如 encoding）
  无需任何声明。

## 样板

- 标准库模块即样板：`stdlib/encoding.ws`（纯计算包，零依赖，向量测试在
  `crates/whisper-cli/tests/integration_tests.rs::encoding_stdlib_vectors`）。
- 最小可安装包见本文档顶部流程；本地开发用 `whisper install ./dir` 迭代。

## 路线

- [ ] registry 目录页（静态 JSON over github raw）
- [ ] `whisper publish`（git tag + 校验和上链）
- [ ] 安装时对包内词跑 `--strict-types` 报告（可选购前体检）
- [ ] std/hash（md5/sha256）：Lexicon 指纹比对的拉动到位后启动
