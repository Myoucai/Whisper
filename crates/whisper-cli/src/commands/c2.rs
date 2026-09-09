//! c2 target — compile via the self-hosted whisperc compiler running on
//! the Rust VM, then invoke the system C compiler.
//!
//! Pipeline: source → (import inlining) → compiler.all.ws's compile-full
//! + cgen (executed by the Rust VM) → C source → gcc.

use std::path::Path;
use std::rc::Rc;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_parser::Parser;

/// The self-hosted compiler source (frozen fixed-point artifact's source).
const WHISPERC_ALL: &str = include_str!("../../../../whisperc/compiler.all.ws");

/// Recursively inline `import "path"` statements into one flat source.
/// An import may sit on its own line or lead a line with trailing code
/// (`import "std/core" 5 sq .`); the whisperc parser drops the import
/// token itself, so textual inclusion is equivalent. Cycles are elided.
pub fn expand_imports(source: &str, base_dir: &Path) -> Result<String, String> {
    let mut out = String::new();
    let mut seen: Vec<std::path::PathBuf> = Vec::new();
    expand_into(source, base_dir, &mut out, &mut seen, 0)?;
    Ok(out)
}

fn expand_into(
    source: &str,
    base_dir: &Path,
    out: &mut String,
    seen: &mut Vec<std::path::PathBuf>,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err("import nesting too deep (cycle?)".into());
    }
    for line in source.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("import ") {
            // The module is the first double-quoted string; the remainder
            // of the line is code that stays in place.
            let (spec, tail) = match rest.split_once('"') {
                Some((_, after)) => match after.split_once('"') {
                    Some((name, tail)) => (name.to_string(), tail.trim().to_string()),
                    None => (rest.trim().trim_matches('"').to_string(), String::new()),
                },
                None => (rest.trim().trim_matches('"').to_string(), String::new()),
            };
            let path = resolve_module(&spec, base_dir)?;
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if !seen.contains(&canonical) {
                seen.push(canonical);
                let imported =
                    std::fs::read_to_string(&path).map_err(|e| format!("import '{spec}': {e}"))?;
                let parent = path.parent().unwrap_or(Path::new(".")).to_path_buf();
                // 先展开模块本体……
                expand_into(&imported, &parent, out, seen, depth + 1)?;
                // ……再追加命名空间限定克隆（J-4 与 resolver 注入对齐）：
                // 模块每个 `: name {` 定义克隆为 `: 末段.name {`，导出
                // 词以 末段.name 同样可用——限定形态即普通词。
                let ns = spec.rsplit('/').next().unwrap_or(spec.as_str());
                let ns = ns.trim_end_matches(".ws");
                append_qualified_clones(&imported, ns, out);
            }
            if !tail.is_empty() {
                out.push_str(&tail);
                out.push('\n');
            }
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    Ok(())
}

/// Textually clone `: name { ... } ;` definitions of an inlined module as
/// `: ns.name { ... } ;` so qualified references resolve on the c2 path
/// (which bypasses the resolver that does this for `whisper run`).
fn append_qualified_clones(module_src: &str, ns: &str, out: &mut String) {
    let mut lines = module_src.lines().peekable();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(": ") {
            // def header: name may be followed by body on the same line or
            // the body opens on the next line(s) until " ;"
            if let Some(name_end) = rest.find(|c: char| c.is_whitespace()) {
                let name = &rest[..name_end];
                let after = &rest[name_end..];
                if !name.contains('.') {
                    out.push_str(": ");
                    out.push_str(ns);
                    out.push('.');
                    out.push_str(name);
                    out.push_str(after);
                    out.push('\n');
                    // 单行 def（after 已含 ";"）不再拷贝体行；多行体拷到闭合 ";"
                    if !after.trim_end().ends_with(";") {
                        for body in lines.by_ref() {
                            let done = body.trim().ends_with(";");
                            out.push_str(body);
                            out.push('\n');
                            if done {
                                break;
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Resolve a module spec the same way the Rust resolver does for `std/X`
/// (-> <repo>/stdlib/X.ws) and relative paths.
fn resolve_module(spec: &str, base_dir: &Path) -> Result<std::path::PathBuf, String> {
    if let Some(name) = spec.strip_prefix("std/") {
        // repo root = two levels up from crates/whisper-cli
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let p = repo.join("stdlib").join(format!("{name}.ws"));
        if p.exists() {
            return Ok(p);
        }
        return Err(format!("stdlib module not found: {spec}"));
    }
    let p = base_dir.join(format!("{spec}.ws"));
    if p.exists() {
        return Ok(p);
    }
    let p = base_dir.join(spec);
    if p.exists() {
        return Ok(p);
    }
    Err(format!("module not found: {spec}"))
}

/// Run the self-hosted compiler (compiler.all.ws) inside the Rust VM on
/// `source`; returns the generated C code.
pub fn compile_via_whisperc_vm(source: &str) -> Result<String, String> {
    // Parse + compile the whisperc compiler itself; register its words
    // but do NOT execute its main bytecode (the trailing `driver` call
    // reads stdin — we drive the pipeline manually).
    let ast = Parser::parse_source(WHISPERC_ALL)
        .map_err(|e| format!("whisperc parse: {} at {}", e.message, e.token.span.line))?;
    let mut gen = BytecodeGenerator::new();
    let (_main_bc, defs) = gen
        .compile(&ast)
        .map_err(|e| format!("whisperc compile: {}", whisper_codegen::block_ir::contract_errors_to_string(&e)))?;
    let mut vm = Vm::new();
    vm.trace = std::env::var("WHISPER_C2_TRACE").is_ok();
    super::bootstrap::register_host_helpers(&mut vm);
    for (name, code) in defs {
        vm.define_word(name, code);
    }

    // compile-full: source_string -> [main_bc, [[name, def_bc]...]]
    vm.data_stack.push(Value::Str(Rc::new(source.to_string())));
    let compiled = vm
        .execute(&[Opcode::Call("compile-full".to_string())])
        .map_err(|e| format!("whisperc compile-full: {e}"))?
        .ok_or("compile-full: no output")?;

    // cgen: [main_bc, defs] -> C source
    vm.data_stack.push(compiled);
    let c_code = vm
        .execute(&[Opcode::Call("cgen".to_string())])
        .map_err(|e| format!("whisperc cgen: {e}"))?
        .ok_or("cgen: no output")?;
    let c_str = match c_code {
        Value::Str(s) => s.to_string(),
        other => return Err(format!("cgen: expected string, got {}", other.type_name())),
    };
    // The driver word prepends the runtime include via its `header` word;
    // we drive the pipeline manually, so do it here.
    Ok(format!(
        "// Generated by the Whisper compiler\n#include \"runtime.c\"\n\n{c_str}"
    ))
}

/// Compile `source` to a native executable: inline imports, run the
/// self-hosted compiler on the Rust VM, emit C, invoke the C compiler.
pub fn build_c2(
    source: &str,
    source_dir: &Path,
    output: &str,
    inline_imports: bool,
) -> Result<(), String> {
    let expanded = if inline_imports {
        // Parity with the Rust VM's built-in core words: auto-import
        // std/core unless the source already does.
        let with_core = if source.contains("std/core") {
            source.to_string()
        } else {
            format!("import \"std/core\"\n{source}")
        };
        expand_imports(&with_core, source_dir)?
    } else {
        source.to_string()
    };
    if std::env::var("WHISPER_C2_DUMP").is_ok() {
        std::fs::write("D:/tmp/expanded.ws", &expanded).ok();
    }
    let c_code = compile_via_whisperc_vm(&expanded)?;
    let c_path = std::path::PathBuf::from(output).with_extension("c");
    std::fs::write(&c_path, c_code).map_err(|e| format!("write {}: {e}", c_path.display()))?;

    let cc = std::env::var("WHISPER_CC").unwrap_or_else(|_| "gcc".into());
    let whisperc_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../whisperc");
    let status = std::process::Command::new(&cc)
        .arg("-O2")
        .arg("-fno-strict-aliasing")
        .arg("-w")
        .arg("-o")
        .arg(output)
        .arg(&c_path)
        .arg("-I")
        .arg(&whisperc_dir)
        .arg("-lm")
        .args(if cfg!(windows) {
            vec!["-lws2_32"]
        } else {
            vec![]
        })
        .status()
        .map_err(|e| format!("failed to invoke {cc}: {e}"))?;
    if !status.success() {
        return Err(format!(
            "{cc} failed with {status} (see {})",
            c_path.display()
        ));
    }
    println!("Compiled native executable via self-hosted chain -> {output}");
    Ok(())
}
