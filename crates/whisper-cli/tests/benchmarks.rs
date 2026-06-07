// Performance benchmarks for the Whisper VM.
// Not run by default — use: cargo test --test benchmarks --release -- --nocapture

use std::rc::Rc;
use std::time::Instant;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;

fn bench(name: &str, mut f: impl FnMut() -> u64) {
    // Warmup
    f();
    let start = Instant::now();
    let iterations = 100_000;
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    let ns_per_op = elapsed.as_nanos() as f64 / iterations as f64;
    println!("  {name:30} {ns_per_op:8.0} ns/op");
}

fn make_vm() -> Vm {
    let mut vm = Vm::new();
    vm.define_word("sq".into(), vec![Opcode::Dup, Opcode::Mul, Opcode::Return]);
    vm.define_word(
        "fib".into(),
        vec![
            Opcode::Dup,
            Opcode::PushI64(1),
            Opcode::Gt,
            Opcode::Cond(11), // if false, skip to Return
            Opcode::Dup,
            Opcode::PushI64(1),
            Opcode::Sub,
            Opcode::Call("fib".into()),
            Opcode::Swap,
            Opcode::PushI64(2),
            Opcode::Sub,
            Opcode::Call("fib".into()),
            Opcode::Add,
            Opcode::Return,
            Opcode::Return,
        ],
    );
    vm
}

#[test]
fn bench_arithmetic() {
    println!("\n=== Arithmetic ===");
    bench("push_i64", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(42));
        42
    });
    bench("add", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(3));
        vm.data_stack.push(Value::I64(4));
        vm.execute(&[Opcode::Add]).unwrap();
        7
    });
    bench("mul", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(6));
        vm.data_stack.push(Value::I64(7));
        vm.execute(&[Opcode::Mul]).unwrap();
        42
    });
    bench("complex (3+4)*2", || {
        let mut vm = Vm::new();
        vm.execute(&[
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Add,
            Opcode::PushI64(2),
            Opcode::Mul,
        ])
        .unwrap();
        14
    });
}

#[test]
fn bench_stack_ops() {
    println!("\n=== Stack Ops ===");
    bench("dup", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(5));
        vm.execute(&[Opcode::Dup]).unwrap();
        5
    });
    bench("swap", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(3));
        vm.data_stack.push(Value::I64(4));
        vm.execute(&[Opcode::Swap]).unwrap();
        3
    });
    bench("dup + mul", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(5));
        vm.execute(&[Opcode::Dup, Opcode::Mul]).unwrap();
        25
    });
}

#[test]
fn bench_string_ops() {
    println!("\n=== String Ops ===");
    bench("strlen", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("hello".into())));
        vm.execute(&[Opcode::StrLen]).unwrap();
        5
    });
    bench("strcat", || {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("hello".into())));
        vm.data_stack.push(Value::Str(Rc::new("world".into())));
        vm.execute(&[Opcode::StrCat]).unwrap();
        10
    });
}

#[test]
fn bench_word_calls() {
    println!("\n=== Word Calls ===");
    bench("call sq(5)", || {
        let mut vm = make_vm();
        vm.data_stack.push(Value::I64(5));
        vm.execute(&[Opcode::Call("sq".into())]).unwrap();
        25
    });
    bench("fib(10)", || {
        let mut vm = make_vm();
        vm.data_stack.push(Value::I64(10));
        vm.execute(&[Opcode::Call("fib".into())]).unwrap();
        55
    });
}

#[test]
fn bench_list_ops() {
    println!("\n=== List Ops ===");
    let list = Value::List(Rc::new(vec![
        Value::I64(1),
        Value::I64(2),
        Value::I64(3),
        Value::I64(4),
        Value::I64(5),
    ]));
    bench("len [1..5]", || {
        let mut vm = Vm::new();
        vm.data_stack.push(list.clone());
        vm.execute(&[Opcode::Len]).unwrap();
        5
    });
}

#[test]
fn bench_throughput() {
    println!("\n=== Throughput ===");
    let start = Instant::now();
    let n = 1_000_000;
    for _ in 0..n {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(1));
        vm.data_stack.push(Value::I64(2));
        vm.execute(&[Opcode::Add]).unwrap();
    }
    let elapsed = start.elapsed();
    let ops_per_sec = n as f64 / elapsed.as_secs_f64();
    println!("  simple add loop: {ops_per_sec:.0} ops/sec ({n} iterations in {elapsed:?})");
}
