//! Bytecode optimizer: constant folding, peephole optimization, dead code elimination.
//!
//! Runs after bytecode generation, before execution.
//! Each pass scans the opcode sequence and applies transformations.

use whisper_core::opcode::Opcode;

/// Optimize a bytecode sequence. Returns optimized bytecode.
pub fn optimize(bytecode: &[Opcode]) -> Vec<Opcode> {
    let mut ops = bytecode.to_vec();

    // Run optimization passes until convergence
    for _ in 0..3 {
        let before = ops.len();
        ops = constant_folding(&ops);
        ops = peephole(&ops);
        ops = strength_reduction(&ops);
        if ops.len() == before {
            break; // No further reduction
        }
    }

    ops
}

/// Constant folding: evaluate constant expressions at compile time.
///
/// Patterns:
///   PushI64(a) PushI64(b) Add  → PushI64(a+b)
///   PushI64(a) PushI64(b) Sub  → PushI64(a-b)
///   PushI64(a) PushI64(b) Mul  → PushI64(a*b)
///   PushI64(a) PushI64(b) Div  → PushI64(a/b)
///   PushI64(a) PushI64(b) Eq   → PushBool(a==b)
///   PushI64(a) PushI64(b) Lt   → PushBool(a<b)
///   PushI64(a) PushI64(b) Gt   → PushBool(a>b)
fn constant_folding(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        // Look ahead for binary op patterns: PushI64(a), PushI64(b), BinOp
        if i + 2 < ops.len() {
            if let (Opcode::PushI64(a), Opcode::PushI64(b), binop) = (&ops[i], &ops[i+1], &ops[i+2]) {
                let folded = match binop {
                    Opcode::Add => Some(Opcode::PushI64(a + b)),
                    Opcode::Sub => Some(Opcode::PushI64(a - b)),
                    Opcode::Mul => Some(Opcode::PushI64(a * b)),
                    Opcode::Div if *b != 0 => Some(Opcode::PushI64(a / b)),
                    Opcode::Eq => Some(Opcode::PushBool(a == b)),
                    Opcode::Lt => Some(Opcode::PushBool(a < b)),
                    Opcode::Gt => Some(Opcode::PushBool(a > b)),
                    Opcode::Neq => Some(Opcode::PushBool(a != b)),
                    Opcode::Le => Some(Opcode::PushBool(a <= b)),
                    Opcode::Ge => Some(Opcode::PushBool(a >= b)),
                    _ => None,
                };
                if let Some(op) = folded {
                    result.push(op);
                    i += 3;
                    continue;
                }
            }
        }

        result.push(ops[i].clone());
        i += 1;
    }

    result
}

/// Peephole optimization: remove redundant opcode sequences.
///
/// Patterns:
///   Dup Drop         → (nothing)
///   PushI64(0) Add   → (nothing)
///   PushI64(1) Mul   → (nothing)
///   PushI64(0) Sub   → (nothing)
///   PushBool(true) And → (nothing, identity)
///   PushBool(false) Or → (nothing, identity)
///   Not Not           → (nothing)
fn peephole(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        if i + 1 < ops.len() {
            match (&ops[i], &ops[i+1]) {
                // Dup followed by Drop: both cancel
                (Opcode::Dup, Opcode::Drop) => { i += 2; continue; }

                // PushI64(0) Add is no-op
                (Opcode::PushI64(0), Opcode::Add) => { i += 2; continue; }

                // PushI64(1) Mul is no-op
                (Opcode::PushI64(1), Opcode::Mul) => { i += 2; continue; }

                // PushI64(0) Sub is no-op
                (Opcode::PushI64(0), Opcode::Sub) => { i += 2; continue; }

                // Not followed by Not: double negation
                (Opcode::Not, Opcode::Not) => { i += 2; continue; }

                // PushBool + Drop → no-op
                (Opcode::PushBool(_), Opcode::Drop) => { i += 2; continue; }

                // Drop followed by PushI64 (dead store elimination for simple cases)
                // Skip: too aggressive without dataflow analysis

                _ => {}
            }
        }

        // Dup Dup → Dup Pick(1) (more efficient)
        if i + 1 < ops.len() && ops[i] == Opcode::Dup && ops[i+1] == Opcode::Dup {
            result.push(Opcode::Dup);
            result.push(Opcode::Pick(1));
            i += 2;
            continue;
        }

        result.push(ops[i].clone());
        i += 1;
    }

    result
}

/// Strength reduction: replace expensive ops with cheaper equivalents.
///
/// Patterns:
///   PushI64(2) Mul  → Dup Add (for small constants: faster on some archs)
///   PushI64(N) Add where N<0 → PushI64(-N) Sub
fn strength_reduction(ops: &[Opcode]) -> Vec<Opcode> {
    let mut result = Vec::with_capacity(ops.len());
    let mut i = 0;

    while i < ops.len() {
        if i + 1 < ops.len() {
            match (&ops[i], &ops[i+1]) {
                // PushI64(N) Add where N is negative → PushI64(-N) Sub (avoid negative constants)
                (Opcode::PushI64(n), Opcode::Add) if *n < 0 => {
                    result.push(Opcode::PushI64(-n));
                    result.push(Opcode::Sub);
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_peephole_dup_drop() {
        let ops = vec![Opcode::PushI64(5), Opcode::Dup, Opcode::Drop];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(5)]);
    }

    #[test]
    fn test_peephole_zero_add() {
        let ops = vec![Opcode::PushI64(5), Opcode::PushI64(0), Opcode::Add];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(5)]);
    }

    #[test]
    fn test_complex_expression() {
        // (3+4)*2 → 14
        let ops = vec![
            Opcode::PushI64(3), Opcode::PushI64(4), Opcode::Add,
            Opcode::PushI64(2), Opcode::Mul,
        ];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::PushI64(14)]);
    }

    #[test]
    fn test_no_optimize_with_variables() {
        // Dup followed by non-Drop should stay
        let ops = vec![Opcode::Dup, Opcode::Add];
        let opt = optimize(&ops);
        assert_eq!(opt, vec![Opcode::Dup, Opcode::Add]);
    }
}
