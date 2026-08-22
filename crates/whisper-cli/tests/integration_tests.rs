//! End-to-end integration tests for the Whisper pipeline.
//!
//! Each test: source text → parse → typecheck → compile → optimize → VM execute.
//! Tests cover all language features: arithmetic, stack, comparisons, logic,
//! strings, lists, control flow, words, confidence, quotations, error cases.

use std::rc::Rc;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_parser::Parser;
use whisper_typecheck::TypeChecker;

// ── Helpers ──────────────────────────────────────────────────────────

/// Compile source, return (main_bytecode, word_defs).
fn compile(source: &str) -> (Vec<Opcode>, std::collections::HashMap<String, Vec<Opcode>>) {
    let ast = Parser::parse_source(source).expect("parse failed");
    let mut gen = BytecodeGenerator::new();
    let (bc, defs) = gen.compile(&ast);
    let bc = whisper_codegen::optimize(&bc);
    let defs: std::collections::HashMap<_, _> = defs
        .into_iter()
        .map(|(k, v)| (k, whisper_codegen::optimize(&v)))
        .collect();
    (bc, defs)
}

/// Compile, typecheck, then execute. Returns the top stack value.
fn run(source: &str) -> Result<Option<Value>, String> {
    let ast = Parser::parse_source(source).map_err(|e| format!("parse: {}", e.message))?;
    let mut tc = TypeChecker::new();
    let errors = tc.check(&ast);
    if !errors.is_empty() {
        return Err(format!("type error: {}", errors[0].message));
    }
    let mut gen = BytecodeGenerator::new();
    let (bc, defs) = gen.compile(&ast);
    let bc = whisper_codegen::optimize(&bc);
    let mut vm = Vm::new();
    for (name, code) in &defs {
        vm.define_word(name.clone(), whisper_codegen::optimize(code));
    }
    vm.execute(&bc).map_err(|e| format!("runtime: {e}"))
}

/// Run and expect a specific I64 result.
fn assert_run_i64(source: &str, expected: i64) {
    let r = run(source).expect("execution failed").expect("no result");
    assert_eq!(r.unwrap_signal(), Value::I64(expected), "source: {source}");
}

/// Run and expect a specific Bool result.
fn assert_run_bool(source: &str, expected: bool) {
    let r = run(source).expect("execution failed").expect("no result");
    assert_eq!(r.unwrap_signal(), Value::Bool(expected), "source: {source}");
}

/// Run and expect a specific Str result.
fn assert_run_str(source: &str, expected: &str) {
    let r = run(source).expect("execution failed").expect("no result");
    assert_eq!(
        r.unwrap_signal(),
        Value::Str(Rc::new(expected.into())),
        "source: {source}"
    );
}

/// Run and expect a runtime error.
fn assert_run_err(source: &str) {
    let result = run(source);
    assert!(
        result.is_err(),
        "expected error for: {source}, got {result:?}"
    );
}

/// Run and expect a type error.
fn assert_type_err(source: &str) {
    let ast = Parser::parse_source(source).expect("parse failed");
    let mut tc = TypeChecker::new();
    let errors = tc.check(&ast);
    assert!(!errors.is_empty(), "expected type error for: {source}");
}

// ── Arithmetic ────────────────────────────────────────────────────────

#[test]
fn int_add() {
    assert_run_i64("3 4 +", 7);
}
#[test]
fn int_sub() {
    assert_run_i64("10 3 -", 7);
}
#[test]
fn int_mul() {
    assert_run_i64("5 6 *", 30);
}
#[test]
fn int_div() {
    assert_run_i64("42 6 /", 7);
}
#[test]
fn int_mod() {
    assert_run_i64("10 3 mod", 1);
}
#[test]
fn complex_expr() {
    assert_run_i64("3 4 + 2 *", 14);
}
#[test]
fn div_by_zero() {
    assert_run_err("10 0 /");
}
#[test]
fn mod_by_zero() {
    assert_run_err("10 0 mod");
}

// ── Stack operations ──────────────────────────────────────────────────

#[test]
fn dup_sq() {
    assert_run_i64("5 _ *", 25);
}
#[test]
fn swap_sub() {
    assert_run_i64("3 4 ` -", 1);
}
#[test]
fn drop_op() {
    assert_run_i64("42 99 drop", 42);
}
#[test]
fn nested_dup() {
    assert_run_i64("3 _ _ * *", 27);
} // 3 dup dup mul mul = 27
#[test]
fn stack_underflow() {
    assert_run_err("+");
}

// ── Comparison ────────────────────────────────────────────────────────

#[test]
fn eq_true() {
    assert_run_bool("5 5 =", true);
}
#[test]
fn eq_false() {
    assert_run_bool("3 4 =", false);
}
#[test]
fn lt_true() {
    assert_run_bool("3 5 <", true);
}
#[test]
fn lt_false() {
    assert_run_bool("5 3 <", false);
}
#[test]
fn gt_true() {
    assert_run_bool("5 3 >", true);
}
#[test]
fn neq_true() {
    assert_run_bool("3 4 !=", true);
}
#[test]
fn le_true() {
    assert_run_bool("3 3 <=", true);
}
#[test]
fn ge_true() {
    assert_run_bool("5 5 >=", true);
}

// ── Logic ─────────────────────────────────────────────────────────────

#[test]
fn and_true() {
    assert_run_bool("#t #t &", true);
}
#[test]
fn and_false() {
    assert_run_bool("#t #f &", false);
}
#[test]
fn or_true() {
    assert_run_bool("#f #t |", true);
}
#[test]
fn or_false() {
    assert_run_bool("#f #f |", false);
}
#[test]
fn not_true() {
    assert_run_bool("#f !", true);
}
#[test]
fn not_false() {
    assert_run_bool("#t !", false);
}

// ── String operations ─────────────────────────────────────────────────

#[test]
fn strlen_hello() {
    assert_run_i64("\"Hello\" strlen", 5);
}
#[test]
fn strlen_empty() {
    assert_run_i64("\"\" strlen", 0);
}
#[test]
fn strcat_basic() {
    assert_run_str("\"Hello, \" \"World!\" strcat", "Hello, World!");
}
#[test]
fn strcat_empty() {
    assert_run_str("\"\" \"Hi\" strcat", "Hi");
}
#[test]
fn strslice_hello() {
    assert_run_str("\"Hello, World!\" 0 5 strslice", "Hello");
}
#[test]
fn strslice_mid() {
    assert_run_str("\"abcdef\" 2 3 strslice", "cde");
}

// ── List operations ───────────────────────────────────────────────────

#[test]
fn list_len() {
    assert_run_i64("[1 2 3 4 5] len", 5);
}
#[test]
fn list_empty_len() {
    assert_run_i64("[] len", 0);
}
#[test]
fn list_nth_first() {
    assert_run_i64("[10 20 30] 0 @nth", 10);
}
#[test]
fn list_nth_last() {
    assert_run_i64("[10 20 30] 2 @nth", 30);
}
#[test]
fn list_nth_oob() {
    assert_run_err("[10 20] 5 @nth");
}
#[test]
fn list_append() {
    assert_run_i64("[1 2] 3 append len", 3);
}
#[test]
fn list_map() {
    assert_run_i64("[1 2 3] { _ * } @map 2 @nth", 9);
} // [1,4,9], nth 2 = 9
#[test]
fn list_map_len() {
    assert_run_i64("[1 2 3 4] { 1 + } @map len", 4);
}
#[test]
fn list_fold_sum() {
    assert_run_i64("[1 2 3 4 5] 0 { + } @fold", 15);
}
#[test]
fn list_fold_product() {
    assert_run_i64("[1 2 3 4] 1 { * } @fold", 24);
}

// ── Control flow ──────────────────────────────────────────────────────

#[test]
fn cond_true() {
    assert_run_i64("5 3 > ??100|0]", 100);
}
#[test]
fn cond_false() {
    assert_run_i64("2 3 > ??100|0]", 0);
}
#[test]
fn cond_true_then_false_else() {
    assert_run_i64("#t ??100|0]", 100);
}
#[test]
fn cond_false_then_false_else() {
    assert_run_i64("#f ??100|0]", 0);
}

#[test]
fn cond_without_else() {
    // When condition is true, execute then-branch, otherwise skip it
    // Result will be 42 (from then-branch) or the value below (if cond is false)
    let source = "3 3 = ??42]";
    let r = run(source).expect("exec failed").expect("no result");
    assert_eq!(r.unwrap_signal(), Value::I64(42));
}

// ── Word definitions ──────────────────────────────────────────────────

#[test]
fn simple_word() {
    assert_run_i64(": sq { _ * } ; 5 sq", 25);
}
#[test]
fn multi_word() {
    assert_run_i64(": double { 2 * } ; : sq { _ * } ; 5 double sq", 100);
}
#[test]
fn factorial_word() {
    // 5! = 120 using recursive definition
    let source = ": fact { _ 1 > ??_ 1 - fact *|drop 1] } ; 5 fact";
    assert_run_i64(source, 120);
}
#[test]
fn fib_word() {
    let source = ": fib { _ 1 > ??_ 1 - fib ` 2 - fib +|] } ; 10 fib";
    assert_run_i64(source, 55);
}

// ── Quotations ────────────────────────────────────────────────────────

#[test]
fn quote_double() {
    let mut vm = Vm::new();
    let block: Rc<[Opcode]> =
        Rc::from(vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return].into_boxed_slice());
    vm.data_stack.push(Value::I64(7));
    vm.execute_ref(&block).unwrap();
    assert_eq!(vm.data_stack.pop().unwrap(), Value::I64(14));
}
#[test]
fn quote_in_list_ops() {
    assert_run_i64("[1 2 3] { 2 * } @map 2 @nth", 6);
}

// ── Confidence ────────────────────────────────────────────────────────

#[test]
fn confidence_label() {
    // execute() returns the top stack value (pops it)
    let source = "42 :0.5";
    let (bc, _) = compile(source);
    let mut vm = Vm::new();
    let val = vm.execute(&bc).unwrap().unwrap();
    match val {
        Value::Signal(v, c) => {
            assert_eq!(*v, Value::I64(42));
            assert!((c - 0.5).abs() < 0.001, "expected 0.5 got {c}");
        }
        _ => panic!("expected Signal, got {val:?}"),
    }
}
#[test]
fn confidence_arith() {
    // 10 :0.5 2 * → 20:0.5 (confidence 0.5 * 1.0 = 0.5)
    let source = "10 :0.5 2 *";
    let (bc, _) = compile(source);
    let mut vm = Vm::new();
    let r = vm.execute(&bc).unwrap().unwrap();
    match r {
        Value::Signal(v, c) => {
            assert_eq!(*v, Value::I64(20));
            assert!((c - 0.5).abs() < 0.001);
        }
        _ => panic!("expected Signal, got {r:?}"),
    }
}

// ── Type checking ─────────────────────────────────────────────────────

#[test]
fn type_check_ok() {
    let source = "3 4 +";
    let ast = Parser::parse_source(source).unwrap();
    let mut tc = TypeChecker::new();
    assert!(tc.check(&ast).is_empty());
}
#[test]
fn type_check_bad_len() {
    assert_type_err("5 len");
}
#[test]
fn type_check_bad_arith() {
    assert_type_err("\"hi\" 3 +");
}

// ── Edge cases ────────────────────────────────────────────────────────

#[test]
fn empty_program() {
    let source = "";
    let r = run(source).expect("exec failed");
    assert_eq!(r, None);
}
#[test]
fn large_stack() {
    // Push many values and sum them
    let mut source = String::new();
    for i in 1..=50 {
        source.push_str(&format!("{i} "));
    }
    for _ in 1..=49 {
        source.push_str("+ ");
    }
    // Sum 1..50 = 1275
    assert_run_i64(&source, 1275);
}
#[test]
fn output_consumes_value() {
    let source = "42 .";
    let r = run(source).expect("exec failed");
    assert_eq!(r, None); // OutputTop consumes the value
}

// ── Bootstrap pipeline ────────────────────────────────────────────────

#[test]
fn bootstrap_hello() {
    // Verify the Rust reference pipeline works for hello.ws
    let source = "\"Hello, World!\" .";
    let r = run(source);
    assert!(r.is_ok(), "hello.ws pipeline failed: {r:?}");
}
#[test]
fn bootstrap_fib() {
    let source = ": fib { _ 1 > ??_ 1 - fib ` 2 - fib +|] } ; 10 fib";
    assert_run_i64(source, 55);
}

// ── Import/Export ─────────────────────────────────────────────────────

#[test]
fn import_export_noop() {
    // import/export are compile-time directives, should not error
    let source = "import \"std/math\" export sq : sq { _ * } ; 5 sq";
    assert_run_i64(source, 25);
}

// ── Capability sandbox ────────────────────────────────────────────────

#[test]
fn capability_not_bound() {
    // @0 ! without capability binding should error
    let source = "\"test.txt\" @0 !";
    let r = run(source);
    assert!(r.is_err(), "unbound capability should error");
}
#[test]
fn capability_bound_executes() {
    use whisper_core::capability::{CapabilityTable, FileReadCap};
    let source = "\"test.txt\" @0 !";
    let ast = Parser::parse_source(source).unwrap();
    let mut gen = BytecodeGenerator::new();
    let (bc, _) = gen.compile(&ast);
    let mut cap_table = CapabilityTable::new();
    cap_table.bind(Box::new(FileReadCap {
        id: 0,
        allowed_paths: vec![std::env::temp_dir()],
    }));
    let mut vm = Vm::with_capabilities(cap_table);
    // Stack: str → push file path
    vm.data_stack.push(Value::Str(Rc::new(
        std::env::temp_dir()
            .join("nonexistent.txt")
            .display()
            .to_string(),
    )));
    let r = vm.execute(&bc);
    // Should fail with IO error (file not found) not capability error
    assert!(r.is_err());
    let err_msg = format!("{r:?}");
    assert!(
        !err_msg.contains("CapabilityNotBound"),
        "cap should be bound"
    );
}

// ── WASM compilation ──────────────────────────────────────────────────

// ── .wbin roundtrip ──────────────────────────────────────────────────
#[test]
fn wbin_roundtrip_program() {
    let source = "3 4 + 2 *";
    let ast = Parser::parse_source(source).unwrap();
    let mut gen = BytecodeGenerator::new();
    let (bc, _) = gen.compile(&ast);
    let data = whisper_codegen::wbin::WbinWriter::write(&bc);
    let decoded = whisper_codegen::wbin::WbinReader::decode(&data).unwrap();
    let mut vm = Vm::new();
    let r = vm.execute(&decoded).unwrap().unwrap();
    assert_eq!(r.unwrap_signal(), Value::I64(14));
}

// ── Phase 0 new opcodes ──────────────────────────────────────────────

#[test]
fn test_strsplit_basic() {
    let source = r#""a-b-c" "-" strsplit len"#;
    assert_run_i64(source, 3);
}

#[test]
fn test_strsplit_words() {
    let source = r#""hello world" " " strsplit len"#;
    assert_run_i64(source, 2);
}

#[test]
fn test_filter_basic() {
    let source = "[1 2 3 4 5] { 3 > } @filter len";
    assert_run_i64(source, 2); // [4, 5]
}

#[test]
fn test_filter_empty_result() {
    let source = "[1 2 3] { 10 > } @filter len";
    assert_run_i64(source, 0);
}

#[test]
fn test_list_slice_basic() {
    let source = "[10 20 30 40 50] 1 3 list-slice len";
    assert_run_i64(source, 3);
}

#[test]
fn test_list_slice_first() {
    let source = "[10 20 30] 0 2 list-slice 1 @nth";
    assert_run_i64(source, 20);
}

#[test]
fn test_ctos_basic() {
    let source = "65 ctos"; // ASCII 'A'
    assert_run_str(source, "A");
}

#[test]
fn test_shl_basic() {
    let source = "1 4 <<"; // 1 << 4 = 16
    assert_run_i64(source, 16);
}

#[test]
fn test_shr_basic() {
    let source = "16 2 >>"; // 16 >> 2 = 4
    assert_run_i64(source, 4);
}

#[test]
fn test_shl_zero() {
    let source = "42 0 <<";
    assert_run_i64(source, 42);
}

// ── Phase 0 new opcodes ────────────────────────────────────────────

#[test]
fn filter_pipeline() {
    // [3 1 4 1 5] { _ 2 > } @filter len -> 3
    assert_run_i64("[3 1 4 1 5] { _ 2 > } @filter len", 3);
}

#[test]
fn filter_keep_all() {
    assert_run_i64("[1 2 3] { #t } @filter len", 3);
}

#[test]
fn filter_keep_none() {
    assert_run_i64("[1 2 3] { #f } @filter len", 0);
}

#[test]
fn strsplit_pipeline() {
    assert_run_i64("\"hello world\" \" \" strsplit len", 2);
}

#[test]
fn strsplit_first_word() {
    // Split and get first element
    assert_run_str("\"hello world\" \" \" strsplit 0 @nth", "hello");
}

#[test]
fn ctos_pipeline() {
    assert_run_str("65 ctos", "A");
}

#[test]
fn ctos_lowercase() {
    assert_run_str("97 ctos", "a");
}

#[test]
fn listslice_pipeline() {
    assert_run_i64("[10 20 30 40 50] 1 3 listslice len", 3);
}

#[test]
fn listslice_first_element() {
    assert_run_i64("[10 20 30 40 50] 1 3 listslice 0 @nth", 20);
}

#[test]
fn shl_pipeline() {
    assert_run_i64("1 3 <<", 8);
}

#[test]
fn shr_pipeline() {
    assert_run_i64("8 2 >>", 2);
}

#[test]
fn shl_larger() {
    assert_run_i64("1 10 <<", 1024);
}

// ── wbin roundtrip for new opcodes ────────────────────────────────

#[test]
fn wbin_roundtrip_new_opcodes() {
    use whisper_core::opcode::Opcode;
    let ops = vec![
        Opcode::PushI64(1),
        Opcode::PushI64(3),
        Opcode::Shl,
        Opcode::PushI64(8),
        Opcode::Eq,
    ];
    let data = whisper_codegen::wbin::WbinWriter::write(&ops);
    let decoded = whisper_codegen::wbin::WbinReader::decode(&data).unwrap();
    assert_eq!(ops, decoded);
}

// ── Stdlib import smoke tests ────────────────────────────────────────
// These exercise the resolver against the real stdlib/ directory and catch
// regressions where a lexer keyword breaks a stdlib definition name.

fn run_with_imports(source: &str) -> Result<Option<Value>, String> {
    let ast = Parser::parse_source(source).map_err(|e| e.message)?;
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../");
    let resolved =
        whisper_parser::resolve_imports(ast, &manifest).map_err(|e| format!("import: {e:?}"))?;
    let mut gen = BytecodeGenerator::new();
    let (bc, defs) = gen.compile(&resolved.ast);
    let mut vm = Vm::new();
    for (name, code) in defs {
        vm.define_word(name, code);
    }
    vm.execute(&bc).map_err(|e| format!("{e}"))
}

#[test]
fn test_stdlib_math_smoke() {
    // math.ws defines words whose names are now lexer keywords
    // (floor, ceil, round, log, exp, atan2, bitand, ...) — this guards the
    // keyword_name round-trip against real stdlib sources.
    let r = run_with_imports(
        "import \"std/math\"
pi floor",
    )
    .expect("math import failed");
    assert_eq!(r, Some(Value::F64(3.0)), "floor(pi) should be 3.0");
}

#[test]
fn test_stdlib_list_smoke() {
    let r = run_with_imports(
        "import \"std/list\"
[1 2 3 4 5] { _ * } @map sum",
    )
    .expect("list import failed");
    assert_eq!(
        r,
        Some(Value::I64(55)),
        "sum of squares of [1..5] should be 55"
    );
}

#[test]
fn test_stdlib_str_smoke() {
    let r = run_with_imports(
        "import \"std/str\"
\"hello\" strlen",
    )
    .expect("str import failed");
    assert_eq!(r, Some(Value::I64(5)), "strlen(hello) should be 5");
}

// ── c2 self-hosted chain tests ───────────────────────────────────────

fn have_cc() -> bool {
    ["cc", "gcc", "clang"].iter().any(|cc| {
        std::process::Command::new(cc)
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

#[test]
fn c2_compiles_and_runs_via_self_hosted_chain() {
    if !have_cc() {
        eprintln!("skipping c2 test (no C compiler)");
        return;
    }
    let src = ": sq { _ * } ; 5 sq .
3 4 + .
5 3 > ?? 100 | 0 ] .";
    let c_code = commands_c2_compile(src).expect("c2 compile failed");
    let dir = std::env::temp_dir().join("whisper_c2_it");
    let _ = std::fs::create_dir_all(&dir);
    let c_path = dir.join("p.c");
    std::fs::write(&c_path, c_code).unwrap();
    let bin = dir.join("p.exe");
    let manifest = env!("CARGO_MANIFEST_DIR");
    let out = std::process::Command::new("gcc")
        .arg("-O2")
        .arg("-fno-strict-aliasing")
        .arg("-w")
        .arg("-o")
        .arg(&bin)
        .arg(&c_path)
        .arg("-I")
        .arg(format!("{manifest}/../../whisperc"))
        .arg("-lm")
        .args(if cfg!(windows) {
            vec!["-lws2_32"]
        } else {
            vec![]
        })
        .output()
        .expect("gcc failed to start");
    assert!(
        out.status.success(),
        "gcc failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let run = std::process::Command::new(&bin)
        .output()
        .expect("run failed");
    let stdout = String::from_utf8_lossy(&run.stdout).replace("\r\n", "\n");
    assert_eq!(
        stdout.trim(),
        "25\n7\n100",
        "c2 self-hosted chain output mismatch"
    );
}

// Wrapper so the test binary can reach the c2 module (bin crate: tests
// link against the crate root via `commands` path below).
fn commands_c2_compile(src: &str) -> Result<String, String> {
    // The c2 module lives in the binary crate; replicate its core here by
    // calling the public API through the built CLI is overkill — instead
    // use the library path: whisper-cli is a bin-only crate, so we exercise
    // compile_via_whisperc_vm indirectly via the c2 build function.
    whisper_c2::compile_for_test(src)
}

mod whisper_c2 {
    pub fn compile_for_test(src: &str) -> Result<String, String> {
        let ast =
            whisper_parser::Parser::parse_source(include_str!("../../../whisperc/compiler.all.ws"))
                .map_err(|e| format!("whisperc parse: {e:?}"))?;
        let mut gen = whisper_codegen::bytecode_gen::BytecodeGenerator::new();
        let (_main, defs) = gen.compile(&ast);
        let mut vm = whisper_core::vm::Vm::new();
        vm.define_word(
            "PushList".into(),
            vec![whisper_core::opcode::Opcode::PushList],
        );
        for (name, code) in defs {
            vm.define_word(name, code);
        }
        vm.data_stack
            .push(whisper_core::value::Value::Str(std::rc::Rc::new(
                src.to_string(),
            )));
        let compiled = vm
            .execute(&[whisper_core::opcode::Opcode::Call("compile-full".into())])
            .map_err(|e| format!("{e}"))?
            .ok_or("no output")?;
        vm.data_stack.push(compiled);
        let c = vm
            .execute(&[whisper_core::opcode::Opcode::Call("cgen".into())])
            .map_err(|e| format!("{e}"))?
            .ok_or("no cgen output")?;
        match c {
            whisper_core::value::Value::Str(s) => Ok(format!(
                "// Generated by the Whisper compiler
#include \"runtime.c\"

{s}"
            )),
            o => Err(format!("cgen returned {}", o.type_name())),
        }
    }
}
