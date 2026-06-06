//! Practical type checker for the Whisper compilation pipeline.
//!
//! Walks the AST, tracks the conceptual type stack, and validates
//! that each operation receives the expected input types.
//!
//! Uses the TypeInferer for type variable unification and constraint
//! solving when checking word definitions and calls.
//!
//! Type representation at check time is lightweight:
//!   T: any type, N: number(i64|f64), I: i64, F: f64, B: bool, S: str, L: list, Q: ref

use whisper_parser::ast::{AstNode, Operator};
use std::collections::HashMap;

use crate::builtins::get_builtin_signature;
use crate::types::Type;
use crate::TypeInferer;

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
            (_, SType::Any) => true, // Unknown type is compatible with anything
            (SType::Num, SType::Int | SType::Float | SType::Num) => true,
            (a, b) => a == b,
        }
    }
}

/// Convert an SType to the full Type system using an inferer for fresh variables.
fn stype_to_full_type(st: &SType, inferer: &mut TypeInferer) -> Type {
    match st {
        SType::Any => inferer.fresh_var(),
        SType::Num => inferer.fresh_var(),
        SType::Int => Type::I64,
        SType::Float => Type::F64,
        SType::Bool => Type::Bool,
        SType::Str => Type::Str,
        SType::List => Type::List(Box::new(inferer.fresh_var())),
        SType::Ref => {
            let t = inferer.fresh_var();
            Type::Ref(vec![t.clone()], vec![t])
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
    /// The type inference engine for constraint solving.
    inferer: TypeInferer,
}

/// Stack effect signature: (inputs, outputs).
#[derive(Debug, Clone)]
pub struct WordSig {
    pub inputs: Vec<SType>,
    pub outputs: Vec<SType>,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            word_sigs: HashMap::new(),
            inferer: TypeInferer::new(),
        }
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
            if matches!(node, AstNode::Def { .. }) {
                continue;
            }
            self.check_node(node, &mut stack, &mut errors, "<main>");
        }

        errors
    }

    /// Infer the stack effect signature of a word body.
    ///
    /// Walks the word body, tracking stack changes and using the TypeInferer
    /// to unify type variables. Returns the net stack effect.
    fn infer_sig(&mut self, body: &[AstNode]) -> WordSig {
        let mut errors = Vec::new();
        let initial_depth = 0usize;
        let mut stack: Vec<SType> = Vec::new();

        // Track stack before first node to detect inputs
        for node in body {
            self.check_node(node, &mut stack, &mut errors, "<word>");
        }

        let depth = stack.len();
        // Stack depth at end = outputs (if > initial) or consumed inputs (if < initial)
        let inputs = if depth < initial_depth {
            (0..initial_depth - depth)
                .map(|_| SType::Any)
                .collect()
        } else {
            vec![]
        };
        let outputs = stack;

        WordSig { inputs, outputs }
    }

    fn check_node(
        &mut self,
        node: &AstNode,
        stack: &mut Vec<SType>,
        errors: &mut Vec<TypeError>,
        ctx: &str,
    ) {
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
                // Resolve word signature: user-defined words first, then builtins
                let resolved_sig: Option<(Vec<SType>, Vec<SType>)> =
                    if let Some(sig) = self.word_sigs.get(name) {
                        Some((sig.inputs.clone(), sig.outputs.clone()))
                    } else if let Some((inputs, outputs)) = get_builtin_signature(name) {
                        Some((
                            inputs.iter().map(type_to_stype).collect(),
                            outputs.iter().map(type_to_stype).collect(),
                        ))
                    } else {
                        None
                    };

                if let Some((expected_inputs, expected_outputs)) = resolved_sig {
                    // Validate inputs with the inferer for precise type unification
                    self.inferer.reset();
                    for expected_st in expected_inputs.iter().rev() {
                        let expected_ty = stype_to_full_type(expected_st, &mut self.inferer);
                        if let Some(actual) = stack.pop() {
                            let actual_ty = stype_to_full_type(&actual, &mut self.inferer);
                            if let Err(e) = self.inferer.unify(&expected_ty, &actual_ty) {
                                errors.push(TypeError {
                                    message: format!(
                                        "Word '{}' type mismatch: {}",
                                        name, e
                                    ),
                                    context: ctx.to_string(),
                                });
                                // Push back a dummy to maintain stack consistency
                                stack.push(expected_st.clone());
                            }
                        } else {
                            errors.push(TypeError {
                                message: format!(
                                    "Stack underflow calling '{}': need {:?}",
                                    name, expected_inputs
                                ),
                                context: ctx.to_string(),
                            });
                            // Push back dummies
                            for _ in 0..expected_inputs.len() {
                                stack.push(SType::Any);
                            }
                            break;
                        }
                    }
                    // Push resolved outputs
                    for out in &expected_outputs {
                        stack.push(out.clone());
                    }
                } else {
                    // Unknown word: assume it produces Any
                    stack.push(SType::Any);
                }
            }

            AstNode::Quote(body) => {
                // Check the quotation body. Start with a fresh type variable
                // representing the input that will be on the stack at call time.
                let mut quote_stack: Vec<SType> = vec![SType::Any];
                let mut quote_errors = Vec::new();
                for n in body {
                    self.check_node(n, &mut quote_stack, &mut quote_errors, &format!("{ctx}/quote"));
                }
                // The quotation consumes 1 value and may produce results.
                // Report internal errors as warnings — don't fail the program,
                // since the quotation may be used in different stack contexts.
                for err in &quote_errors {
                    if !err.message.contains("Stack underflow") {
                        errors.push(TypeError {
                            message: format!("[quote] {}", err.message),
                            context: err.context.clone(),
                        });
                    }
                }
                stack.push(SType::Ref);
            }

            AstNode::List(items) => {
                // Check list elements for type consistency
                let mut elem_type: Option<SType> = None;
                for item in items {
                    self.check_node(item, stack, errors, ctx);
                    if let Some(popped) = stack.pop() {
                        if let Some(ref expected) = elem_type {
                            if !expected.accepts(&popped) {
                                errors.push(TypeError {
                                    message: format!(
                                        "List element type mismatch: expected {:?}, got {}",
                                        expected,
                                        popped.name()
                                    ),
                                    context: ctx.to_string(),
                                });
                            }
                        } else {
                            elem_type = Some(popped.clone());
                        }
                    }
                }
                stack.push(SType::List);
            }

            AstNode::Cond {
                then_branch,
                else_branch,
            } => {
                self.expect(stack, &SType::Bool, errors, ctx, "condition");
                let depth = stack.len();
                let mut then_stack = stack.clone();
                for n in then_branch {
                    self.check_node(n, &mut then_stack, errors, &format!("{ctx}/then"));
                }
                let mut else_stack = stack.clone();
                if let Some(eb) = else_branch {
                    for n in eb {
                        self.check_node(n, &mut else_stack, errors, &format!("{ctx}/else"));
                    }
                }
                // Restore to original depth + push the unified result type
                stack.truncate(depth);
                // If both branches produce results, unify their types
                let then_result = then_stack.get(then_stack.len().wrapping_sub(1));
                let else_result = else_stack.get(else_stack.len().wrapping_sub(1));
                match (then_result, else_result) {
                    (Some(t), Some(e)) if t.accepts(e) || e.accepts(t) => {
                        stack.push(t.clone());
                    }
                    _ => {
                        stack.push(SType::Any);
                    }
                }
            }

            AstNode::Loop { body, condition } => {
                for n in body {
                    self.check_node(n, stack, errors, &format!("{ctx}/loop"));
                }
                self.expect(stack, &SType::Bool, errors, ctx, "loop condition");
                for n in condition {
                    self.check_node(n, stack, errors, ctx);
                }
            }

            AstNode::Def { .. } | AstNode::Import(_) | AstNode::Export(_) => {}

            AstNode::Times { .. } => {
                self.expect(stack, &SType::Int, errors, ctx, "@times count");
                self.expect(stack, &SType::Ref, errors, ctx, "@times quot");
            }

            AstNode::CondArrow { .. } => {
                self.expect(stack, &SType::Bool, errors, ctx, "?-> condition");
            }

            AstNode::ConfidenceLabel { body, confidence: _ } => {
                for n in body {
                    self.check_node(n, stack, errors, ctx);
                }
            }

            AstNode::ProbChoice { alt1, alt2 } => {
                // Both alternatives must produce the same type
                let depth = stack.len();
                let mut s1 = stack.clone();
                for n in alt1 {
                    self.check_node(n, &mut s1, errors, &format!("{ctx}/alt1"));
                }
                let mut s2 = stack.clone();
                for n in alt2 {
                    self.check_node(n, &mut s2, errors, &format!("{ctx}/alt2"));
                }
                let r1 = s1.last().cloned().unwrap_or(SType::Any);
                let r2 = s2.last().cloned().unwrap_or(SType::Any);
                stack.truncate(depth);
                if r1.accepts(&r2) {
                    stack.push(r1);
                } else {
                    stack.push(SType::Any);
                }
            }
        }
    }

    fn check_op(
        &mut self,
        op: Operator,
        stack: &mut Vec<SType>,
        errors: &mut Vec<TypeError>,
        ctx: &str,
    ) {
        match op {
            // Stack ops
            Operator::Dup => {
                if stack.is_empty() {
                    errors.push(TypeError {
                        message: "Dup: stack empty".into(),
                        context: ctx.into(),
                    });
                } else {
                    let t = stack.last().unwrap().clone();
                    stack.push(t);
                }
            }
            Operator::Swap => {
                if stack.len() < 2 {
                    errors.push(TypeError {
                        message: "Swap: need 2 values".into(),
                        context: ctx.into(),
                    });
                } else {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    stack.push(a);
                    stack.push(b);
                }
            }
            Operator::Drop => {
                if stack.is_empty() {
                    errors.push(TypeError {
                        message: "Drop: stack empty".into(),
                        context: ctx.into(),
                    });
                } else {
                    stack.pop();
                }
            }
            Operator::Rot => {
                if stack.len() < 3 {
                    errors.push(TypeError {
                        message: "Rot: need 3 values".into(),
                        context: ctx.into(),
                    });
                }
            }

            // Arithmetic: need Num, Num → Num
            Operator::Add
            | Operator::Sub
            | Operator::Mul
            | Operator::Div
            | Operator::Mod => {
                self.expect(stack, &SType::Num, errors, ctx, "arithmetic rhs");
                self.expect(stack, &SType::Num, errors, ctx, "arithmetic lhs");
                stack.push(SType::Num);
            }

            // Comparison: need Any, Any → Bool
            Operator::Eq
            | Operator::Lt
            | Operator::Gt
            | Operator::Neq
            | Operator::Le
            | Operator::Ge => {
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
                    errors.push(TypeError {
                        message: ".: stack empty".into(),
                        context: ctx.into(),
                    });
                } else {
                    stack.pop();
                }
            }
            Operator::OutputAll => {
                stack.clear();
            }

            _ => {} // CapCall, CapExec, Cond, etc. unchecked
        }
    }

    fn expect(
        &mut self,
        stack: &mut Vec<SType>,
        expected: &SType,
        errors: &mut Vec<TypeError>,
        ctx: &str,
        desc: &str,
    ) {
        match stack.pop() {
            Some(actual) if expected.accepts(&actual) => {}
            Some(actual) => {
                errors.push(TypeError {
                    message: format!(
                        "{}: expected {}, got {}",
                        desc,
                        expected.name(),
                        actual.name()
                    ),
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

/// Map a full Type to the simplified SType for the checker.
fn type_to_stype(ty: &Type) -> SType {
    match ty {
        Type::I64 => SType::Int,
        Type::F64 => SType::Float,
        Type::Bool => SType::Bool,
        Type::Str => SType::Str,
        Type::List(_) => SType::List,
        Type::Ref(_, _) => SType::Ref,
        Type::TypeVar(_) => SType::Any,
        Type::Signal(inner) => type_to_stype(inner),
        Type::Union(a, _) => type_to_stype(a), // take the first branch for SType
        Type::Cap(_) => SType::Any,
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
        assert!(
            errors.is_empty(),
            "Expected no errors for: {source}\nGot: {errors:?}"
        );
    }

    fn check_err(source: &str) {
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(
            !errors.is_empty(),
            "Expected errors for: {source}"
        );
    }

    #[test]
    fn test_simple_ok() {
        check_ok("3 4 +");
    }

    #[test]
    fn test_stack_underflow() {
        check_err("+");
    }

    #[test]
    fn test_dup_ok() {
        check_ok("5 _ *");
    }

    #[test]
    fn test_map_ok() {
        check_ok("[1 2 3] { _ * } @map");
    }

    #[test]
    fn test_string_output_ok() {
        check_ok("\"hello\" .");
    }

    #[test]
    fn test_word_sig_inference() {
        // The checker should infer that 'sq' consumes 0 inputs, produces 1 output
        let mut tc = TypeChecker::new();
        let source = ": sq { _ * } ; 5 sq";
        let ast = Parser::parse_source(source).unwrap();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
        // Verify 'sq' was registered
        let sig = tc.word_sigs.get("sq").unwrap();
        assert!(
            sig.outputs.len() >= 1,
            "sq should produce at least 1 output, got {:?}",
            sig.outputs
        );
    }

    #[test]
    fn test_builtin_signature_used() {
        // 'len' builtin should be recognized and produce Int
        let source = "[1 2 3] len";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_type_mismatch_in_list_elements() {
        // Passing non-list to 'len' should be caught
        // Actually, the checker compares SType::List expectation vs actual
        let source = "5 len";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        // Pushing Int then calling 'len' — the builtin signature says List input
        // type_to_stype(Type::List(_)) = SType::List
        // stack has SType::Int → SType::Int != SType::List → error
        assert!(!errors.is_empty(), "Expected type error for '5 len'");
    }

    #[test]
    fn test_single_branch_conditional() {
        // cond {then} ?-> should be recognized
        let source = "5 3 > ??100|0]";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_quote_type_checking() {
        // Quote bodies are checked for internal consistency
        let source = "[1 2 3] { _ * } @map";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }
}
