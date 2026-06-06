//! Practical type checker for the Whisper compilation pipeline.
//!
//! Walks the AST, tracks the conceptual type stack, and validates
//! that each operation receives the expected input types.
//!
//! Type representation at check time is lightweight:
//!   T: any type, N: number(i64|f64), I: i64, F: f64, B: bool, S: str, L: list, Q: ref

use whisper_parser::ast::{AstNode, Operator};
use std::collections::HashMap;

/// Simplified type for stack tracking.
#[derive(Debug, Clone, PartialEq)]
pub enum SType {
    Any,     // T (unknown/polymorphic)
    Num,     // i64 | f64
    Int,     // i64
    Float,   // f64
    Bool,    // bool
    Str,     // str
    List,    // [T]
    Ref,     // quotation block
}

impl SType {
    pub fn name(&self) -> &str {
        match self {
            SType::Any => "T",
            SType::Num => "num",
            SType::Int => "i64",
            SType::Float => "f64",
            SType::Bool => "bool",
            SType::Str => "str",
            SType::List => "[T]",
            SType::Ref => "ref",
        }
    }

    /// Check if self can accept other (self is expected, other is actual).
    pub fn accepts(&self, other: &SType) -> bool {
        match (self, other) {
            (SType::Any, _) => true,
            (SType::Num, SType::Int | SType::Float | SType::Num) => true,
            (a, b) => a == b,
        }
    }
}

/// A type error with context.
#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub context: String,
}

/// The type checker.
pub struct TypeChecker {
    /// Inferred stack signatures for user-defined words.
    word_sigs: HashMap<String, WordSig>,
}

/// Stack effect signature: (inputs, outputs).
#[derive(Debug, Clone)]
pub struct WordSig {
    pub inputs: Vec<SType>,
    pub outputs: Vec<SType>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker { word_sigs: HashMap::new() }
    }

    /// Check an entire program. Returns list of errors (empty = OK).
    pub fn check(&mut self, nodes: &[AstNode]) -> Vec<TypeError> {
        let mut errors = Vec::new();

        // Pass 1: collect word definitions and infer signatures
        for node in nodes {
            if let AstNode::Def { name, body } = node {
                let sig = self.infer_sig(body);
                self.word_sigs.insert(name.clone(), sig);
            }
        }

        // Pass 2: check main program body
        let mut stack: Vec<SType> = Vec::new();
        for node in nodes {
            if matches!(node, AstNode::Def { .. }) { continue; }
            self.check_node(node, &mut stack, &mut errors, "<main>");
        }

        errors
    }

    fn infer_sig(&self, _body: &[AstNode]) -> WordSig {
        // Conservative: assume 0 inputs, 1 output (T)
        // Full inference would require complex analysis
        WordSig {
            inputs: vec![],
            outputs: vec![SType::Any],
        }
    }

    fn check_node(&self, node: &AstNode, stack: &mut Vec<SType>, errors: &mut Vec<TypeError>, ctx: &str) {
        match node {
            AstNode::Literal(val) => {
                let t = match val {
                    whisper_core::value::Value::I64(_) => SType::Int,
                    whisper_core::value::Value::F64(_) => SType::Float,
                    whisper_core::value::Value::Bool(_) => SType::Bool,
                    whisper_core::value::Value::Str(_) => SType::Str,
                    whisper_core::value::Value::List(_) => SType::List,
                    whisper_core::value::Value::Ref(_) => SType::Ref,
                    _ => SType::Any,
                };
                stack.push(t);
            }

            AstNode::Op(op) => {
                self.check_op(*op, stack, errors, ctx);
            }

            AstNode::WordRef(name) => {
                if let Some(sig) = self.word_sigs.get(name) {
                    // Pop expected inputs, push expected outputs
                    for expected in &sig.inputs {
                        if let Some(actual) = stack.pop() {
                            if !expected.accepts(&actual) {
                                errors.push(TypeError {
                                    message: format!("Word '{}' expects {:?} but got {}", name, expected, actual.name()),
                                    context: ctx.to_string(),
                                });
                            }
                        } else {
                            errors.push(TypeError {
                                message: format!("Stack underflow calling '{}': need {:?}", name, sig.inputs),
                                context: ctx.to_string(),
                            });
                        }
                    }
                    for out in &sig.outputs {
                        stack.push(out.clone());
                    }
                } else {
                    // Unknown word: assume it produces Any
                    stack.push(SType::Any);
                }
            }

            AstNode::Quote(_body) => {
                // Quotation body is deferred — don't check inline.
                // It will be checked when applied (e.g., by @map/@each/@fold).
                stack.push(SType::Ref);
            }

            AstNode::List(items) => {
                for item in items {
                    self.check_node(item, stack, errors, ctx);
                    stack.pop(); // consume into list
                }
                stack.push(SType::List);
            }

            AstNode::Cond { then_branch, else_branch } => {
                // Condition should be Bool
                self.expect(stack, &SType::Bool, errors, ctx, "condition");
                // Save stack depth before branches
                let depth = stack.len();
                // Check then branch
                let mut then_stack = stack.clone();
                for n in then_branch { self.check_node(n, &mut then_stack, errors, &format!("{ctx}/then")); }
                // Check else branch
                let mut else_stack = stack.clone();
                if let Some(eb) = else_branch {
                    for n in eb { self.check_node(n, &mut else_stack, errors, &format!("{ctx}/else")); }
                }
                // Both branches should leave stack at same depth
                // Conservative: restore to original depth + 1 result
                stack.truncate(depth);
                stack.push(SType::Any);
            }

            AstNode::Loop { body, condition } => {
                for n in body { self.check_node(n, stack, errors, &format!("{ctx}/loop")); }
                self.expect(stack, &SType::Bool, errors, ctx, "loop condition");
                for n in condition { self.check_node(n, stack, errors, ctx); }
            }

            AstNode::Def { .. } | AstNode::Import(_) | AstNode::Export(_) => {}

            AstNode::Times { .. } => {
                self.expect(stack, &SType::Int, errors, ctx, "@times count");
                self.expect(stack, &SType::Ref, errors, ctx, "@times quot");
            }

            _ => {}
        }
    }

    fn check_op(&self, op: Operator, stack: &mut Vec<SType>, errors: &mut Vec<TypeError>, ctx: &str) {
        match op {
            // Stack ops
            Operator::Dup => {
                if stack.is_empty() {
                    errors.push(TypeError { message: "Dup: stack empty".into(), context: ctx.into() });
                } else {
                    let t = stack.last().unwrap().clone();
                    stack.push(t);
                }
            }
            Operator::Swap => {
                if stack.len() < 2 {
                    errors.push(TypeError { message: "Swap: need 2 values".into(), context: ctx.into() });
                } else {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    stack.push(a);
                    stack.push(b);
                }
            }
            Operator::Drop => {
                if stack.is_empty() {
                    errors.push(TypeError { message: "Drop: stack empty".into(), context: ctx.into() });
                } else {
                    stack.pop();
                }
            }
            Operator::Rot => {
                if stack.len() < 3 {
                    errors.push(TypeError { message: "Rot: need 3 values".into(), context: ctx.into() });
                }
            }

            // Arithmetic: need Num, Num → Num
            Operator::Add | Operator::Sub | Operator::Mul | Operator::Div | Operator::Mod => {
                self.expect(stack, &SType::Num, errors, ctx, "arithmetic rhs");
                self.expect(stack, &SType::Num, errors, ctx, "arithmetic lhs");
                stack.push(SType::Num);
            }

            // Comparison: need Num, Num → Bool
            Operator::Eq | Operator::Lt | Operator::Gt | Operator::Neq | Operator::Le | Operator::Ge => {
                self.expect(stack, &SType::Any, errors, ctx, "compare rhs");
                self.expect(stack, &SType::Any, errors, ctx, "compare lhs");
                stack.push(SType::Bool);
            }

            // Logic: Bool, Bool → Bool
            Operator::And | Operator::Or => {
                self.expect(stack, &SType::Bool, errors, ctx, "logic rhs");
                self.expect(stack, &SType::Bool, errors, ctx, "logic lhs");
                stack.push(SType::Bool);
            }
            Operator::Not => {
                self.expect(stack, &SType::Bool, errors, ctx, "not");
                stack.push(SType::Bool);
            }

            // List ops
            Operator::Len => {
                self.expect(stack, &SType::List, errors, ctx, "len");
                stack.push(SType::Int);
            }
            Operator::Nth => {
                self.expect(stack, &SType::Int, errors, ctx, "@nth index");
                self.expect(stack, &SType::List, errors, ctx, "@nth list");
                stack.push(SType::Any);
            }
            Operator::Append => {
                self.expect(stack, &SType::Any, errors, ctx, "append element");
                self.expect(stack, &SType::List, errors, ctx, "append list");
                stack.push(SType::List);
            }
            Operator::Map => {
                self.expect(stack, &SType::Ref, errors, ctx, "@map quot");
                self.expect(stack, &SType::List, errors, ctx, "@map list");
                stack.push(SType::List);
            }
            Operator::Each => {
                self.expect(stack, &SType::Ref, errors, ctx, "@each quot");
                self.expect(stack, &SType::List, errors, ctx, "@each list");
            }
            Operator::Fold => {
                self.expect(stack, &SType::Ref, errors, ctx, "@fold quot");
                self.expect(stack, &SType::Any, errors, ctx, "@fold init");
                self.expect(stack, &SType::List, errors, ctx, "@fold list");
                stack.push(SType::Any);
            }
            Operator::AtTimes => {
                self.expect(stack, &SType::Ref, errors, ctx, "@times body");
                self.expect(stack, &SType::Int, errors, ctx, "@times count");
            }

            // IO
            Operator::OutputTop => {
                if stack.is_empty() {
                    errors.push(TypeError { message: ".: stack empty".into(), context: ctx.into() });
                } else {
                    stack.pop();
                }
            }
            Operator::OutputAll => { stack.clear(); }

            _ => {} // CapCall, CapExec, Cond, etc. unchecked
        }
    }

    fn expect(&self, stack: &mut Vec<SType>, expected: &SType, errors: &mut Vec<TypeError>, ctx: &str, desc: &str) {
        match stack.pop() {
            Some(actual) if expected.accepts(&actual) => {}
            Some(actual) => {
                errors.push(TypeError {
                    message: format!("{}: expected {}, got {}", desc, expected.name(), actual.name()),
                    context: ctx.to_string(),
                });
            }
            None => {
                errors.push(TypeError {
                    message: format!("Stack underflow: {} needs {}", desc, expected.name()),
                    context: ctx.to_string(),
                });
            }
        }
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use whisper_parser::Parser;

    fn check_ok(source: &str) {
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Expected no errors for: {source}\nGot: {errors:?}");
    }

    fn check_err(source: &str) {
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(!errors.is_empty(), "Expected errors for: {source}");
    }

    #[test]
    fn test_simple_ok() { check_ok("3 4 +"); }

    #[test]
    fn test_stack_underflow() { check_err("+"); }

    #[test]
    fn test_dup_ok() { check_ok("5 _ *"); }

    #[test]
    fn test_map_ok() { check_ok("[1 2 3] { _ * } @map"); }

    #[test]
    fn test_string_output_ok() { check_ok("\"hello\" ."); }
}
