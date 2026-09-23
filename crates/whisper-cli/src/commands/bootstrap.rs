//! whisper bootstrap — 用**权威自举编译器**（`whisperc/compiler.all.ws`）
//! 编译一个源文件，把生成的 C 打到 stdout。
//!
//!     whisper bootstrap prog.ws > prog.c
//!
//! 语义与 `whisper build -t c2` 的前半段完全相同：import 内联 →
//! `compile-full` → `cgen` → 头部。契约由集成测试
//! `bootstrap_matches_c2_byte_for_byte` 钉住（逐字节比较，不只看退出码）。
//!
//! ## 为什么这里只剩一个入口（2026-09-21 收口战役 D-1）
//!
//! 这里原先挂着**第二棵编译器**（顶层 `whisperc/lexer.ws`、`classify.ws`、
//! `parser.ws`、`main.ws` 与 `src/compile.ws`），与 `compiler.all.ws` 各写
//! 一遍同一条流水线，由本命令的 `soft`/`hard` 两种模式分别驱动。实测两者
//! 都只是**部分可用**（基础算术、单行定义、引号、跨行字符串可以；`@map`
//! 融合、import 展开、`print`/`exit`、CRLF 归一化都不行），而且它与权威
//! 编译器之间的漂移**没有任何机制把关**——`test_pipeline_compare` 只比对
//! 一个表达式的结果，对手还是 Rust 前端。
//!
//! 收口方式不是"给两棵树加一句注释说它们会漂移"，而是让本命令直接委托给权威
//! 流水线，然后删掉那棵树：**只有一份编译器源码，漂移面归零**，而命令本身
//! 从"部分可用"变成"和 `-t c2` 逐字节一致"。

/// `c2.rs` 的权威流水线在 Rust VM 上跑 `compiler.all.ws` 时需要这个宿主词：
/// `compiler.all.ws` 里有 `PushList` 的调用点，而它不是内建 opcode 的拼写。
/// **这是活代码**，与下面被删掉的模块化流水线无关。
pub(crate) fn register_host_helpers(vm: &mut whisper_core::vm::Vm) {
    vm.define_word(
        "PushList".into(),
        vec![whisper_core::opcode::Opcode::PushList],
    );
}

/// 编译 `source` 并返回完整 C 文本（含头部）。`base_dir` 是 import 的解析基准。
///
/// 与 `build_c2` 走**同一段**展开逻辑（`c2::expand_with_core`）——曾经这里
/// 自己写了一遍，结果少了 `std/core` 的自动前置，生成的 C 比 `-t c2` 少
/// 1300 余字节。调用同一段代码是唯一能保证不漂移的写法。
pub fn compile_to_c(source: &str, base_dir: &std::path::Path) -> Result<String, String> {
    let expanded = crate::commands::c2::expand_with_core(source, base_dir)?;
    crate::commands::c2::compile_via_whisperc_vm(&expanded)
}

/// CLI 入口：把生成的 C 打到 stdout。
pub fn bootstrap_compile(source: &str, base_dir: &std::path::Path) -> Result<(), String> {
    let c = compile_to_c(source, base_dir)?;
    print!("{c}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `bootstrap` 与 `-t c2` 必须调用**同一段**展开逻辑。这曾经是漂移的
    /// 入口：这里自己写了一遍展开，漏掉 `std/core` 自动前置，生成的 C 比
    /// `-t c2` 少 1300 余字节（而退出码一样，所以只看退出码的检查发现不了）。
    ///
    /// 真正逐字节的端到端对比在集成测试
    /// `bootstrap_matches_c2_byte_for_byte`（跑真 CLI、比真文件）——单元测试
    /// 里再比一遍只会变成"函数和自己比"的同义反复。
    #[test]
    fn bootstrap_shares_the_c2_expansion() {
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let ours = compile_to_c("1 .\n", &here).unwrap();
        let theirs = crate::commands::c2::compile_via_whisperc_vm(
            &crate::commands::c2::expand_with_core("1 .\n", &here).unwrap(),
        )
        .unwrap();
        assert_eq!(ours, theirs);
    }

    /// `std/core` 的自动前置是两边的共同契约，钉住它。
    #[test]
    fn std_core_is_prepended_unless_mentioned() {
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let a = crate::commands::c2::expand_with_core("1 .\n", &here).unwrap();
        assert!(a.contains("std/core") || a.contains("gcd"), "core 应被内联");
        let b = crate::commands::c2::expand_with_core("import \"std/core\"\n1 .\n", &here).unwrap();
        assert_eq!(a, b, "已提及 std/core 时不得重复前置");
    }

    /// 头部必须被带上，否则产物缺 `runtime.c` 的 include。
    #[test]
    fn output_carries_the_runtime_include() {
        let here = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let c = compile_to_c("1 .\n", &here).unwrap();
        assert!(c.contains("#include \"runtime.c\""), "{c}");
    }

    /// `register_host_helpers` 是 `c2.rs` 的活依赖，钉住它别在清理中被删掉。
    /// 只断言"已注册且可调用"——具体的栈语义归 opcode 自己的测试管。
    #[test]
    fn host_helper_pushlist_stays_registered() {
        let mut vm = whisper_core::vm::Vm::new();
        register_host_helpers(&mut vm);
        let r = vm.execute(&[whisper_core::opcode::Opcode::Call("PushList".into())]);
        assert!(
            !matches!(r, Err(whisper_core::VmError::UndefinedWord(_))),
            "PushList 必须保持注册：{r:?}"
        );
    }
}
