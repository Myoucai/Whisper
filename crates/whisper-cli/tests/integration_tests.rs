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
    let (bc, defs) = gen.compile(&ast).unwrap();
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
    let (bc, defs) = gen.compile(&ast).unwrap();
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
    let (bc, _) = gen.compile(&ast).unwrap();
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
    let (bc, _) = gen.compile(&ast).unwrap();
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
    let (bc, defs) = gen.compile(&resolved.ast).unwrap();
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

/// Regression (2026-09-21): a 2048-frame word-call ceiling made every
/// `-t c2` build that imports a stdlib module fail with RecursionLimit — the
/// self-hosted compiler recurses roughly once per token, so std/math (1.9 KB)
/// already needs >2048 frames. The c2 chain was unusable for real programs
/// (stdlib-gate, app-gate, net-gate and opcode_coverage all build this way).
#[test]
fn c2_chain_compiles_stdlib_imports() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let ws = std::env::temp_dir().join("whisper_c2_stdlib_it");
    std::fs::create_dir_all(&ws).unwrap();
    let src = ws.join("m.ws");
    std::fs::write(&src, "import \"std/math\"\n5 sq .\npi floor f64toi64 .\n").unwrap();
    let exe = ws.join("m_exe");

    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "c2", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "c2 build with a stdlib import failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let artifact = if exe.exists() {
        exe.clone()
    } else {
        exe.with_extension(std::env::consts::EXE_EXTENSION)
    };
    let run = std::process::Command::new(&artifact).output().unwrap();
    assert!(run.status.success());
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace('\r', ""),
        "25\n3\n"
    );
}

/// The `@n` capability ABI must behave identically on both chains. Before
/// Step 2a the C chain emitted calls to undefined `W__<n>` functions, so
/// `import "std/os"` — whose `exec`/`getenv` are capability wrappers — could
/// not be compiled at all, and no c2 gate used a capability token, so the gap
/// went unnoticed. The shell coverage gate covers @4/@5; this adds the file
/// pair (@0/@1), which needs a CWD-anchored absolute path because the VM
/// whitelists file capabilities to the process CWD.
#[test]
fn capability_abi_matches_across_chains() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let cwd = std::env::current_dir().unwrap();
    let probe = cwd.join("whisper_cap_abi_probe.txt");
    let probe_s = probe.to_string_lossy().replace('\\', "/");
    let src_text = format!(
        "\"{probe_s}\" \"abcdef\" @1 .\n\
         \"{probe_s}\" @0 .\n\
         \"WHISPER_CAP_ABI_ENV\" @4 .\n\
         \"echo hi\" @5 1 @nth .\n\
         \"exit 7\" @5 0 @nth .\n"
    );
    let dir = std::env::temp_dir().join("whisper_cap_abi_it");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("cap.ws");
    std::fs::write(&src, &src_text).unwrap();

    let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args([
            "run",
            "--no-strict-types",
            "--allow-file-read",
            "--allow-file-write",
            "--allow-env",
            "--allow-exec",
        ])
        .arg(&src)
        .env("WHISPER_CAP_ABI_ENV", "envval")
        .output()
        .unwrap();
    assert!(
        vm.status.success(),
        "VM run failed: {}",
        String::from_utf8_lossy(&vm.stderr)
    );

    let exe = dir.join("cap_exe");
    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "c2", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "c2 build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let artifact = if exe.exists() {
        exe.clone()
    } else {
        exe.with_extension(std::env::consts::EXE_EXTENSION)
    };
    let c2 = std::process::Command::new(&artifact)
        .env("WHISPER_CAP_ABI_ENV", "envval")
        .output()
        .unwrap();
    assert!(c2.status.success());

    // The VM prints strings quoted, the C chain prints them raw — the one
    // documented display divergence, so compare quote-stripped.
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");
    let (a, b) = (norm(&vm.stdout), norm(&c2.stdout));
    assert_eq!(a, b, "capability ABI diverged between chains");
    assert_eq!(a, "6\nabcdef\nenvval\nhi\n7\n");

    let _ = std::fs::remove_file(&probe);
}

// ── `call`: dynamic quotation invocation ───────────────────────────

/// `call` exposes `Opcode::CapExec`, which the VM has always implemented but
/// which had no reachable spelling: the design intended `!`, and `!` went to
/// logical not (spec §3.2, frozen), so `TokenKind::Bang` stopped being
/// produced by the lexer and the opcode was left orphaned.
///
/// Only the single-quotation-literal forms are asserted here, because the
/// self-hosted chain cannot yet compile quotations as first-class values
/// (two or more quote literals, or a quote inside a list literal) — a
/// pre-existing limitation unrelated to `call`, tracked in
/// docs/language-backlog.md as campaign O.
#[test]
fn call_invokes_quotation_on_both_chains() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_call_it");
    std::fs::create_dir_all(&dir).unwrap();

    // One quote literal per program on purpose: the self-hosted chain bails
    // out on two or more (see the limitation note above), so a single source
    // file exercising all three forms would assert the wrong thing.
    let cases = [
        ("basic", "{ 42 } call .\n", "42\n"),
        ("input", "10 { 2 * } call .\n", "20\n"),
        ("in-def", ": ap { call } ; 5 { 1 + } ap .\n", "6\n"),
    ];

    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");

    for (name, source, expected) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["run", "--no-strict-types"])
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            vm.status.success(),
            "[{name}] VM run failed: {}",
            String::from_utf8_lossy(&vm.stderr)
        );

        let exe = dir.join(format!("{name}_exe"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "[{name}] c2 build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let c2 = std::process::Command::new(&artifact).output().unwrap();
        assert!(c2.status.success(), "[{name}] native run failed");

        assert_eq!(norm(&vm.stdout), expected, "[{name}] VM output");
        assert_eq!(
            norm(&c2.stdout),
            expected,
            "[{name}] `call` diverged between chains"
        );
    }
}

// ── Campaign O / O′: quotation literals as values ──────────────────

/// Two or more non-fused quotation literals used to break the self-hosted
/// compiler. `genq-name` re-read its counter from the KV store and left the
/// original copy on the stack — invisible with one quote (the true branch
/// drops it), but every extra quote piled up another stray string and pushed
/// the real operands out of reach ("Type mismatch: expected [T], got str").
///
/// A third defect hid behind the first two: `try-def-colon` / `try-loop-inner`
/// called bare `streq` on whatever token followed a quotation body, which may
/// be another quotation node (a list). The Rust VM raised "expected str, got
/// [T]" while the C chain's `w_str_eq` silently returned false — a real
/// cross-engine divergence, visible only from three quotes onward and fixed
/// with the guarded `str-eq?`.
///
/// Quotations inside *list literals* are covered by
/// `quotation_in_list_literal_compiles_on_both_chains` (campaign O′).
#[test]
fn multiple_quotation_literals_compile_on_both_chains() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_quotes_it");
    std::fs::create_dir_all(&dir).unwrap();

    let cases = [
        ("pair", "{ 1 + } { 2 * } drop drop 7 .\n", "7\n"),
        (
            "triple",
            "{ 1 + } { 2 * } { 3 * } drop drop drop 7 .\n",
            "7\n",
        ),
        (
            "quad",
            "{ 1 + } { 2 * } { 3 * } { 4 * } drop drop drop drop 9 .\n",
            "9\n",
        ),
    ];
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");

    for (name, source, expected) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["run", "--no-strict-types"])
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            vm.status.success(),
            "[{name}] VM run failed: {}",
            String::from_utf8_lossy(&vm.stderr)
        );

        let exe = dir.join(format!("{name}_exe"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "[{name}] c2 build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let c2 = std::process::Command::new(&artifact).output().unwrap();
        assert!(c2.status.success(), "[{name}] native run failed");

        assert_eq!(norm(&vm.stdout), expected, "[{name}] VM output");
        assert_eq!(norm(&c2.stdout), expected, "[{name}] chains diverged");
    }
}

/// Campaign O′ — quotations *inside* list literals, and the dynamic dispatch
/// they exist for (stash a quotation in a list, take it out, `call` it).
///
/// `fuse-hof` expands a quotation used as a value into a def/ref pair:
/// `[9, "__qN", body]` (the definition) plus `[16, "__qN"]` (the reference).
/// Only the top level reaches `split-defs`, which is what turns a tag-9 token
/// into a real static function; inside a container node `compile-one` has no
/// tag-9 branch, so the definition was silently dropped and the reference
/// pointed at a function that was never emitted — gcc then failed with
/// `'W___q0' undeclared`. `hs` / `part` now hoist those definitions out of the
/// node before the surrounding token stream is compiled.
#[test]
fn quotation_in_list_literal_compiles_on_both_chains() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_qlist_it");
    std::fs::create_dir_all(&dir).unwrap();

    let cases = [
        // Regression guard: plain list literals must keep compiling.
        ("plain", "[] len .\n", "0\n"),
        ("flat", "[1 2 3] len .\n", "3\n"),
        ("nested", "[[1] [2]] len .\n", "2\n"),
        // One and two quotations inside a list literal.
        ("one", "[{ 1 + }] len .\n", "1\n"),
        ("two", "[{ 1 + } { 2 * }] len .\n", "2\n"),
        // Dynamic dispatch: retrieve and call.
        ("call", "[{ 1 + }] 0 @nth 5 swap call .\n", "6\n"),
        (
            "dispatch",
            "[{ 1 + } { 2 * } { 3 * }] 1 @nth 5 swap call .\n",
            "10\n",
        ),
        ("deep", "[[{ 1 + }]] 0 @nth 0 @nth 5 swap call .\n", "6\n"),
        ("input", "9 [{ 1 + }] 0 @nth call .\n", "10\n"),
    ];
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");

    for (name, source, expected) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["run", "--no-strict-types"])
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            vm.status.success(),
            "[{name}] VM run failed: {}",
            String::from_utf8_lossy(&vm.stderr)
        );

        let exe = dir.join(format!("{name}_exe"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "[{name}] c2 build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let c2 = std::process::Command::new(&artifact).output().unwrap();
        assert!(c2.status.success(), "[{name}] native run failed");

        assert_eq!(norm(&vm.stdout), expected, "[{name}] VM output");
        assert_eq!(norm(&c2.stdout), expected, "[{name}] chains diverged");
    }
}

/// Campaign O₂ — the same def/ref hoisting, for the *other* four container
/// kinds. A quotation used as a value can sit inside a list, a conditional
/// branch, a loop body, a word definition body or a fused higher-order body;
/// all five are container nodes and all five dropped the synthesised
/// definition the same way. `hs` dispatches on the node tag range (6..9 and
/// 249..254) so one code path covers every container.
#[test]
fn quotation_in_any_container_compiles_on_both_chains() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_qcont_it");
    std::fs::create_dir_all(&dir).unwrap();

    let cases = [
        // Conditional branches (node [7, then, else]).
        ("cond_then", "#t ?? { 9 } | { 8 } ] call .\n", "9\n"),
        ("cond_else", "#f ?? { 9 } | { 8 } ] call .\n", "8\n"),
        (
            "cond_multi",
            "#t ?? { 9 } { 10 } | { 8 } ] drop call .\n",
            "9\n",
        ),
        (
            "cond_word",
            ": f { 3 } ; #t ?? { f } | { 7 } ] call .\n",
            "3\n",
        ),
        ("cond_plain", "#t ?? 1 | 2 ] .\n", "1\n"),
        // Loop bodies (node [8, body, cond]).
        ("loop_body", "2 { { 5 } drop 1 - } { dup 0 > } # .\n", "0\n"),
        ("loop_plain", "0 { 1 + } { dup 3 < } # .\n", "3\n"),
        // Word definition bodies (node [9, name, body]). The body is field 2;
        // field 1 is the name and must be left alone.
        ("def_body", ": f { { 5 } drop 7 } ; f .\n", "7\n"),
        ("def_plain", ": f { 7 } ; f .\n", "7\n"),
        // Quotation inside a quotation, and inside a fused higher-order body
        // (node [249..254, inner]).
        ("quote_nest", "{ { 5 } } call call .\n", "5\n"),
        ("hof_body", "[1 2] { { 7 } drop 1 + } @map .\n", "[2 3]\n"),
    ];
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");

    for (name, source, expected) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["run", "--no-strict-types"])
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            vm.status.success(),
            "[{name}] VM run failed: {}",
            String::from_utf8_lossy(&vm.stderr)
        );

        let exe = dir.join(format!("{name}_exe"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "[{name}] c2 build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let c2 = std::process::Command::new(&artifact).output().unwrap();
        assert!(c2.status.success(), "[{name}] native run failed");

        assert_eq!(norm(&vm.stdout), expected, "[{name}] VM output");
        assert_eq!(norm(&c2.stdout), expected, "[{name}] chains diverged");
    }
}

// ── Step 2b: HTTP capabilities on the C chain ──────────────────────

/// Read one whole HTTP request (headers + `Content-Length` body) off `sock`.
fn read_http_request(sock: &mut impl std::io::Read) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let mut want = 0usize; // total bytes once the header terminator shows up
    loop {
        if want > 0 && buf.len() >= want {
            return Some(buf);
        }
        match sock.read(&mut tmp) {
            Ok(0) => return if buf.is_empty() { None } else { Some(buf) },
            Ok(k) => buf.extend_from_slice(&tmp[..k]),
            Err(_) => return if buf.is_empty() { None } else { Some(buf) },
        }
        if want == 0 {
            if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                let clen: usize = head
                    .lines()
                    .find_map(|l| l.strip_prefix("content-length:"))
                    .and_then(|v| v.trim().parse().ok())
                    .unwrap_or(0);
                want = p + 4 + clen;
            }
        }
    }
}

/// `@2` (http-get) and `@3` (http-post) must put the same bytes on the wire and
/// return the same body on both chains. The server here is an in-process
/// `TcpListener`, so the test needs no external service; it records every
/// request so the two chains can be compared at the protocol level too.
///
/// HTTPS is deliberately *not* exercised: neither chain implements TLS, and the
/// pre-registered criterion (tests/step2b_preregistered.md, S2) only requires
/// that both refuse it rather than sending plaintext to port 443.
#[test]
fn http_capabilities_match_across_chains() {
    use std::io::Write as _;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }

    // (name, program template — {url} is substituted, expected stdout)
    let cases: [(&str, &str, &str); 4] = [
        ("get", "\"{url}/hi\" @2 .\n", "resp:/hi\n"),
        ("get_nested", "\"{url}/a/b?c=d\" @2 .\n", "resp:/a/b?c=d\n"),
        (
            "post",
            "\"{url}/echo\" \"{\\\"a\\\":1}\" @3 .\n",
            "resp:/echo\n",
        ),
        ("get_again", "\"{url}/zz\" @2 .\n", "resp:/zz\n"),
    ];

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let seen_srv = Arc::clone(&seen);
    let expected_requests = cases.len() * 2; // one per chain per case

    let server = std::thread::spawn(move || {
        for _ in 0..expected_requests {
            let (mut sock, _) = match listener.accept() {
                Ok(x) => x,
                Err(_) => return,
            };
            let req = match read_http_request(&mut sock) {
                Some(r) => r,
                None => continue,
            };
            let text = String::from_utf8_lossy(&req).to_string();
            // Body = "resp:" + request path, so the reply identifies the path
            // the server actually saw.
            let path = text
                .lines()
                .next()
                .and_then(|l| l.split(' ').nth(1))
                .unwrap_or("/")
                .to_string();
            seen_srv.lock().unwrap().push(text);
            let body = format!("resp:{path}");
            let _ = write!(
                sock,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.flush();
        }
    });

    let dir = std::env::temp_dir().join("whisper_http_it");
    std::fs::create_dir_all(&dir).unwrap();
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");

    for (name, template, expected) in cases {
        let source = template.replace("{url}", &format!("http://127.0.0.1:{port}"));
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, &source).unwrap();

        let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["run", "--no-strict-types", "--allow-http"])
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            vm.status.success(),
            "[{name}] VM run failed: {}",
            String::from_utf8_lossy(&vm.stderr)
        );

        let exe = dir.join(format!("{name}_exe"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "[{name}] c2 build failed: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let c2 = std::process::Command::new(&artifact)
            .env("WHISPER_NET", "1")
            .env_remove("WHISPER_NET_ALLOW")
            .output()
            .unwrap();
        assert!(
            c2.status.success(),
            "[{name}] native run failed: {}",
            String::from_utf8_lossy(&c2.stderr)
        );

        assert_eq!(norm(&vm.stdout), expected, "[{name}] VM output");
        assert_eq!(norm(&c2.stdout), expected, "[{name}] c2 output");
    }

    let _ = server.join();
    let seen = seen.lock().unwrap();
    assert_eq!(
        seen.len(),
        expected_requests,
        "server saw {} requests, expected {expected_requests}",
        seen.len()
    );
    // Requests arrive in pairs, VM first. Compare each pair byte-for-byte:
    // same request line, same Host, same Connection, same POST headers.
    for (i, (name, _, _)) in cases.iter().enumerate() {
        let vm_req = &seen[i * 2];
        let c2_req = &seen[i * 2 + 1];
        assert_eq!(vm_req, c2_req, "[{name}] requests differ across chains");
        assert!(
            vm_req.starts_with(if *name == "post" { "POST " } else { "GET " }),
            "[{name}] unexpected method: {vm_req:?}"
        );
        assert!(
            vm_req.contains("Connection: close\r\n"),
            "[{name}] missing Connection: close: {vm_req:?}"
        );
        if *name == "post" {
            assert!(
                vm_req.contains("Content-Type: application/json\r\n"),
                "[{name}] missing content type: {vm_req:?}"
            );
            assert!(
                vm_req.contains("Content-Length: 7\r\n") && vm_req.ends_with("{\"a\":1}"),
                "[{name}] bad POST body/length: {vm_req:?}"
            );
        }
    }
}

/// Step 2b, criterion S2 (a security criterion, not a convenience one): an
/// `https://` URL must be refused by *both* chains — never silently downgraded
/// to plaintext on port 443. The listener here proves the refusal happens
/// before any connection is attempted: if either chain opened a socket, the
/// accept below would find it.
#[test]
fn https_never_falls_back_to_plaintext() {
    use std::net::TcpListener;

    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();

    let dir = std::env::temp_dir().join("whisper_https_it");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("tls.ws");
    std::fs::write(&src, format!("\"https://127.0.0.1:{port}/x\" @2 .\n")).unwrap();

    let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["run", "--no-strict-types", "--allow-http"])
        .arg(&src)
        .output()
        .unwrap();
    assert!(!vm.status.success(), "VM must refuse https://");
    assert!(
        String::from_utf8_lossy(&vm.stderr).contains("HTTPS is not yet supported"),
        "VM error should name the cause: {}",
        String::from_utf8_lossy(&vm.stderr)
    );

    let exe = dir.join("tls_exe");
    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "c2", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "c2 build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let artifact = if exe.exists() {
        exe.clone()
    } else {
        exe.with_extension(std::env::consts::EXE_EXTENSION)
    };
    let c2 = std::process::Command::new(&artifact)
        .env("WHISPER_NET", "1")
        .env_remove("WHISPER_NET_ALLOW")
        .output()
        .unwrap();
    // C may attempt TLS (and fail because no TLS server is listening); the
    // invariant is that it never sends plaintext to the HTTPS port.
    assert!(
        !c2.status.success(),
        "C chain must not fall back to plaintext"
    );

    match listener.accept() {
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Ok(_) => panic!("a chain opened a plaintext connection to an https:// URL"),
        Err(e) => panic!("accept failed unexpectedly: {e}"),
    }
}

/// Step 2b, criterion S3: parallel id (9) is not implemented on the
/// C chain, but a program that merely imports `std/http` must still build and
/// run — so it cannot be rejected at compile time. It is defined as a
/// function that fails loudly with the root cause.
/// (@6 is a basic HTTP(S) session; @7 audit; @8 raw body.)
#[test]
fn unimplemented_capabilities_fail_loudly_not_at_compile_time() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_cap_it");
    std::fs::create_dir_all(&dir).unwrap();

    // Merely importing the module must build and run.
    let import_only = "import \"std/http\"\n\"http://127.0.0.1:1/x\" @2 drop 7 .\n";
    let src = dir.join("import_only.ws");
    std::fs::write(&src, import_only).unwrap();
    let exe = dir.join("import_only_exe");
    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "c2", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "importing std/http must still build: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // Calling an unimplemented id must report the root cause, not a gcc
    // `implicit declaration`.
    for (name, source, needle) in [("par", "\"http://127.0.0.1:1/x\" @9 drop\n", "@9")] {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();
        let exe = dir.join(format!("{name}_exe"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&exe)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "[{name}] must build (only the call fails): {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let run = std::process::Command::new(&artifact).output().unwrap();
        assert!(!run.status.success(), "[{name}] should fail at run time");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(
            stderr.contains(needle) && stderr.contains("not implemented on the C chain"),
            "[{name}] unhelpful error: {stderr}"
        );
    }
}

// ── Step 3: the Whisper-written driver `wd` ────────────────────────

/// `wd` is built by `scripts/build-wd.sh`, which takes the *frozen seed* through
/// two self-hosted generations (~15-20 min) — far too slow to run inside a unit
/// test. So this test is conditional: when the artifact is present (a developer
/// or CI job ran the script), exercise it; otherwise skip with a note.
///
/// What it pins (tests/step3_preregistered.md, milestone A):
///   A2 — `wd build` produces an executable, `wd run` forwards its output;
///   A4 — a missing source file is named, not compiled as empty.
///
/// It deliberately does NOT assert A3 (byte-parity with `-t c2`): that needs
/// the import expansion of milestone B, which is not implemented yet.
#[test]
fn wd_driver_compiles_and_runs_when_present() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let plain = repo.join("target/wd/wd");
    let wd = if plain.exists() {
        plain
    } else {
        plain.with_extension(std::env::consts::EXE_EXTENSION)
    };
    if !wd.exists() {
        eprintln!("skipping: build target/wd/wd first (bash scripts/build-wd.sh)");
        return;
    }

    let dir = std::env::temp_dir().join("whisper_wd_it");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("a.ws");
    std::fs::write(&src, "1 2 + .\n").unwrap();
    let out = dir.join("a");
    let norm = |b: &[u8]| String::from_utf8_lossy(b).replace(['\r', '"'], "");

    // A2: build
    let build = std::process::Command::new(&wd)
        .arg("build")
        .arg(&src)
        .arg(&out)
        .current_dir(&repo)
        .env("WHISPER_ROOT", &repo)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "wd build failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let artifact = if out.exists() {
        out.clone()
    } else {
        out.with_extension(std::env::consts::EXE_EXTENSION)
    };
    assert!(
        artifact.exists(),
        "wd build reported success but {artifact:?} does not exist"
    );
    let ran = std::process::Command::new(&artifact).output().unwrap();
    assert_eq!(norm(&ran.stdout), "3\n", "wd-built artifact output");

    // A2: run (the driver forwards the artifact's output)
    let out2 = dir.join("b");
    let run = std::process::Command::new(&wd)
        .arg("run")
        .arg(&src)
        .arg(&out2)
        .current_dir(&repo)
        .env("WHISPER_ROOT", &repo)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "wd run failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert!(
        norm(&run.stdout).contains('3'),
        "wd run should forward the artifact's output, got {:?}",
        norm(&run.stdout)
    );

    // A4: a missing source is named — and, since L-1 gave the language an
    // `exit`, the failure also reaches the shell as a non-zero status.
    let bad = std::process::Command::new(&wd)
        .arg("build")
        .arg(dir.join("nope.ws"))
        .arg(dir.join("nope"))
        .current_dir(&repo)
        .env("WHISPER_ROOT", &repo)
        .output()
        .unwrap();
    let msg = norm(&bad.stdout);
    assert!(
        msg.contains("cannot read source") && msg.contains("nope.ws"),
        "wd should name the missing file, got {msg:?}"
    );
    assert_eq!(
        bad.status.code(),
        Some(1),
        "E4-a: a failed build must not report success to the shell (stderr: {})",
        String::from_utf8_lossy(&bad.stderr)
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
        let (_main, defs) = gen.compile(&ast).unwrap();
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

// ── -t bundle: interpreter-bundled single-file executables ─────────
// NOTE: these embed the Rust VM by design. `-t exe` is the zero-Rust
// native target and is covered separately below.

#[test]
fn bundle_target_runs_and_matches_vm() {
    let ws = std::env::temp_dir().join("whisper_bundle_test");
    std::fs::create_dir_all(&ws).unwrap();
    let src = ws.join("app.ws");
    std::fs::write(
        &src,
        "import \"std/str\"\n\"hello\" \" world\" strcat .\n[1 2 3] { 2 * } @map 2 @nth .\n1.5 2.5 + f64toi64 .\n\"OK\" .\n",
    )
    .unwrap();
    let exe = ws.join("app_bin");

    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "bundle", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build -t bundle failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    let run = std::process::Command::new(&exe).output().unwrap();
    assert!(
        run.status.success(),
        "bundled exe failed: {}",
        String::from_utf8_lossy(&run.stderr)
    );
    let stdout = String::from_utf8_lossy(&run.stdout);
    let expected = "\"hello world\"\n6\n4\n\"OK\"\n";
    assert_eq!(stdout, expected, "bundled exe stdout mismatch");

    // The original CLI binary must NOT carry a trailer (still a normal whisper).
    let normal = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(normal.status.success());
}

#[test]
fn bundle_target_trailer_layout() {
    // Trailer = [payload][u64 len LE][b"WSHPEXE1"]; a plain whisper binary
    // has none, a bundled one does, and the payload decodes as a wbin unit.
    let ws = std::env::temp_dir().join("whisper_bundle_layout");
    std::fs::create_dir_all(&ws).unwrap();
    let src = ws.join("a.ws");
    std::fs::write(&src, "1 .\n").unwrap();
    let exe = ws.join("a_bin");
    std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "bundle", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();

    let data = std::fs::read(&exe).unwrap();
    let n = data.len();
    assert!(n > 16);
    assert_eq!(&data[n - 8..], b"WSHPEXE1");
    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&data[n - 16..n - 8]);
    let plen = u64::from_le_bytes(len_bytes) as usize;
    assert!(plen + 16 <= n);
    assert_eq!(&data[n - 16 - plen..n - 16][..4], b"WHSP");
}

// ── -t exe: native executable, zero Rust in the artifact ───────────

/// The independence pass moved `-t exe` off the interpreter-bundling path
/// (now `-t bundle`) onto the self-hosted C chain. This test pins the three
/// properties that make the artifact "zero Rust":
///   1. it runs the program directly,
///   2. it carries no bundle trailer (so it is not a copy of the whisper CLI),
///   3. the C chain actually ran (a sibling `.c` was emitted).
#[test]
fn exe_target_is_native_and_carries_no_rust() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let ws = std::env::temp_dir().join("whisper_exe_native_it");
    std::fs::create_dir_all(&ws).unwrap();
    let src = ws.join("app.ws");
    std::fs::write(&src, "\"native\" .\n1.5 2.5 + f64toi64 .\n").unwrap();
    let exe = ws.join("app_exe");

    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "exe", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "build -t exe failed: {}",
        String::from_utf8_lossy(&build.stderr)
    );

    // 1. Runs directly. (A bundled CLI would need the payload's own entry
    //    path and would not accept the program's argv semantics.)
    let run = std::process::Command::new(&exe).output().unwrap();
    assert!(run.status.success(), "exe run failed");
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace('\r', ""),
        "native\n4\n"
    );

    // Some compilers append the platform suffix themselves (mingw gcc turns
    // `-o app_exe` into `app_exe.exe`).
    let artifact = if exe.exists() {
        exe.clone()
    } else {
        exe.with_extension(std::env::consts::EXE_EXTENSION)
    };
    assert!(
        artifact.exists(),
        "-t exe produced no artifact at {} nor {}",
        exe.display(),
        artifact.display()
    );

    // 2. No bundle trailer anywhere in the image.
    let data = std::fs::read(&artifact).unwrap();
    let has_trailer = data.windows(8).any(|w| w == b"WSHPEXE1");
    assert!(
        !has_trailer,
        "-t exe artifact must not embed the Rust VM bundle trailer"
    );

    // 3. The self-hosted C chain produced it (sibling .c emitted by build_c2).
    let c_path = exe.with_extension("c");
    assert!(
        c_path.exists(),
        "-t exe should go through the C chain; {} missing",
        c_path.display()
    );
}

// ── -t native: install-time specialization with content cache ──────

#[test]
fn native_target_specializes_and_caches() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let ws = std::env::temp_dir().join("whisper_native_it");
    std::fs::create_dir_all(&ws).unwrap();
    // Unique source per run -> guaranteed cache miss; second build must hit.
    let src = ws.join(format!("n{}.ws", std::process::id()));
    std::fs::write(
        &src,
        "import \"std/math\"\n1.5 2.5 + f64toi64 .\n[1 2 3] { dup * } @map 2 @nth .\n",
    )
    .unwrap();
    let exe = ws.join(format!("n{}.exe", std::process::id()));

    let b1 = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "native", "-o"])
        .arg(&exe)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        b1.status.success(),
        "native build: {}",
        String::from_utf8_lossy(&b1.stderr)
    );
    let b2 = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "native", "-o"])
        .arg(ws.join(format!("n{}b.exe", std::process::id())))
        .arg(&src)
        .output()
        .unwrap();
    assert!(b2.status.success());
    assert!(
        String::from_utf8_lossy(&b2.stdout).contains("cache hit"),
        "second build must be a cache hit: {}",
        String::from_utf8_lossy(&b2.stdout)
    );

    // Polymorphic float arithmetic must match the VM bit-for-bit.
    let run = std::process::Command::new(&exe).output().unwrap();
    assert!(run.status.success());
    // Windows pipes CRLF; compare newline-agnostically.
    assert_eq!(
        String::from_utf8_lossy(&run.stdout).replace('\r', ""),
        "4\n9\n"
    );
}

// ── std/conf: finalized confidence surface ─────────────────────────

#[test]
fn conf_stdlib_surface() {
    let dir = std::env::temp_dir().join("whisper_conf_it");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("conf.ws");
    std::fs::write(
        &src,
        "import \"std/conf\"\n\
         42 seed!\n\
         #t:0.75 conf-of . unwrap .\n\
         0.9 0.5 conf-mul .\n\
         #t:0.7 labeled? .\n\
         42 labeled? .\n\
         0.7 { 1 } { 2 } ?| .\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .arg("run")
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        s.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>(),
        vec!["0.75", "#t", "0.45", "#t", "#f", "1"],
        "confidence surface output mismatch: {s}"
    );
}

// ── std/encoding: base64 / base64url / percent ─────────────────────

#[test]
fn encoding_stdlib_vectors() {
    let dir = std::env::temp_dir().join("whisper_encoding_it");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("enc.ws");
    std::fs::write(
        &src,
        "import \"std/encoding\"\n\
         \"Man\" b64-enc .\n\
         \"M\" b64-enc .\n\
         \"hello world\" b64-enc b64-dec .\n\
         \"TWFu\" b64-dec .\n\
         \"any carnal pleasure.\" b64-enc .\n\
         \"a b&c\" pct-enc .\n\
         \"a%20b%26c\" pct-dec .\n\
         255 ctos 255 ctos strcat dup b64-enc . b64url-enc .\n\
         \"aGk\" b64-dec .\n\
         \"aGVsbG8\" b64-dec .\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .arg("run")
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        s.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>(),
        vec![
            "\"TWFu\"",
            "\"TQ==\"",
            "\"hello world\"",
            "\"Man\"",
            "\"YW55IGNhcm5hbCBwbGVhc3VyZS4=\"",
            "\"%61%20%62%26%63\"",
            "\"a b&c\"",
            "\"//8=\"",
            "\"__8=\"",
            "\"hi\"",
            "\"hello\"",
        ],
        "encoding vectors mismatch: {s}"
    );
}

// ── std/hash: MD5 (RFC 1321) ───────────────────────────────────────

#[test]
fn md5_stdlib_vectors() {
    let dir = std::env::temp_dir().join("whisper_md5_it");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("md5.ws");
    let long_a = "a".repeat(200);
    std::fs::write(
        &src,
        format!(
            "import \"std/hash\"\n\
             \"\" md5 .\n\
             \"abc\" md5 .\n\
             \"The quick brown fox jumps over the lazy dog\" md5 .\n\
             \"message digest\" md5 .\n\
             \"{long_a}\" md5 .\n"
        ),
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .arg("run")
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        s.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>(),
        vec![
            "\"d41d8cd98f00b204e9800998ecf8427e\"",
            "\"900150983cd24fb0d6963f7d28e17f72\"",
            "\"9e107d9d372bb6826bd81d3542a419d6\"",
            "\"f96b697d7cb7938d525a2f31aaf161d0\"",
            "\"887f30b43b2867f4a9accceee7d16e6c\"",
        ],
        "md5 vectors mismatch: {s}"
    );
}

// ── J-4: 模块命名空间（限定量词） ─────────────────────────────────

#[test]
fn namespace_qualified_words() {
    let dir = std::env::temp_dir().join("whisper_ns_it");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("mymod.ws"), ": find { \"MINE\" } ;\nexport find\n").unwrap();
    let src = dir.join("ns.ws");
    std::fs::write(
        &src,
        "import \"std/str\"\n\
         import \"mymod\"\n\
         \"abc\" \"b\" str.find i64tostr .\n\
         mymod.find .\n\
         find .\n",
    )
    .unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .arg("run")
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let s = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        s.lines().filter(|l| !l.is_empty()).collect::<Vec<_>>(),
        vec!["\"1\"", "\"MINE\"", "\"MINE\""],
        "namespace output mismatch: {s}"
    );
}

// ── L-1: `print` and `exit` ─────────────────────────────────────────
// tests/l1_preregistered.md. These two primitives were missing from the
// language itself, not from any program: cgen always emitted `return 0;`
// (so no Whisper CLI could report failure) and the only printing word was
// `.`, which adds a newline and quotes strings differently per chain (so
// nothing could forward a child process's output byte-for-byte).
//
// The assertions below are deliberately byte-level: `Command::output()`
// hands back raw stdout and a real `ExitStatus`, which is exactly what the
// coverage gate's `$( )`-based `t()` helper cannot see.

/// Run `source` on the VM and on the C chain, returning raw stdout bytes.
/// Panics with the compiler's stderr if either build fails.
fn both_chains_stdout(dir: &std::path::Path, name: &str, source: &str) -> (Vec<u8>, Vec<u8>) {
    let src = dir.join(format!("{name}.ws"));
    std::fs::write(&src, source).unwrap();

    let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["run", "--no-strict-types"])
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        vm.status.success(),
        "VM failed on {name} ({source:?}): {}",
        String::from_utf8_lossy(&vm.stderr)
    );

    let out = dir.join(name);
    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "c2", "-o"])
        .arg(&out)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "c2 build failed on {name} ({source:?}): {}",
        String::from_utf8_lossy(&build.stderr)
    );
    let artifact = if out.exists() {
        out.clone()
    } else {
        out.with_extension(std::env::consts::EXE_EXTENSION)
    };
    let c2 = std::process::Command::new(&artifact).output().unwrap();
    assert!(
        c2.status.success(),
        "C chain failed on {name}: {}",
        String::from_utf8_lossy(&c2.stderr)
    );
    (vm.stdout, c2.stdout)
}

/// E1 — `print` is byte plumbing, not formatting. The expected vectors are
/// spelled as bytes on purpose: a `\r` smuggled in by Windows text mode, or
/// a `\n` added by a formatter, is exactly the failure this pins.
#[test]
fn l1_print_is_byte_exact_on_both_chains() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_l1_print");
    std::fs::create_dir_all(&dir).unwrap();

    let cases: &[(&str, &str, &[u8])] = &[
        ("p1", "\"hi\" print\n", b"hi"),
        ("p2", "\"a\" print \"b\" print\n", b"ab"),
        // The literal's own newline survives as a single LF — no CRLF.
        ("p3", "\"x\\n\" print\n", b"x\n"),
        ("p4", "\"\" print\n", b""),
    ];
    for (name, source, want) in cases {
        let (vm, c2) = both_chains_stdout(&dir, name, source);
        assert_eq!(
            vm, *want,
            "VM stdout for {source:?} should be {want:?}, got {vm:?}"
        );
        assert_eq!(
            c2, *want,
            "C-chain stdout for {source:?} should be {want:?}, got {c2:?}"
        );
        assert_eq!(vm, c2, "chains disagree on {source:?}");
    }
}

/// E2 — `exit` reports a real process exit code and really stops. X-3 is the
/// one that matters: a fake `exit` implemented as "return to main" would
/// still print the `1` after the call.
#[test]
fn l1_exit_sets_a_real_exit_code() {
    if std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_err()
    {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_l1_exit");
    std::fs::create_dir_all(&dir).unwrap();

    let cases: &[(&str, &str, i32)] = &[
        ("x1", "0 exit\n", 0),
        ("x2", "3 exit\n", 3),
        ("x3", ": f { 7 exit } ;\nf 1 .\n", 7),
    ];
    for (name, source, want) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["run", "--no-strict-types"])
            .arg(&src)
            .output()
            .unwrap();
        assert_eq!(
            vm.status.code(),
            Some(*want),
            "VM exit code for {source:?} (stderr: {})",
            String::from_utf8_lossy(&vm.stderr)
        );
        assert!(
            vm.stdout.is_empty(),
            "X-3: `exit` must terminate, so nothing after it prints; got {:?}",
            vm.stdout
        );

        let out = dir.join(name);
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&out)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "c2 build failed on {source:?}: {}",
            String::from_utf8_lossy(&build.stderr)
        );
        let artifact = if out.exists() {
            out.clone()
        } else {
            out.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let c2 = std::process::Command::new(&artifact).output().unwrap();
        assert_eq!(
            c2.status.code(),
            Some(*want),
            "C-chain exit code for {source:?}"
        );
        assert!(
            c2.stdout.is_empty(),
            "X-3 (C chain): nothing may print after `exit`; got {:?}",
            c2.stdout
        );
    }
}

// ── 收口战役 W1/W2/W4/W6 ────────────────────────────────────────────
// tests/closeout_preregistered.md.

fn gcc_available() -> bool {
    std::process::Command::new("gcc")
        .arg("--version")
        .output()
        .is_ok()
}

/// W1 — a CRLF source must not behave differently from an LF one, and must
/// not behave differently depending on which path compiled it. Before the
/// fix, `"a\r\nb" strlen .` printed 4 under `whisper run` but 3 under
/// `-t c2` (whose import expansion goes through `str::lines()`) and 3 under
/// `wd run` — the `\r` leaked into the program's *data*, not just its layout.
#[test]
fn w1_crlf_source_is_normalized_on_both_paths() {
    if !gcc_available() {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_w1_crlf");
    std::fs::create_dir_all(&dir).unwrap();
    // `"a<CRLF>b" strlen .` — written with explicit bytes, not a Rust literal.
    let src = dir.join("crlf.ws");
    std::fs::write(&src, b"\"a\r\nb\" strlen .\r\n").unwrap();

    let vm = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["run", "--no-strict-types"])
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        vm.status.success(),
        "{}",
        String::from_utf8_lossy(&vm.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&vm.stdout).trim(),
        "3",
        "VM must collapse the CRLF inside the literal"
    );

    let out = dir.join("crlf");
    let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["build", "-t", "c2", "-o"])
        .arg(&out)
        .arg(&src)
        .output()
        .unwrap();
    assert!(
        build.status.success(),
        "{}",
        String::from_utf8_lossy(&build.stderr)
    );
    let artifact = if out.exists() {
        out.clone()
    } else {
        out.with_extension(std::env::consts::EXE_EXTENSION)
    };
    let c2 = std::process::Command::new(&artifact).output().unwrap();
    assert_eq!(
        String::from_utf8_lossy(&c2.stdout).trim(),
        "3",
        "the C chain must agree with the VM"
    );
}

/// W4 — `-t c` used to emit `// cap_exec: no-op in native mode` for both
/// capability opcodes, so a generated program silently did *less* than its
/// source said. The runtime already had `W_call` and the `W__<n>` ABI.
#[test]
fn w4_c_transpiler_executes_call_and_capabilities() {
    if !gcc_available() {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_w4_native_caps");
    std::fs::create_dir_all(&dir).unwrap();

    for (name, source, want) in [
        ("q", "{ 42 } call .\n", "42"),
        ("qi", "10 { 2 * } call .\n", "20"),
    ] {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();
        let cfile = dir.join(format!("{name}.c"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c", "-o"])
            .arg(&cfile)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        let text = std::fs::read_to_string(&cfile).unwrap();
        assert!(
            !text.contains("no-op in native mode"),
            "-t c must not drop capability opcodes: {text}"
        );

        let exe = dir.join(format!("{name}.exe"));
        let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut cc = std::process::Command::new("gcc");
        cc.args(["-O2", "-w", "-o"])
            .arg(&exe)
            .arg(&cfile)
            .arg("-I")
            .arg(repo.join("whisperc"))
            .arg("-lm");
        if cfg!(windows) {
            cc.arg("-lws2_32");
        }
        let cc = cc.output().unwrap();
        assert!(
            cc.status.success(),
            "{}",
            String::from_utf8_lossy(&cc.stderr)
        );
        let artifact = if exe.exists() {
            exe.clone()
        } else {
            exe.with_extension(std::env::consts::EXE_EXTENSION)
        };
        let run = std::process::Command::new(&artifact).output().unwrap();
        assert_eq!(
            String::from_utf8_lossy(&run.stdout).trim(),
            want,
            "-t c output for {source:?}"
        );
    }
}

/// W6 — `@10` (exec-raw) is `@5` without the trim, so byte-exact forwarding
/// is possible without changing what `@5` means. Both ids are granted
/// together: raw output is a different *shape*, not a broader permission.
#[test]
fn w6_exec_raw_is_verbatim_and_needs_the_same_grant() {
    let dir = std::env::temp_dir().join("whisper_w6_execraw");
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("len.ws");
    // `echo x` yields "x\r\n" on Windows and "x\n" elsewhere — one byte of
    // trailing newline either way, which is exactly what `@5` removes.
    std::fs::write(
        &src,
        "\"echo x\" @10 1 @nth strlen .\n\"echo x\" @5 1 @nth strlen .\n",
    )
    .unwrap();

    let run = |args: &[&str]| {
        std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(args)
            .arg(&src)
            .output()
            .unwrap()
    };

    let denied = run(&["run", "--no-strict-types"]);
    assert!(
        !denied.status.success(),
        "@10 must not work without a grant"
    );
    assert!(
        String::from_utf8_lossy(&denied.stderr).contains("Capability not bound"),
        "denial should name the missing binding: {}",
        String::from_utf8_lossy(&denied.stderr)
    );

    let allowed = run(&["run", "--no-strict-types", "--allow-exec"]);
    assert!(
        allowed.status.success(),
        "{}",
        String::from_utf8_lossy(&allowed.stderr)
    );
    let out = String::from_utf8_lossy(&allowed.stdout);
    let vals: Vec<&str> = out.split_whitespace().collect();
    assert_eq!(vals.len(), 2, "expected two lengths, got {out:?}");
    let raw: usize = vals[0].parse().unwrap();
    let trimmed: usize = vals[1].parse().unwrap();
    let raw_expected = if cfg!(windows) { 3 } else { 2 };
    assert_eq!(
        raw, raw_expected,
        "@10 should keep the child's trailing newline: {out:?}"
    );
    assert_eq!(trimmed, 1, "@5 must keep trimming: {out:?}");
}

/// W2 — the driver's generated C must be byte-identical to `-t c2`'s, for
/// self-contained *and* importing programs. The last gap was multi-line
/// definitions: only single-line ones were getting qualified clones, so
/// `math.clamp` (whose body spans lines) had no `W_math_clamp`.
#[test]
fn w2_wd_output_matches_c2_byte_for_byte() {
    if !gcc_available() {
        eprintln!("skipping: gcc not available");
        return;
    }
    let repo = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let plain = repo.join("target/wd/wd");
    let wd = if plain.exists() {
        plain
    } else {
        plain.with_extension(std::env::consts::EXE_EXTENSION)
    };
    if !wd.exists() {
        eprintln!("skipping: build target/wd/wd first (bash scripts/build-wd.sh)");
        return;
    }

    let dir = std::env::temp_dir().join("whisper_w2_parity");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, source) in [
        ("self", "1 2 + .\n"),
        ("imp", "import \"std/math\"\n5 sq .\n"),
        ("multi", "import \"std/math\"\n5 0 10 math.clamp .\n"),
        ("list", "import \"std/list\"\n[1 2 3] len .\n"),
    ] {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let wd_out = dir.join(format!("wd_{name}"));
        let b = std::process::Command::new(&wd)
            .arg("build")
            .arg(&src)
            .arg(&wd_out)
            .current_dir(&repo)
            .env("WHISPER_ROOT", &repo)
            .output()
            .unwrap();
        assert!(
            b.status.success(),
            "wd build {name}: {}",
            String::from_utf8_lossy(&b.stderr)
        );

        let c2_out = dir.join(format!("c2_{name}"));
        let b2 = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&c2_out)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            b2.status.success(),
            "c2 build {name}: {}",
            String::from_utf8_lossy(&b2.stderr)
        );

        let a = std::fs::read(dir.join(format!("wd_{name}.c"))).unwrap();
        let b = std::fs::read(dir.join(format!("c2_{name}.c"))).unwrap();
        assert_eq!(
            a.len(),
            b.len(),
            "[{name}] wd and -t c2 produced different C sizes"
        );
        assert!(a == b, "[{name}] wd and -t c2 C output differs");
    }
}

/// D-1 (收口战役第二轮) — `whisper bootstrap` 与 `whisper build -t c2` 必须
/// 是**同一条流水线**。曾经这里挂着第二棵编译器（顶层 `whisperc/*.ws` +
/// `src/compile.ws`），它与权威编译器各写一遍同一条流水线，而两者之间的漂移
/// 没有任何机制把关。现在那棵树已删除，本测试逐字节钉住"只有一份"。
///
/// 注意比的是**生成的 C 文件**：只看退出码不够——被删掉的那棵树在简单程序上
/// 退出码同样是 0。
#[test]
fn bootstrap_matches_c2_byte_for_byte() {
    let dir = std::env::temp_dir().join("whisper_d1_bootstrap");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let cases: &[(&str, &str)] = &[
        ("arith", "3 4 + 5 * .\n"),
        ("def", ": sq { dup * } ;\n5 sq .\n"),
        ("quote", "{ 42 } call .\n"),
        ("hof", "[1 2 3] { 2 * } @map . drop\n"),
        ("import", "import \"std/math\"\n5 sq .\n"),
        ("print", "\"hi\" print\n"),
        ("multiline", "\"a\nb\" strlen .\n"),
        ("crlf", "\"x\r\ny\" strlen .\n"),
    ];

    for (name, source) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();

        let boot = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .arg("bootstrap")
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            boot.status.success(),
            "bootstrap failed on {name}: {}",
            String::from_utf8_lossy(&boot.stderr)
        );

        let out = dir.join(format!("{name}_c2"));
        let build = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
            .args(["build", "-t", "c2", "-o"])
            .arg(&out)
            .arg(&src)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "c2 build failed on {name}: {}",
            String::from_utf8_lossy(&build.stderr)
        );

        let from_c2 = std::fs::read(dir.join(format!("{name}_c2.c"))).unwrap();
        assert_eq!(
            boot.stdout.len(),
            from_c2.len(),
            "[{name}] bootstrap and -t c2 emitted different C sizes"
        );
        assert!(
            boot.stdout == from_c2,
            "[{name}] bootstrap and -t c2 emitted different C"
        );
    }
}

/// `--hard` 是历史别名（CI 曾用它），必须仍然接受且行为相同。
#[test]
fn bootstrap_hard_flag_is_a_legacy_alias() {
    let dir = std::env::temp_dir().join("whisper_d1_hard");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let src = dir.join("p.ws");
    std::fs::write(&src, "import \"std/math\"\n5 sq .\n").unwrap();

    let plain = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .arg("bootstrap")
        .arg(&src)
        .output()
        .unwrap();
    let hard = std::process::Command::new(env!("CARGO_BIN_EXE_whisper"))
        .args(["bootstrap", "--hard"])
        .arg(&src)
        .output()
        .unwrap();
    assert!(plain.status.success() && hard.status.success());
    assert_eq!(
        plain.stdout, hard.stdout,
        "--hard must not change behaviour"
    );
}

/// S-1 — 非 ASCII 的**字节语义**（`tests/s1_preregistered.md`）。
///
/// spec §3.5 与 `stdlib/encoding.ws`/`hash.ws` 的注释都写着字节语义，C 链也
/// 一直是字节的；只有 VM 的 `strchars`/`charsstr`/`striter` 按**码点**。
/// 后果不止是"两链不一致"：自举 lexer 的 `read-str-acc` 用 `striter` 前进、
/// 用 `strnth`+`ctos` 取字节，于是 `-t c2` 把 `"你好"` 在**编译期**削成两个
/// 字节 `E4 E5`——含非 ASCII 字面量的程序被静默写坏。
///
/// 这里比的是**内容**：字符串结果一律经 `strchars` 转成数字列表，绕开
/// "VM 打印字符串带引号、且对非法 UTF-8 做 lossy"这条既有显示约定。
#[test]
fn s1_non_ascii_uses_byte_semantics_on_both_chains() {
    if !gcc_available() {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_s1_bytes");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    // 每条都产出**同一种形状**（数字或数字列表），两链应当逐字节相同。
    let cases: &[(&str, &str)] = &[
        ("strlen", "\"你好ab\" strlen .\n"),
        ("nth", "\"你好ab\" 1 strnth .\n"),
        ("chars", "\"你好ab\" strchars .\n"),
        ("slice", "\"你好ab\" 0 2 strslice strchars .\n"),
        ("slice-mid", "\"你好ab\" 2 2 strslice strchars .\n"),
        ("iter-rest", "\"你好ab\" striter swap drop strchars .\n"),
        ("roundtrip", "\"你好ab\" strchars charsstr strchars .\n"),
        ("find", "\"你好ab\" \"好\" strfind .\n"),
        ("concat", "\"你\" \"好ab\" strcat strchars .\n"),
        ("emoji", "\"a🎉b\" strchars .\n"),
        ("emoji-len", "\"a🎉b\" strlen .\n"),
    ];

    for (name, source) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();
        let (vm, c2) = both_chains_stdout(&dir, name, source);
        assert_eq!(
            vm,
            c2,
            "[{name}] chains disagree on {source:?}\n  VM={:?}\n  C ={:?}",
            String::from_utf8_lossy(&vm),
            String::from_utf8_lossy(&c2)
        );
    }

    // 字面量本身必须活下来：`"你好"` 是 **6** 字节，不是 2。
    let (vm, c2) = both_chains_stdout(&dir, "lit", "\"你好\" strlen .\n");
    assert_eq!(String::from_utf8_lossy(&vm).trim(), "6");
    assert_eq!(String::from_utf8_lossy(&c2).trim(), "6");
}

/// S-1（S1-d）—— stdlib 的**字节管线**两链同值。这些词直接建在
/// `strchars`/`charsstr` 上，是这次分叉影响面最广的地方。
#[test]
fn s1_stdlib_byte_pipeline_matches_across_chains() {
    if !gcc_available() {
        eprintln!("skipping: gcc not available");
        return;
    }
    let dir = std::env::temp_dir().join("whisper_s1_stdlib");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let cases: &[(&str, &str)] = &[
        ("rev", "import \"std/str\"\n\"你好ab\" str-rev strchars .\n"),
        ("upper", "import \"std/str\"\n\"a你b\" upper strchars .\n"),
        ("lower", "import \"std/str\"\n\"A你B\" lower strchars .\n"),
        (
            "urldecode",
            "import \"std/net\"\n\"%E4%BD%A0\" url-decode strchars .\n",
        ),
        // 这里**故意不放** `import "std/hash"` + `md5`：它正被 S-3 挡着
        // （hash.ws 用了 std/list 的 range-to 却没 import 它，C 链编不出）。
        // 按项目规矩不往通过的测试里塞已知失败的用例；S-3 修好后再补回来。
    ];

    for (name, source) in cases {
        let src = dir.join(format!("{name}.ws"));
        std::fs::write(&src, source).unwrap();
        let (vm, c2) = both_chains_stdout(&dir, name, source);
        assert_eq!(
            vm,
            c2,
            "[{name}] chains disagree on {source:?}\n  VM={:?}\n  C ={:?}",
            String::from_utf8_lossy(&vm),
            String::from_utf8_lossy(&c2)
        );
    }
}
