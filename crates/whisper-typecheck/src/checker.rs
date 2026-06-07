//! Type checker for the Whisper compilation pipeline.
//!
//! Walks the AST, tracks the type stack, and validates that each operation
//! receives the expected input types. Uses the full Type system with the
//! TypeInferer for constraint-solving across the entire program.
//!
//! Type representation: the full Type enum (I64, F64, Bool, Str, List, Ref,
//! Cap, Signal, TypeVar, Union).  Type variables are resolved by the inferer
//! which persists for the whole check() call.

use whisper_parser::ast::{AstNode, Operator};
use std::collections::HashMap;

use crate::builtins::get_builtin_signature;
use crate::types::Type;
use crate::TypeInferer;

/// A type error with context.
#[derive(Debug, Clone)]
pub struct TypeError {
    pub message: String,
    pub context: String,
}

/// Stack effect signature: (inputs, outputs).
#[derive(Debug, Clone)]
pub struct WordSig {
    pub inputs: Vec<Type>,
    pub outputs: Vec<Type>,
}

/// The type checker.
pub struct TypeChecker {
    /// Inferred stack signatures for user-defined words.
    pub word_sigs: HashMap<String, WordSig>,
    /// The type inference engine, kept alive for the whole program.
    pub inferer: TypeInferer,
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
        let mut stack: Vec<Type> = Vec::new();
        for node in nodes {
            if matches!(node, AstNode::Def { .. }) {
                continue;
            }
            self.check_node(node, &mut stack, &mut errors, "<main>");
        }

        errors
    }

    /// Infer the stack effect signature of a word body.
    fn infer_sig(&mut self, body: &[AstNode]) -> WordSig {
        let mut errors = Vec::new();
        let mut stack: Vec<Type> = Vec::new();

        for node in body {
            self.check_node(node, &mut stack, &mut errors, "<word>");
        }

        // The word starts with an unknown stack; any net producers become outputs,
        // and any net consumers become inputs (in practice, simple words just
        // produce outputs from their body).
        let inputs = vec![];
        let outputs = stack;

        WordSig { inputs, outputs }
    }

    fn check_node(
        &mut self,
        node: &AstNode,
        stack: &mut Vec<Type>,
        errors: &mut Vec<TypeError>,
        ctx: &str,
    ) {
        match node {
            AstNode::Literal(val) => {
                let t = match val {
                    whisper_core::value::Value::I64(_) => Type::I64,
                    whisper_core::value::Value::F64(_) => Type::F64,
                    whisper_core::value::Value::Bool(_) => Type::Bool,
                    whisper_core::value::Value::Str(_) => Type::Str,
                    whisper_core::value::Value::List(items) => {
                        let elem = items.first()
                            .map(|v| value_to_type(v, &mut self.inferer))
                            .unwrap_or_else(|| self.inferer.fresh_var());
                        Type::List(Box::new(elem))
                    }
                    whisper_core::value::Value::Ref(_) => {
                        let t = self.inferer.fresh_var();
                        Type::Ref(vec![t.clone()], vec![t])
                    }
                    _ => self.inferer.fresh_var(),
                };
                stack.push(t);
            }

            AstNode::Op(op) => {
                self.check_op(*op, stack, errors, ctx);
            }

            AstNode::WordRef(name) => {
                let resolved_sig: Option<(Vec<Type>, Vec<Type>)> =
                    if let Some(sig) = self.word_sigs.get(name) {
                        Some((sig.inputs.clone(), sig.outputs.clone()))
                    } else if let Some((inputs, outputs)) = get_builtin_signature(name) {
                        Some((inputs, outputs))
                    } else {
                        None
                    };

                if let Some((expected_inputs, expected_outputs)) = resolved_sig {
                    for expected in expected_inputs.iter().rev() {
                        if let Some(actual) = stack.pop() {
                            if let Err(e) = self.inferer.unify(expected, &actual) {
                                errors.push(TypeError {
                                    message: format!(
                                        "Word '{}' type mismatch: {}",
                                        name, e
                                    ),
                                    context: ctx.to_string(),
                                });
                                stack.push(expected.clone());
                            }
                        } else {
                            errors.push(TypeError {
                                message: format!(
                                    "Stack underflow calling '{}': need {:?}",
                                    name, expected_inputs
                                ),
                                context: ctx.to_string(),
                            });
                            for _ in 0..expected_inputs.len() {
                                stack.push(self.inferer.fresh_var());
                            }
                            break;
                        }
                    }
                    for out in &expected_outputs {
                        stack.push(out.clone());
                    }
                } else {
                    // Unknown word: assume it consumes nothing, produces Any
                    stack.push(self.inferer.fresh_var());
                }
            }

            AstNode::Quote(body) => {
                let mut quote_stack: Vec<Type> = vec![self.inferer.fresh_var()];
                let mut quote_errors = Vec::new();
                for n in body {
                    self.check_node(n, &mut quote_stack, &mut quote_errors, &format!("{ctx}/quote"));
                }
                for err in &quote_errors {
                    if !err.message.contains("Stack underflow") {
                        errors.push(TypeError {
                            message: format!("[quote] {}", err.message),
                            context: err.context.clone(),
                        });
                    }
                }
                // Build a Ref type from the quote's consumed/produced stack effect
                let input = self.inferer.fresh_var();
                let output = if quote_stack.len() > 1 {
                    quote_stack[1..].to_vec()
                } else {
                    vec![self.inferer.fresh_var()]
                };
                stack.push(Type::Ref(vec![input], output));
            }

            AstNode::List(items) => {
                let mut elem_type: Option<Type> = None;
                for item in items {
                    self.check_node(item, stack, errors, ctx);
                    if let Some(popped) = stack.pop() {
                        if let Some(ref expected) = elem_type {
                            if let Err(_) = self.inferer.unify(expected, &popped) {
                                errors.push(TypeError {
                                    message: format!(
                                        "List element type mismatch: expected {}, got {}",
                                        self.inferer.resolve(expected),
                                        self.inferer.resolve(&popped),
                                    ),
                                    context: ctx.to_string(),
                                });
                            }
                        } else {
                            elem_type = Some(popped);
                        }
                    }
                }
                let elem = elem_type.unwrap_or_else(|| self.inferer.fresh_var());
                stack.push(Type::List(Box::new(self.inferer.resolve(&elem))));
            }

            AstNode::Cond { then_branch, else_branch } => {
                self.expect(stack, &Type::Bool, errors, ctx, "condition");
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
                stack.truncate(depth);
                let then_result = then_stack.last().cloned().unwrap_or_else(|| self.inferer.fresh_var());
                let else_result = else_stack.last().cloned().unwrap_or_else(|| self.inferer.fresh_var());
                if self.inferer.unify(&then_result, &else_result).is_ok() {
                    stack.push(self.inferer.resolve(&then_result));
                } else {
                    // Branches produce different types → union type
                    stack.push(Type::Union(
                        Box::new(self.inferer.resolve(&then_result)),
                        Box::new(self.inferer.resolve(&else_result)),
                    ));
                }
            }

            AstNode::Loop { body, condition } => {
                for n in body {
                    self.check_node(n, stack, errors, &format!("{ctx}/loop"));
                }
                self.expect(stack, &Type::Bool, errors, ctx, "loop condition");
                for n in condition {
                    self.check_node(n, stack, errors, ctx);
                }
            }

            AstNode::Def { .. } | AstNode::Import(_) | AstNode::Export(_) => {}

            AstNode::Times { .. } => {
                self.expect(stack, &Type::I64, errors, ctx, "@times count");
                self.expect_ref(stack, errors, ctx, "@times quot");
            }

            AstNode::CondArrow { .. } => {
                self.expect(stack, &Type::Bool, errors, ctx, "?-> condition");
            }

            AstNode::ConfidenceLabel { body, confidence: _ } => {
                for n in body {
                    self.check_node(n, stack, errors, ctx);
                }
            }

            AstNode::ProbChoice { alt1, alt2 } => {
                let depth = stack.len();
                let mut s1 = stack.clone();
                for n in alt1 {
                    self.check_node(n, &mut s1, errors, &format!("{ctx}/alt1"));
                }
                let mut s2 = stack.clone();
                for n in alt2 {
                    self.check_node(n, &mut s2, errors, &format!("{ctx}/alt2"));
                }
                let r1 = s1.last().cloned().unwrap_or_else(|| self.inferer.fresh_var());
                let r2 = s2.last().cloned().unwrap_or_else(|| self.inferer.fresh_var());
                stack.truncate(depth);
                if self.inferer.unify(&r1, &r2).is_ok() {
                    stack.push(self.inferer.resolve(&r1));
                } else {
                    stack.push(Type::Union(
                        Box::new(self.inferer.resolve(&r1)),
                        Box::new(self.inferer.resolve(&r2)),
                    ));
                }
            }
        }
    }

    fn check_op(
        &mut self,
        op: Operator,
        stack: &mut Vec<Type>,
        errors: &mut Vec<TypeError>,
        ctx: &str,
    ) {
        // Helper: num = i64 | f64
        let num = || Type::Union(Box::new(Type::I64), Box::new(Type::F64));

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

            // Arithmetic: Num, Num → Num
            Operator::Add | Operator::Sub | Operator::Mul | Operator::Div | Operator::Mod => {
                let n = num();
                self.expect(stack, &n, errors, ctx, "arithmetic rhs");
                self.expect(stack, &n, errors, ctx, "arithmetic lhs");
                stack.push(n);
            }

            // Comparison: Any, Any → Bool
            Operator::Eq | Operator::Lt | Operator::Gt
            | Operator::Neq | Operator::Le | Operator::Ge => {
                stack.pop();
                stack.pop();
                stack.push(Type::Bool);
            }

            // Logic: Bool, Bool → Bool
            Operator::And | Operator::Or => {
                self.expect(stack, &Type::Bool, errors, ctx, "logic rhs");
                self.expect(stack, &Type::Bool, errors, ctx, "logic lhs");
                stack.push(Type::Bool);
            }
            Operator::Not => {
                self.expect(stack, &Type::Bool, errors, ctx, "not");
                stack.push(Type::Bool);
            }

            // List ops
            Operator::Len => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect(stack, &any_list, errors, ctx, "len");
                stack.push(Type::I64);
            }

            // String ops
            Operator::StrLen => {
                self.expect(stack, &Type::Str, errors, ctx, "strlen");
                stack.push(Type::I64);
            }
            Operator::StrCat => {
                self.expect(stack, &Type::Str, errors, ctx, "strcat rhs");
                self.expect(stack, &Type::Str, errors, ctx, "strcat lhs");
                stack.push(Type::Str);
            }
            Operator::StrSlice => {
                self.expect(stack, &Type::I64, errors, ctx, "strslice len");
                self.expect(stack, &Type::I64, errors, ctx, "strslice start");
                self.expect(stack, &Type::Str, errors, ctx, "strslice string");
                stack.push(Type::Str);
            }

            Operator::Nth => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect(stack, &Type::I64, errors, ctx, "@nth index");
                self.expect(stack, &any_list, errors, ctx, "@nth list");
                stack.push(self.inferer.fresh_var());
            }
            Operator::Append => {
                let elem = self.inferer.fresh_var();
                let list_ty = Type::List(Box::new(elem.clone()));
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                stack.pop(); // element — accept anything
                self.expect(stack, &any_list, errors, ctx, "append list");
                stack.push(list_ty);
            }
            Operator::Map => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref(stack, errors, ctx, "@map quot");
                self.expect(stack, &any_list, errors, ctx, "@map list");
                stack.push(Type::List(Box::new(self.inferer.fresh_var())));
            }
            Operator::Each => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref(stack, errors, ctx, "@each quot");
                self.expect(stack, &any_list, errors, ctx, "@each list");
            }
            Operator::Fold => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref(stack, errors, ctx, "@fold quot");
                stack.pop(); // init — accept anything
                self.expect(stack, &any_list, errors, ctx, "@fold list");
                stack.push(self.inferer.fresh_var());
            }
            Operator::AtTimes => {
                self.expect_ref(stack, errors, ctx, "@times body");
                self.expect(stack, &Type::I64, errors, ctx, "@times count");
            }

            // IO
            Operator::OutputTop => {
                if stack.is_empty() {
                    errors.push(TypeError { message: ".: stack empty".into(), context: ctx.into() });
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
        stack: &mut Vec<Type>,
        expected: &Type,
        errors: &mut Vec<TypeError>,
        ctx: &str,
        desc: &str,
    ) {
        match stack.pop() {
            Some(actual) => {
                // Unify expected with actual; if it fails, report error but tolerate
                if let Err(_) = self.inferer.unify(expected, &actual) {
                    errors.push(TypeError {
                        message: format!(
                            "{}: expected {}, got {}",
                            desc,
                            self.inferer.resolve(expected),
                            self.inferer.resolve(&actual),
                        ),
                        context: ctx.to_string(),
                    });
                }
            }
            None => {
                errors.push(TypeError {
                    message: format!("Stack underflow: {} needs {}", desc, expected),
                    context: ctx.to_string(),
                });
            }
        }
    }

    fn expect_ref(
        &mut self,
        stack: &mut Vec<Type>,
        errors: &mut Vec<TypeError>,
        ctx: &str,
        desc: &str,
    ) {
        let t = self.inferer.fresh_var();
        let ref_ty = Type::Ref(vec![t.clone()], vec![t]);
        self.expect(stack, &ref_ty, errors, ctx, desc);
    }
}

impl Default for TypeChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert a Whisper Value to its corresponding Type.
fn value_to_type(val: &whisper_core::value::Value, inferer: &mut TypeInferer) -> Type {
    match val {
        whisper_core::value::Value::I64(_) => Type::I64,
        whisper_core::value::Value::F64(_) => Type::F64,
        whisper_core::value::Value::Bool(_) => Type::Bool,
        whisper_core::value::Value::Str(_) => Type::Str,
        whisper_core::value::Value::List(items) => {
            let elem = items.first()
                .map(|v| value_to_type(v, inferer))
                .unwrap_or_else(|| inferer.fresh_var());
            Type::List(Box::new(elem))
        }
        whisper_core::value::Value::Ref(_) => {
            let t = inferer.fresh_var();
            Type::Ref(vec![t.clone()], vec![t])
        }
        _ => inferer.fresh_var(),
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
        let mut tc = TypeChecker::new();
        let source = ": sq { _ * } ; 5 sq";
        let ast = Parser::parse_source(source).unwrap();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
        let sig = tc.word_sigs.get("sq").unwrap();
        assert!(
            sig.outputs.len() >= 1,
            "sq should produce at least 1 output, got {:?}",
            sig.outputs
        );
    }

    #[test]
    fn test_builtin_signature_used() {
        let source = "[1 2 3] len";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty());
    }

    #[test]
    fn test_type_mismatch_in_list_elements() {
        let source = "5 len";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(!errors.is_empty(), "Expected type error for '5 len'");
    }

    #[test]
    fn test_single_branch_conditional() {
        let source = "5 3 > ??100|0]";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }

    #[test]
    fn test_quote_type_checking() {
        let source = "[1 2 3] { _ * } @map";
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        let errors = tc.check(&ast);
        assert!(errors.is_empty(), "Unexpected errors: {errors:?}");
    }
}
