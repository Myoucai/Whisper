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

// ── Campaign O: multiple quotation literals ────────────────────────

/// Two or more non-fused quotation literals used to break the self-hosted
/// compiler. `genq-name` re-read its counter from the KV store and left the
/// original copy on the stack — invisible with one quote (the true branch
/// drops it), but every extra quote piled up another stray string and pushed
/// the real operands out of reach ("Type mismatch: expected [T], got str").
///
/// Quotations nested in a *list literal* are still not supported by the c2
/// chain (root cause recorded in docs/language-backlog.md, campaign O');
/// this test therefore only pins the bare-multi-quote forms.
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

    let cases = [("pair", "{ 1 + } { 2 * } drop drop 7 .\n", "7\n")];
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
