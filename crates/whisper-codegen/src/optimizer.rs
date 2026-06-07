//! Bytecode optimizer: constant folding, peephole, dead code, strength reduction.
//!
//! Runs after bytecode generation, before execution.
//! Each pass scans the opcode sequence and applies transformations.

use whisper_core::opcode::Opcode;

/// Optimize a bytecode sequence. Returns optimized bytecode.
pub fn optimize(bytecode: &[Opcode]) -> Vec<Opcode> {
    let mut ops = bytecode.to_vec();

    // Run optimization passes until convergence (max 4 iterations)
    for _ in 0..4 {
        let before = ops.len();
        ops = constant_folding(&ops);
        ops = peephole(&ops);
        ops = strength_reduction(&ops);
        ops = dead_store_elimination(&ops);
        ops = optimize_refs(&ops);
        if ops.len() == before {
            break; // No further reduction
        }
    }

    ops
}

// ── Constant folding ──────────────────────────────────────────────────

/// Evaluate constant expressions at compile time.
///
/// Patterns:
///   PushI64(a) PushI64(b) BinOp  → PushResult
///   PushF64(a) PushF64(b) FBinOp → PushResult
fn constant_folding(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        if i + 2 < ops.len() {
            // i64 binary ops
            if let (Opcode::PushI64(a), Opcode::PushI64(b), binop) =
                (&ops[i], &ops[i + 1], &ops[i + 2])
            {
                let folded = fold_i64_binary(*a, *b, binop);
                if let Some(op) = folded {
                    result.push(op);
                    i += 3;
                    continue;
                }
            }
        }

        // Single-arg folding: PushF64(x) FSqrt → PushF64(sqrt(x))
        if i + 1 < ops.len() {
            if let (Opcode::PushF64(x), Opcode::FSqrt) = (&ops[i], &ops[i + 1]) {
                result.push(Opcode::PushF64(x.sqrt()));
                i += 2;
                continue;
            }
            if let (Opcode::PushF64(x), Opcode::FSin) = (&ops[i], &ops[i + 1]) {
                result.push(Opcode::PushF64(x.sin()));
                i += 2;
                continue;
            }
            if let (Opcode::PushF64(x), Opcode::FCos) = (&ops[i], &ops[i + 1]) {
                result.push(Opcode::PushF64(x.cos()));
                i += 2;
                continue;
            }
            if let (Opcode::PushF64(x), Opcode::FTan) = (&ops[i], &ops[i + 1]) {
                result.push(Opcode::PushF64(x.tan()));
                i += 2;
                continue;
            }
            // PushI64(x) I64ToStr → PushStr(x.to_string())
            if let (Opcode::PushI64(x), Opcode::I64ToStr) = (&ops[i], &ops[i + 1]) {
                result.push(Opcode::PushStr(x.to_string()));
                i += 2;
                continue;
            }
            // PushStr(s) StrLen → PushI64(s.len())
            if let (Opcode::PushStr(s), Opcode::StrLen) = (&ops[i], &ops[i + 1]) {
                result.push(Opcode::PushI64(s.len() as i64));
                i += 2;
                continue;
            }
            // PushStr(s) StrToI64 → PushI64(parsed)
            if let (Opcode::PushStr(s), Opcode::StrToI64) = (&ops[i], &ops[i + 1]) {
                if let Ok(n) = s.parse::<i64>() {
                    result.push(Opcode::PushI64(n));
                    i += 2;
                    continue;
                }
            }
        }

        result.push(ops[i].clone());
        i += 1;
    }

    result
}

fn fold_i64_binary(a: i64, b: i64, binop: &Opcode) -> Option<Opcode> {
    match binop {
        Opcode::Add => Some(Opcode::PushI64(a.wrapping_add(b))),
        Opcode::Sub => Some(Opcode::PushI64(a.wrapping_sub(b))),
        Opcode::Mul => Some(Opcode::PushI64(a.wrapping_mul(b))),
        Opcode::Div if b != 0 => Some(Opcode::PushI64(a.wrapping_div(b))),
        Opcode::Mod if b != 0 => Some(Opcode::PushI64(a.wrapping_rem(b))),
        Opcode::Eq => Some(Opcode::PushBool(a == b)),
        Opcode::Lt => Some(Opcode::PushBool(a < b)),
        Opcode::Gt => Some(Opcode::PushBool(a > b)),
        Opcode::Neq => Some(Opcode::PushBool(a != b)),
        Opcode::Le => Some(Opcode::PushBool(a <= b)),
        Opcode::Ge => Some(Opcode::PushBool(a >= b)),
        _ => None,
    }
}

// ── Peephole ──────────────────────────────────────────────────────────

/// Remove redundant opcode sequences.
///
/// Patterns:
///   Dup Drop           → (nothing)
///   Swap Swap          → (nothing)
///   Rot Rot Rot        → (nothing)
///   Pick(0)            → Dup
///   PushI64(0) Add/Sub → (nothing)
///   PushI64(1) Mul     → (nothing)
///   Not Not            → (nothing)
///   Jump(0)            → (nothing)
fn peephole(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        if i + 1 < ops.len() {
            match (&ops[i], &ops[i + 1]) {
                // Dup + Drop → remove both
                (Opcode::Dup, Opcode::Drop) => {
                    i += 2;
                    continue;
                }
                // Swap + Swap → remove both
                (Opcode::Swap, Opcode::Swap) => {
                    i += 2;
                    continue;
                }
                // PushI64(0) + Add → no-op
                (Opcode::PushI64(0), Opcode::Add) => {
                    i += 2;
                    continue;
                }
                // PushI64(0) + Sub → no-op
                (Opcode::PushI64(0), Opcode::Sub) => {
                    i += 2;
                    continue;
                }
                // PushI64(1) + Mul → no-op
                (Opcode::PushI64(1), Opcode::Mul) => {
                    i += 2;
                    continue;
                }
                // Not + Not → remove double negation
                (Opcode::Not, Opcode::Not) => {
                    i += 2;
                    continue;
                }
                // PushBool(x) + Drop → no-op
                (Opcode::PushBool(_), Opcode::Drop) => {
                    i += 2;
                    continue;
                }
                // Jump(0) → no-op (skip the Jump, keep the next op)
                (Opcode::Jump(0), _) => {
                    result.push(ops[i + 1].clone());
                    i += 2;
                    continue;
                }
                _ => {}
            }

            // Dup + Dup → Dup + Pick(1) (save stack space)
            if ops[i] == Opcode::Dup && ops[i + 1] == Opcode::Dup {
                result.push(Opcode::Dup);
                result.push(Opcode::Pick(1));
                i += 2;
                continue;
            }
        }

        // Rot + Rot + Rot → no-op (identity on 3 elements)
        if i + 2 < ops.len()
            && ops[i] == Opcode::Rot
            && ops[i + 1] == Opcode::Rot
            && ops[i + 2] == Opcode::Rot
        {
            i += 3;
            continue;
        }

        // Pick(0) → Dup (canonical form)
        if let Opcode::Pick(0) = ops[i] {
            result.push(Opcode::Dup);
            i += 1;
            continue;
        }

        result.push(ops[i].clone());
        i += 1;
    }

    result
}

// ── Strength reduction ────────────────────────────────────────────────

/// Replace expensive ops with cheaper equivalents.
///
/// Patterns:
///   PushI64(N) Add where N<0  → PushI64(-N) Sub
///   PushI64(2) Mul            → Dup Add
fn strength_reduction(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        if i + 1 < ops.len() {
            match (&ops[i], &ops[i + 1]) {
                // PushI64(N) Add with N < 0 → PushI64(-N) Sub
                (Opcode::PushI64(n), Opcode::Add) if *n < 0 => {
                    result.push(Opcode::PushI64(-n));
                    result.push(Opcode::Sub);
                    i += 2;
                    continue;
                }
                // PushI64(2) Mul → Dup Add (dup is often cheaper than const+multiply)
                (Opcode::PushI64(2), Opcode::Mul) => {
                    result.push(Opcode::Dup);
                    result.push(Opcode::Add);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        result.push(ops[i].clone());
        i += 1;
    }

    result
}

// ── Dead store elimination ────────────────────────────────────────────

/// Remove literal pushes immediately followed by Drop.
///
/// Patterns:
///   PushI64(x) Drop     → (nothing)
///   PushF64(x) Drop     → (nothing)
///   PushStr(s) Drop     → (nothing)
///   PushBool(b) Drop    → (nothing)  (already in peephole)
fn dead_store_elimination(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        if i + 1 < ops.len() && ops[i + 1] == Opcode::Drop {
            match &ops[i] {
                Opcode::PushI64(_) | Opcode::PushF64(_) | Opcode::PushStr(_) => {
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }

        result.push(ops[i].clone());
        i += 1;
    }

    result
}

// ── Recursive ref optimization ────────────────────────────────────────

/// Recursively optimize bytecode inside PushRef blocks.
fn optimize_refs(ops: &[Opcode]) -> Vec<Opcode> {
    ops.iter()
        .map(|op| match op {
            Opcode::PushRef(inner) => {
                Opcode::PushRef(optimize(inner))
            }
            other => other.clone(),
        })
        .collect()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // Constant folding
    #[test]
    fn test_constant_folding_add() {
        let ops = vec![Opcode::PushI64(3), Opcode::PushI64(4), Opcode::Add];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(7)]);
    }

    #[test]
    fn test_constant_folding_mul() {
        let ops = vec![Opcode::PushI64(6), Opcode::PushI64(7), Opcode::Mul];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(42)]);
    }

    #[test]
    fn test_constant_folding_comparison() {
        let ops = vec![Opcode::PushI64(3), Opcode::PushI64(4), Opcode::Eq];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushBool(false)]);
    }

    #[test]
    fn test_constant_folding_fsqrt() {
        let ops = vec![Opcode::PushF64(16.0), Opcode::FSqrt];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushF64(4.0)]);
    }

    #[test]
    fn test_constant_folding_i64tostr() {
        let ops = vec![Opcode::PushI64(42), Opcode::I64ToStr];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushStr("42".into())]);
    }

    #[test]
    fn test_constant_folding_strlen() {
        let ops = vec![Opcode::PushStr("hello".into()), Opcode::StrLen];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(5)]);
    }

    #[test]
    fn test_constant_folding_strtoi64() {
        let ops = vec![Opcode::PushStr("99".into()), Opcode::StrToI64];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(99)]);
    }

    // Peephole
    #[test]
    fn test_peephole_dup_drop() {
        let ops = vec![Opcode::PushI64(5), Opcode::Dup, Opcode::Drop];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(5)]);
    }

    #[test]
    fn test_peephole_swap_swap() {
        let ops = vec![Opcode::PushI64(1), Opcode::PushI64(2), Opcode::Swap, Opcode::Swap];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(1), Opcode::PushI64(2)]);
    }

    #[test]
    fn test_peephole_zero_add() {
        let ops = vec![Opcode::PushI64(5), Opcode::PushI64(0), Opcode::Add];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(5)]);
    }

    #[test]
    fn test_peephole_not_not() {
        let ops = vec![Opcode::PushBool(true), Opcode::Not, Opcode::Not];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushBool(true)]);
    }

    #[test]
    fn test_peephole_jump_zero() {
        let ops = vec![Opcode::Jump(0), Opcode::PushI64(42)];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(42)]);
    }

    #[test]
    fn test_peephole_rot_triple() {
        let ops = vec![
            Opcode::PushI64(1),
            Opcode::PushI64(2),
            Opcode::PushI64(3),
            Opcode::Rot,
            Opcode::Rot,
            Opcode::Rot,
        ];
        let opt = optimize(&ops);
        // 3 rots cancel, leaving just the 3 pushes
        assert_eq!(opt.len(), 3);
    }

    #[test]
    fn test_peephole_pick_zero_to_dup() {
        let ops = vec![Opcode::PushI64(5), Opcode::Pick(0)];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(5), Opcode::Dup]);
    }

    // Strength reduction
    #[test]
    fn test_strength_mul2_to_dup_add() {
        // PushI64(2) Mul → Dup Add when the other operand is not a constant.
        // With two constants, folding takes priority (PushI64(10) is better).
        // Test: PushF64(x) PushI64(2) Mul — can't fold across types,
        // but Mul auto-coerces. Strength reduction still applies.
        let ops = vec![Opcode::PushF64(5.0), Opcode::PushI64(2), Opcode::Mul];
        let opt = optimize(&ops);
        // After strength reduction: PushF64(5), Dup, Add
        assert_eq!(opt.len(), 3);
        assert_eq!(opt[0], Opcode::PushF64(5.0));
        assert_eq!(opt[1], Opcode::Dup);
        assert_eq!(opt[2], Opcode::Add);
    }

    // Dead store
    #[test]
    fn test_dead_store_push_drop() {
        let ops = vec![Opcode::PushI64(99), Opcode::Drop, Opcode::PushI64(1)];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(1)]);
    }

    #[test]
    fn test_dead_store_push_str_drop() {
        let ops = vec![
            Opcode::PushStr("unused".into()),
            Opcode::Drop,
            Opcode::PushI64(0),
        ];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(0)]);
    }

    // Composite
    #[test]
    fn test_complex_expression() {
        // (3+4)*2 → PushI64(7) Dup Add  (fold 3+4=7, strength-reduce *2 to dup+add)
        let ops = vec![
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Add,
            Opcode::PushI64(2),
            Opcode::Mul,
        ];
        let opt = optimize(&ops);
        assert_eq!(
            opt,
            vec![Opcode::PushI64(7), Opcode::Dup, Opcode::Add]
        );
    }

    #[test]
    fn test_ref_optimization() {
        // PushRef containing optimizable sequence
        let inner = vec![
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Add,
            Opcode::Dup,
            Opcode::Drop,
        ];
        let ops = vec![Opcode::PushRef(inner)];
        let opt = optimize(&ops);
        match &opt[0] {
            Opcode::PushRef(code) => {
                assert_eq!(&**code, &[Opcode::PushI64(7)] as &[Opcode]);
            }
            _ => panic!("expected PushRef"),
        }
    }

    #[test]
    fn test_no_optimize_with_variables() {
        let ops = vec![Opcode::Dup, Opcode::Add];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::Dup, Opcode::Add]);
    }

    #[test]
    fn test_convergence() {
        // Multiple passes: Dup, Drop, Swap, Swap → should all be removed
        let ops = vec![
            Opcode::PushI64(1),
            Opcode::Dup,
            Opcode::Drop,     // removed (dup+drop)
            Opcode::Swap,
            Opcode::Swap,     // removed (swap+swap)
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Add,      // folded to 7
        ];
        let opt = optimize(&ops);
        assert_eq!(
            opt,
            vec![Opcode::PushI64(1), Opcode::PushI64(7)]
        );
    }
}
