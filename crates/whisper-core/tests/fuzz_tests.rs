/// Fuzz testing for the Whisper VM.
/// Generates random opcode sequences and verifies the VM handles them gracefully
/// (no panics, only returns Err or Ok with reasonable stack state).
use std::rc::Rc;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_core::vm::Vm;

/// Deterministic pseudo-random i64.
fn rand_i64(seed: &mut u64) -> i64 {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    *seed as i64
}

/// Generate a random opcode, excluding IO and unsafe control-flow ops.
fn random_op_safe(seed: &mut u64) -> Opcode {
    let n = rand_i64(seed).unsigned_abs() % 32;
    match n {
        0 => Opcode::Dup,
        1 => Opcode::Swap,
        2 => Opcode::Drop,
        3 => Opcode::Rot,
        4 => Opcode::Pick((rand_i64(seed).unsigned_abs() % 10) as u8),
        5 => Opcode::Add,
        6 => Opcode::Sub,
        7 => Opcode::Mul,
        8 => Opcode::Div,
        9 => Opcode::Mod,
        10 => Opcode::Eq,
        11 => Opcode::Lt,
        12 => Opcode::Gt,
        13 => Opcode::Neq,
        14 => Opcode::Le,
        15 => Opcode::Ge,
        16 => Opcode::And,
        17 => Opcode::Or,
        18 => Opcode::Not,
        19 => Opcode::PushI64(rand_i64(seed) % 1000),
        20 => Opcode::PushF64((rand_i64(seed) % 1000) as f64),
        21 => Opcode::PushBool(rand_i64(seed) % 2 == 0),
        22 => Opcode::PushStr(Rc::from(format!("s{}", rand_i64(seed) % 100))),
        23 => Opcode::Nth,
        24 => Opcode::Append,
        25 => Opcode::Len,
        26 => Opcode::PushList,
        27 => Opcode::Map,
        28 => Opcode::Each,
        29 => Opcode::Fold,
        30 => Opcode::Return,
        _ => Opcode::Times,
    }
}

/// Generate a random opcode including control-flow ops (but still excluding IO).
fn random_op_with_cf(seed: &mut u64) -> Opcode {
    let n = rand_i64(seed).unsigned_abs() % 36;
    match n {
        // Safe ops 0-31 (same as random_op_safe)
        0 => Opcode::Dup,
        1 => Opcode::Swap,
        2 => Opcode::Drop,
        3 => Opcode::Rot,
        4 => Opcode::Pick((rand_i64(seed).unsigned_abs() % 10) as u8),
        5 => Opcode::Add,
        6 => Opcode::Sub,
        7 => Opcode::Mul,
        8 => Opcode::Div,
        9 => Opcode::Mod,
        10 => Opcode::Eq,
        11 => Opcode::Lt,
        12 => Opcode::Gt,
        13 => Opcode::Neq,
        14 => Opcode::Le,
        15 => Opcode::Ge,
        16 => Opcode::And,
        17 => Opcode::Or,
        18 => Opcode::Not,
        19 => Opcode::PushI64(rand_i64(seed) % 1000),
        20 => Opcode::PushF64((rand_i64(seed) % 1000) as f64),
        21 => Opcode::PushBool(rand_i64(seed) % 2 == 0),
        22 => Opcode::PushStr(Rc::from(format!("s{}", rand_i64(seed) % 100))),
        23 => Opcode::Nth,
        24 => Opcode::Append,
        25 => Opcode::Len,
        26 => Opcode::PushList,
        27 => Opcode::Map,
        28 => Opcode::Each,
        29 => Opcode::Fold,
        30 => Opcode::Return,
        31 => Opcode::Times,
        // Control flow — use bounded offsets to avoid infinite loops
        32 => Opcode::Cond((rand_i64(seed) % 6 - 1) as i32),
        33 => Opcode::Jump((rand_i64(seed) % 6) as i32),
        34 => Opcode::Loop((rand_i64(seed) % 5 - 3) as i32),
        _ => Opcode::ConfLabel((rand_i64(seed).unsigned_abs() % 100) as f64 / 100.0),
    }
}

/// Generate a random program of given length.
fn random_program(len: usize, seed: u64) -> Vec<Opcode> {
    let mut s = seed;
    (0..len).map(|_| random_op_safe(&mut s)).collect()
}

#[test]
fn fuzz_small_programs() {
    for seed in 0..50 {
        let prog = random_program(5, seed);
        let mut vm = Vm::new();
        let _ = vm.execute(&prog); // Must not panic
    }
}

#[test]
fn fuzz_medium_programs() {
    for seed in 0..30 {
        let prog = random_program(20, seed);
        let mut vm = Vm::new();
        let _ = vm.execute(&prog);
    }
}

#[test]
fn fuzz_list_operations() {
    // Random list operations with pre-pushed list
    for seed in 0..100 {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::List(std::rc::Rc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
        ])));
        let prog = random_program(10, seed);
        let _ = vm.execute(&prog);
    }
}

#[test]
fn fuzz_arithmetic_only() {
    // Push several values, then random arithmetic
    for seed in 0..100 {
        let mut s1 = seed + 1000;
        let mut s2 = seed + 2000;
        let mut s3 = seed + 3000;
        let mut ops = vec![
            Opcode::PushI64(rand_i64(&mut s1) % 100),
            Opcode::PushI64(rand_i64(&mut s2) % 100),
            Opcode::PushI64(rand_i64(&mut s3) % 100),
        ];
        ops.extend(random_program(10, seed + 4000));
        let mut vm = Vm::new();
        let _ = vm.execute(&ops);
    }
}

#[test]
fn fuzz_division_by_zero() {
    // Ensure division by zero returns error, not panic
    let mut vm = Vm::new();
    let prog = [Opcode::PushI64(42), Opcode::PushI64(0), Opcode::Div];
    assert!(vm.execute(&prog).is_err());
}

#[test]
fn fuzz_stack_underflow() {
    // Operations on empty stack should return error
    for op in [
        Opcode::Add,
        Opcode::Sub,
        Opcode::Mul,
        Opcode::Div,
        Opcode::Dup,
        Opcode::Swap,
        Opcode::Drop,
    ] {
        let mut vm = Vm::new();
        assert!(
            vm.execute(&[op.clone()]).is_err(),
            "Op {:?} on empty stack should error",
            op
        );
    }
}

#[test]
fn fuzz_control_flow() {
    // Random control flow should not panic
    for mut seed in 0..50 {
        let mut vm = Vm::new();
        vm.data_stack
            .push(Value::Bool(rand_i64(&mut seed) % 2 == 0));
        let mut s = seed + 5000;
        let prog: Vec<Opcode> = (0..10).map(|_| random_op_with_cf(&mut s)).collect();
        let _ = vm.execute(&prog);
    }
}
