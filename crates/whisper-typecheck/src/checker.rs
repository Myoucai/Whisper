//! Type checker for the Whisper compilation pipeline.
//!
//! Walks the AST, tracks the type stack, and validates that each operation
//! receives the expected input types. Uses the full Type system with the
//! TypeInferer for constraint-solving across the entire program.
//!
//! Type representation: the full Type enum (I64, F64, Bool, Str, List, Ref,
//! Cap, Signal, TypeVar, Union).  Type variables are resolved by the inferer
//! which persists for the whole check() call.

use std::collections::HashMap;
use whisper_parser::ast::{AstNode, Operator};

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
    /// N-2 Phase 2.5：infer_sig 模式标志 + 借位计数——词体欠压操作
    /// （体首 dup/drop 等）合成 fresh 输入并计数，而非报错跳过。
    /// 签名输入数 = 借位数（J-3 "体首即消费给 0 输入" 的修正）。
    inferring: bool,
    borrowed: usize,
    /// `return` 之后的节点不可达（12 臂分发链的借位虚高根因）
    terminated: bool,
}

impl TypeChecker {
    pub fn new() -> Self {
        TypeChecker {
            word_sigs: HashMap::new(),
            inferer: TypeInferer::new(),
            inferring: false,
            borrowed: 0,
            terminated: false,
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
        let trace = std::env::var("WHISPER_TC_TRACE").is_ok();
        for node in nodes {
            if matches!(node, AstNode::Def { .. }) {
                continue;
            }
            self.check_node(node, &mut stack, &mut errors, "<main>");
            if trace {
                let rendered: Vec<String> = stack
                    .iter()
                    .map(|t| self.inferer.resolve(t).name())
                    .collect();
                eprintln!("[tc-trace] {:?} => {:?}", node_brief(node), rendered);
            }
        }

        errors
    }

    /// Infer the stack effect signature of a word body.
    ///
    /// Uses a temporary TypeInferer so that intermediate type variables
    /// donʼt leak into the main programʼs type state.
    fn infer_sig(&mut self, body: &[AstNode]) -> WordSig {
        // Swap in a fresh inferer for inference, restore afterwards
        let mut temp_inferer = TypeInferer::new();
        std::mem::swap(&mut self.inferer, &mut temp_inferer);
        self.inferring = true;
        self.borrowed = 0;
        self.terminated = false;

        let mut errors = Vec::new();
        let mut stack: Vec<Type> = Vec::new();
        let mut min_depth: isize = 0;
        let mut input_types: Vec<Type> = Vec::new();

        for node in body {
            if self.terminated {
                break;
            }
            let _depth_before = stack.len() as isize;
            self.check_node(node, &mut stack, &mut errors, "<word>");
            if self.terminated {
                // return 后的栈形以当前深度封盘
            }
            let depth_after = stack.len() as isize;

            // Track minimum depth — when it goes negative, those are inputs
            if depth_after < min_depth {
                // The difference represents consumed inputs
                let consumed = (min_depth - depth_after) as usize;
                for _ in 0..consumed {
                    input_types.push(self.inferer.fresh_var());
                }
                min_depth = depth_after;
            }
        }

        // Discard underflow errors from inference — they happen when
        // the word consumes values; those become inputs to the signature.
        errors.retain(|e| {
            !e.message.contains("Stack underflow")
                && !e.message.contains("stack empty")
                && !e.message.contains("underflow")
        });

        // N-2 Phase 2.5：输入数以借位计数为准（欠压操作真实消耗的入参）
        let mut inputs: Vec<Type> = input_types
            .iter()
            .map(|t| self.inferer.resolve(t))
            .collect();
        while inputs.len() < self.borrowed {
            inputs.push(self.inferer.fresh_var());
        }
        self.inferring = false;
        let outputs: Vec<Type> = stack.iter().map(|t| self.inferer.resolve(t)).collect();

        // Restore the main inferer (discard inference temporaries)
        std::mem::swap(&mut self.inferer, &mut temp_inferer);

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
                    whisper_core::value::Value::Map(_) => Type::Map,
                    whisper_core::value::Value::List(items) => {
                        let elem = items
                            .first()
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
                    if let Some(sig) = self.word_sigs.get(name).cloned() {
                        // N-2 Phase 2 多态实例化：每次调用深拷贝签名并把
                        // TypeVar 整体换成新 var——签名里的 var id 若被两个
                        // 调用点共享，首个调用点的 unify 会污染全局（id:=str
                        // 连锁，"单形签名被一个调用点污染全程序"）。
                        let mut mapping: std::collections::HashMap<u64, Type> =
                            std::collections::HashMap::new();
                        let inputs: Vec<Type> = sig
                            .inputs
                            .iter()
                            .map(|t| self.freshen(t, &mut mapping))
                            .collect();
                        let outputs: Vec<Type> = sig
                            .outputs
                            .iter()
                            .map(|t| self.freshen(t, &mut mapping))
                            .collect();
                        Some((inputs, outputs))
                    } else if let Some((inputs, outputs)) = get_builtin_signature(name) {
                        // N-2 Phase 2.5：内建签名同样实例化——builtins 表里的
                        // TypeVar(99)/(0) 是固定 id，不换新则所有调用点共享
                        // 同一变量，首个调用点的绑定污染全局（t_base 最后一条
                        // FP 的根源）。
                        let mut mapping: std::collections::HashMap<u64, Type> =
                            std::collections::HashMap::new();
                        let i2: Vec<Type> = inputs
                            .iter()
                            .map(|t| self.freshen(t, &mut mapping))
                            .collect();
                        let o2: Vec<Type> = outputs
                            .iter()
                            .map(|t| self.freshen(t, &mut mapping))
                            .collect();
                        Some((i2, o2))
                    } else {
                        None
                    };

                if let Some((expected_inputs, expected_outputs)) = resolved_sig {
                    for expected in expected_inputs.iter().rev() {
                        if let Some(actual) = stack.pop() {
                            if let Err(e) = self.inferer.unify(expected, &actual) {
                                errors.push(TypeError {
                                    message: format!("Word '{}' type mismatch: {}", name, e),
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
                // N-2 Phase 1: body checked on (ambient Sigma, e1, e2) —
                // two entry slots covers @fold's (acc, elem); @map/@each
                // bodies treat the second slot as extra ambient.
                let ambient: usize = 4;
                let entry: usize = ambient + 2;
                let mut quote_stack: Vec<Type> =
                    (0..entry).map(|_| self.inferer.fresh_var()).collect();
                let mut low_water: usize = entry;
                let mut quote_errors = Vec::new();
                for n in body {
                    self.check_node(
                        n,
                        &mut quote_stack,
                        &mut quote_errors,
                        &format!("{ctx}/quote"),
                    );
                    low_water = low_water.min(quote_stack.len());
                }
                for err in &quote_errors {
                    if !err.message.contains("Stack underflow") {
                        errors.push(TypeError {
                            message: format!("[quote] {}", err.message),
                            context: err.context.clone(),
                        });
                    }
                }
                // Honest Ref arity from REACH (low-water), not net — a body
                // that produces-then-drops its own value must not be counted
                // as reaching into the ambient stack:
                //   in  = entry - low_water (how deep it truly dipped)
                //   out = slots above the entry boundary at the end
                let final_len = quote_stack.len();
                let in_arity = std::cmp::max(1, entry.saturating_sub(low_water));
                let inputs: Vec<Type> = (0..in_arity).map(|_| self.inferer.fresh_var()).collect();
                let keep_from = entry - in_arity;
                let outputs: Vec<Type> = if final_len > keep_from {
                    quote_stack[keep_from..].to_vec()
                } else {
                    vec![]
                };
                stack.push(Type::Ref(inputs, outputs));
            }

            AstNode::List(items) => {
                let mut elem_type: Option<Type> = None;
                for item in items {
                    self.check_node(item, stack, errors, ctx);
                    if let Some(popped) = stack.pop() {
                        if let Some(ref expected) = elem_type {
                            if self.inferer.unify(expected, &popped).is_err() {
                                // N-2 Phase 2.5：异构列表运行时合法（[1 2 [3 4]]）
                                // ——元素型升级为 union 而非报错
                                elem_type = Some(Type::Union(
                                    Box::new(expected.clone()),
                                    Box::new(popped.clone()),
                                ));
                            }
                        } else {
                            elem_type = Some(popped);
                        }
                    }
                }
                let elem = elem_type.unwrap_or_else(|| self.inferer.fresh_var());
                stack.push(Type::List(Box::new(self.inferer.resolve(&elem))));
            }

            AstNode::Cond {
                then_branch,
                else_branch,
            } => {
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
                // N-2 Phase 2.5 臂逐槽合并：两臂按槽位对齐合并（unify 成
                // 功用合并型，失败用 union），不再只取 last()——last() 丢
                // 掉其余槽位是共享闭括号多 ?? 链误报的根源。J-3 当年撤回
                // 是因被坏签名放大；Phase 2 调用点 freshen 后放大源已除。
                let then_tail = &then_stack[depth.min(then_stack.len())..];
                let else_tail = &else_stack[depth.min(else_stack.len())..];
                let n = then_tail.len().max(else_tail.len());
                for i in 0..n {
                    let t_slot = then_tail
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| self.inferer.fresh_var());
                    let e_slot = else_tail
                        .get(i)
                        .cloned()
                        .unwrap_or_else(|| self.inferer.fresh_var());
                    if self.inferer.unify(&t_slot, &e_slot).is_ok() {
                        stack.push(self.inferer.resolve(&t_slot));
                    } else {
                        stack.push(Type::Union(
                            Box::new(self.inferer.resolve(&t_slot)),
                            Box::new(self.inferer.resolve(&e_slot)),
                        ));
                    }
                }
            }

            AstNode::Loop { body, condition } => {
                // 运行时语义：条件在循环顶块先求值（block_gen Loop 也按此编译），
                // 类型检查按同序执行——先条件、弹出 bool、再查循环体。
                for n in condition {
                    self.check_node(n, stack, errors, &format!("{ctx}/loop-cond"));
                }
                self.expect(stack, &Type::Bool, errors, ctx, "loop condition");
                for n in body {
                    self.check_node(n, stack, errors, &format!("{ctx}/loop"));
                }
            }

            AstNode::Def { .. } | AstNode::Import(_) | AstNode::Export(_) => {}

            AstNode::Times { .. } => {
                self.expect(stack, &Type::I64, errors, ctx, "@times count");
                self.expect_ref(stack, errors, ctx, "@times quot");
            }

            AstNode::CondArrow { then_branch } => {
                self.expect(stack, &Type::Bool, errors, ctx, "?-> condition");
                let depth = stack.len();
                let mut then_stack = stack.clone();
                for n in then_branch {
                    self.check_node(n, &mut then_stack, errors, &format!("{ctx}/cond-arrow"));
                }
                // Take the suffix above original depth
                stack.truncate(depth);
                for t in then_stack.into_iter().skip(depth) {
                    stack.push(t);
                }
            }

            AstNode::ConfidenceLabel {
                body,
                confidence: _,
            } => {
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
                let r1 = s1
                    .last()
                    .cloned()
                    .unwrap_or_else(|| self.inferer.fresh_var());
                let r2 = s2
                    .last()
                    .cloned()
                    .unwrap_or_else(|| self.inferer.fresh_var());
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

    fn expect_n(
        &mut self,
        stack: &mut Vec<Type>,
        n: usize,
        errors: &mut Vec<TypeError>,
        ctx: &str,
        what: &str,
    ) {
        if stack.len() < n {
            errors.push(TypeError {
                message: format!("{}: need {} values, have {}", what, n, stack.len()),
                context: ctx.into(),
            });
            // 补齐占位，让后续推断继续
            for _ in 0..(n - stack.len()) {
                stack.push(self.inferer.fresh_var());
            }
        }
        for _ in 0..n {
            stack.pop();
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
                    if self.inferring {
                        // 借位：dup 的操作数来自词入参
                        self.borrowed += 1;
                        let f = self.inferer.fresh_var();
                        stack.push(f.clone());
                        stack.push(f);
                    } else {
                        errors.push(TypeError {
                            message: "Dup: stack empty".into(),
                            context: ctx.into(),
                        });
                    }
                } else {
                    let t = stack.last().unwrap().clone();
                    stack.push(t);
                }
            }
            Operator::Swap => {
                if stack.len() < 2 {
                    if self.inferring {
                        while stack.len() < 2 {
                            self.borrowed += 1;
                            let f = self.inferer.fresh_var();
                            stack.insert(0, f);
                        }
                    } else {
                        errors.push(TypeError {
                            message: "Swap: need 2 values".into(),
                            context: ctx.into(),
                        });
                    }
                }
                if stack.len() >= 2 {
                    let a = stack.pop().unwrap();
                    let b = stack.pop().unwrap();
                    stack.push(a);
                    stack.push(b);
                }
            }
            Operator::Drop => {
                if stack.is_empty() {
                    if self.inferring {
                        self.borrowed += 1; // 借位后即弃
                    } else {
                        errors.push(TypeError {
                            message: "Drop: stack empty".into(),
                            context: ctx.into(),
                        });
                    }
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
                } else {
                    let len = stack.len();
                    let c = stack.remove(len - 3);
                    stack.push(c);
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
            Operator::Eq
            | Operator::Lt
            | Operator::Gt
            | Operator::Neq
            | Operator::Le
            | Operator::Ge => {
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
            Operator::Return => {
                self.terminated = true;
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
            Operator::StrEq | Operator::StrLt => {
                self.expect(stack, &Type::Str, errors, ctx, "strcmp rhs");
                self.expect(stack, &Type::Str, errors, ctx, "strcmp lhs");
                stack.push(Type::Bool);
            }
            Operator::StrFind => {
                self.expect(stack, &Type::Str, errors, ctx, "strfind pattern");
                self.expect(stack, &Type::Str, errors, ctx, "strfind haystack");
                stack.push(Type::I64);
            }
            Operator::StrReplace => {
                self.expect(stack, &Type::Str, errors, ctx, "strreplace new");
                self.expect(stack, &Type::Str, errors, ctx, "strreplace old");
                self.expect(stack, &Type::Str, errors, ctx, "strreplace string");
                stack.push(Type::Str);
            }
            Operator::StrToI64 => {
                self.expect(stack, &Type::Str, errors, ctx, "strtoi64");
                stack.push(Type::I64);
            }
            Operator::I64ToStr => {
                self.expect(stack, &Type::I64, errors, ctx, "i64tostr");
                stack.push(Type::Str);
            }
            Operator::StrNth => {
                self.expect(stack, &Type::I64, errors, ctx, "strnth index");
                self.expect(stack, &Type::Str, errors, ctx, "strnth string");
                stack.push(Type::I64);
            }
            Operator::StrChars => {
                self.expect(stack, &Type::Str, errors, ctx, "strchars");
                stack.push(Type::List(Box::new(Type::I64)));
            }
            Operator::CharsStr => {
                self.expect(
                    stack,
                    &Type::List(Box::new(Type::I64)),
                    errors,
                    ctx,
                    "charsstr",
                );
                stack.push(Type::Str);
            }
            Operator::StrIter => {
                self.expect(stack, &Type::Str, errors, ctx, "striter");
                stack.push(Type::I64);
                stack.push(Type::Str);
            }
            Operator::ListFind => {
                let v = self.inferer.fresh_var();
                self.expect(stack, &v, errors, ctx, "listfind key");
                self.expect(
                    stack,
                    &Type::List(Box::new(Type::List(Box::new(v.clone())))),
                    errors,
                    ctx,
                    "listfind assoc-list",
                );
                stack.push(Type::Bool);
                stack.push(v);
            }
            Operator::StrJoin => {
                self.expect(
                    stack,
                    &Type::List(Box::new(Type::Str)),
                    errors,
                    ctx,
                    "strjoin",
                );
                stack.push(Type::Str);
            }
            Operator::BytesNew => {
                stack.push(Type::I64); // returns handle
            }
            Operator::BytesPush => {
                self.expect(stack, &Type::I64, errors, ctx, "bytes-push byte");
                self.expect(stack, &Type::I64, errors, ctx, "bytes-push handle");
                stack.push(Type::I64);
            }
            Operator::BytesLen => {
                self.expect(stack, &Type::I64, errors, ctx, "bytes-len handle");
                stack.push(Type::I64);
            }
            Operator::BytesToHex => {
                self.expect(stack, &Type::I64, errors, ctx, "bytes->hex handle");
                stack.push(Type::Str);
            }
            Operator::BytesWriteFile => {
                self.expect(stack, &Type::Str, errors, ctx, "bytes-write filename");
                self.expect(stack, &Type::I64, errors, ctx, "bytes-write handle");
            }
            Operator::Try => {
                // `try` CAPTURES its body's output, so the body may leave any
                // number of values — the VM pops one (or defaults to 0) and
                // returns it as the second element of `[flag, value]`.
                // Requiring the net-zero `[T] → [T]` consumer shape
                // (expect_ref) rejected exactly that: under the strict
                // default `{ 1 0 / } try drop ... ` failed with
                // "try quot: expected [T7] → [T7], got [T6] → [T5 i64|f64]"
                // even though the VM runs it fine. Any-output is the same
                // contract @each already uses.
                self.expect_ref_shape(stack, 0, None, errors, ctx, "try quot");
                stack.push(Type::Bool);
                stack.push(self.inferer.fresh_var());
            }

            Operator::CapExec => {
                // `call`: invoke whatever is on top of the stack. The VM
                // re-enters a `Value::Ref` against the caller's data stack, so
                // the quotation's declared outputs land on the enclosing
                // stack — model exactly that.
                let popped = stack.pop();
                match popped.as_ref().map(|t| self.inferer.resolve(t)) {
                    Some(Type::Ref(_, outs)) => stack.extend(outs),
                    // Capability tokens (`@n call`) and unresolved vars: the
                    // result shape is not modelled (the HTTP family is still
                    // unmodelled; @0/@1/@4/@5 are normally written directly).
                    Some(Type::Cap(_)) | Some(Type::TypeVar(_)) => {
                        stack.push(self.inferer.fresh_var());
                    }
                    Some(other) => errors.push(TypeError {
                        message: format!("call: expected quotation, got {}", other.name()),
                        context: ctx.into(),
                    }),
                    None => errors.push(TypeError {
                        message: "Stack underflow: call needs a quotation".into(),
                        context: ctx.into(),
                    }),
                }
            }

            // Float ops
            Operator::I64ToF64 => {
                self.expect(stack, &Type::I64, errors, ctx, "i64tof64");
                stack.push(Type::F64);
            }
            Operator::F64ToI64 => {
                self.expect(stack, &Type::F64, errors, ctx, "f64toi64");
                stack.push(Type::I64);
            }
            Operator::FSqrt | Operator::FSin | Operator::FCos | Operator::FTan => {
                self.expect(stack, &Type::F64, errors, ctx, "float math");
                stack.push(Type::F64);
            }

            // JSON
            Operator::JsonParse => {
                self.expect(stack, &Type::Str, errors, ctx, "json-parse");
                stack.push(self.inferer.fresh_var());
            }
            Operator::JsonStringify => {
                stack.pop(); // accept any value
                stack.push(Type::Str);
            }

            // Time
            Operator::Now => {
                stack.push(Type::I64);
            }
            Operator::Sleep => {
                self.expect(stack, &Type::I64, errors, ctx, "sleep");
            }

            // Random
            Operator::Random => {
                stack.push(Type::F64);
            }
            Operator::RandInt => {
                self.expect(stack, &Type::I64, errors, ctx, "randint max");
                self.expect(stack, &Type::I64, errors, ctx, "randint min");
                stack.push(Type::I64);
            }

            // Float math
            Operator::Floor | Operator::Ceil | Operator::Round | Operator::Log | Operator::Exp => {
                self.expect(stack, &Type::F64, errors, ctx, "float math");
                stack.push(Type::F64);
            }
            Operator::Atan2 => {
                self.expect(stack, &Type::F64, errors, ctx, "atan2 x");
                self.expect(stack, &Type::F64, errors, ctx, "atan2 y");
                stack.push(Type::F64);
            }

            // Bitwise
            Operator::BitAnd | Operator::BitOr | Operator::BitXor => {
                self.expect(stack, &Type::I64, errors, ctx, "bitwise rhs");
                self.expect(stack, &Type::I64, errors, ctx, "bitwise lhs");
                stack.push(Type::I64);
            }
            Operator::BitNot => {
                self.expect(stack, &Type::I64, errors, ctx, "bitnot");
                stack.push(Type::I64);
            }

            // Sort
            Operator::Sort => {
                let t = self.inferer.fresh_var();
                self.expect(stack, &Type::List(Box::new(t.clone())), errors, ctx, "sort");
                stack.push(Type::List(Box::new(t)));
            }
            Operator::Rev => {
                let t = self.inferer.fresh_var();
                self.expect(stack, &Type::List(Box::new(t.clone())), errors, ctx, "rev");
                stack.push(Type::List(Box::new(t)));
            }
            Operator::Dedup => {
                let t = self.inferer.fresh_var();
                self.expect(
                    stack,
                    &Type::List(Box::new(t.clone())),
                    errors,
                    ctx,
                    "dedup",
                );
                stack.push(Type::List(Box::new(t)));
            }
            Operator::Take => {
                self.expect(stack, &Type::I64, errors, ctx, "take n");
                let t = self.inferer.fresh_var();
                self.expect(
                    stack,
                    &Type::List(Box::new(t.clone())),
                    errors,
                    ctx,
                    "take list",
                );
                stack.push(Type::List(Box::new(t)));
            }
            Operator::DropN => {
                self.expect(stack, &Type::I64, errors, ctx, "drop-n n");
                let t = self.inferer.fresh_var();
                self.expect(
                    stack,
                    &Type::List(Box::new(t.clone())),
                    errors,
                    ctx,
                    "drop-n list",
                );
                stack.push(Type::List(Box::new(t)));
            }

            Operator::Nth => {
                // The element type must flow: @nth on List(T) yields T.
                let elem = self.inferer.fresh_var();
                self.expect(stack, &Type::I64, errors, ctx, "@nth index");
                self.expect(
                    stack,
                    &Type::List(Box::new(elem.clone())),
                    errors,
                    ctx,
                    "@nth list",
                );
                stack.push(elem);
            }
            Operator::Append => {
                // ( list elem -- list' ): elem must unify with the list's
                // element type and the result keeps the unified type.
                let elem_ty = stack.pop().unwrap_or_else(|| self.inferer.fresh_var());
                let elem = self.inferer.fresh_var();
                self.expect(
                    stack,
                    &Type::List(Box::new(elem.clone())),
                    errors,
                    ctx,
                    "append list",
                );
                if self.inferer.unify(&elem, &elem_ty).is_err() {
                    errors.push(TypeError {
                        message: format!(
                            "append element: expected {}, got {}",
                            self.inferer.resolve(&elem),
                            self.inferer.resolve(&elem_ty),
                        ),
                        context: ctx.to_string(),
                    });
                }
                stack.push(Type::List(Box::new(elem)));
            }
            Operator::Prepend => {
                let t = self.inferer.fresh_var();
                self.expect(
                    stack,
                    &Type::List(Box::new(t.clone())),
                    errors,
                    ctx,
                    "prepend list",
                );
                self.expect(stack, &t, errors, ctx, "prepend elem");
                stack.push(Type::List(Box::new(t)));
            }
            Operator::Map => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref_shape(stack, 1, Some(1), errors, ctx, "@map quot");
                self.expect(stack, &any_list, errors, ctx, "@map list");
                stack.push(Type::List(Box::new(self.inferer.fresh_var())));
            }
            Operator::Each => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref_shape(stack, 1, None, errors, ctx, "@each quot");
                self.expect(stack, &any_list, errors, ctx, "@each list");
            }
            Operator::Fold => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref_shape(stack, 2, Some(1), errors, ctx, "@fold quot");
                stack.pop(); // init — accept anything
                self.expect(stack, &any_list, errors, ctx, "@fold list");
                stack.push(self.inferer.fresh_var());
            }
            Operator::Filter => {
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect_ref_shape(stack, 1, Some(1), errors, ctx, "@filter quot");
                self.expect(stack, &any_list, errors, ctx, "@filter list");
                stack.push(Type::List(Box::new(self.inferer.fresh_var())));
            }
            Operator::StrSplit => {
                self.expect(stack, &Type::Str, errors, ctx, "strsplit delimiter");
                self.expect(stack, &Type::Str, errors, ctx, "strsplit input");
                stack.push(Type::List(Box::new(Type::Str)));
            }
            Operator::Ctos => {
                self.expect(stack, &Type::I64, errors, ctx, "ctos codepoint");
                stack.push(Type::Str);
            }
            Operator::ListSlice => {
                self.expect(stack, &Type::I64, errors, ctx, "listslice len");
                self.expect(stack, &Type::I64, errors, ctx, "listslice start");
                let any_list = Type::List(Box::new(self.inferer.fresh_var()));
                self.expect(stack, &any_list, errors, ctx, "listslice list");
                stack.push(Type::List(Box::new(self.inferer.fresh_var())));
            }
            Operator::Shl | Operator::Shr => {
                self.expect(stack, &Type::I64, errors, ctx, "shift amount");
                self.expect(stack, &Type::I64, errors, ctx, "shift value");
                stack.push(Type::I64);
            }
            Operator::Args => {
                stack.push(Type::List(Box::new(Type::Str)));
            }
            Operator::AtTimes => {
                // times body runs N times with free effects: no shape check
                if stack.pop().is_none() {
                    errors.push(TypeError {
                        message: "Stack underflow: @times body needs quote".into(),
                        context: ctx.into(),
                    });
                }
                self.expect(stack, &Type::I64, errors, ctx, "@times count");
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

            // Map operations
            Operator::MapNew => {
                stack.push(Type::Map);
            }
            Operator::MapGet => {
                self.expect(stack, &Type::Str, errors, ctx, "map-get key");
                self.expect(stack, &Type::Map, errors, ctx, "map-get map");
                stack.push(self.inferer.fresh_var()); // value (any type)
            }
            Operator::MapSet => {
                // VM 弹序：[map, key, val]，值在顶（惯用形："键" 值 map-set）
                let v = self.inferer.fresh_var();
                self.expect(stack, &v, errors, ctx, "map-set value");
                self.expect(stack, &Type::Str, errors, ctx, "map-set key");
                self.expect(stack, &Type::Map, errors, ctx, "map-set map");
                stack.push(Type::Map);
            }
            Operator::MapDel => {
                self.expect(stack, &Type::Str, errors, ctx, "map-del key");
                self.expect(stack, &Type::Map, errors, ctx, "map-del map");
                stack.push(Type::Map);
            }
            Operator::MapKeys => {
                self.expect(stack, &Type::Map, errors, ctx, "map-keys map");
                stack.push(Type::List(Box::new(Type::Str)));
            }
            Operator::MapHas => {
                self.expect(stack, &Type::Str, errors, ctx, "map-has key");
                self.expect(stack, &Type::Map, errors, ctx, "map-has map");
                stack.push(Type::Bool);
            }

            // KV：键值均为字符串（与 vm.rs KvSet/KvGet 口径一致）
            Operator::OpEffect => {
                self.expect_n(stack, 1, errors, ctx, "op-effect");
                stack.push(Type::I64);
                stack.push(Type::I64);
            }
            Operator::QuoteSize => {
                self.expect_n(stack, 1, errors, ctx, "size-of");
                stack.push(Type::I64);
            }
            Operator::WordSize => {
                self.expect_n(stack, 1, errors, ctx, "word-size");
                stack.push(Type::I64);
            }
            Operator::SetSeed => {
                self.expect_n(stack, 1, errors, ctx, "seed!");
            }
            Operator::Rand => {
                stack.push(Type::F64);
            }
            Operator::ConfOf => {
                // ( v -- v f64 ) 非破坏读置信度
                stack.push(Type::F64);
            }
            Operator::UnwrapSig => {
                self.expect_n(stack, 1, errors, ctx, "unwrap");
                stack.push(self.inferer.fresh_var());
            }
            Operator::KvSet => {
                self.expect_n(stack, 2, errors, ctx, "kv-set");
            }
            Operator::KvGet => {
                self.expect_n(stack, 1, errors, ctx, "kv-get");
                stack.push(Type::Str);
            }
            Operator::KvDel => {
                self.expect_n(stack, 1, errors, ctx, "kv-del");
            }
            Operator::KvKeys => {
                stack.push(Type::List(Box::new(Type::Str)));
            }
            Operator::ReadFile => {
                self.expect_n(stack, 1, errors, ctx, "read-file");
                stack.push(Type::Str);
            }
            Operator::WriteFile => {
                self.expect_n(stack, 2, errors, ctx, "write-file");
                stack.push(Type::I64);
            }
            Operator::StrFindRe => {
                self.expect_n(stack, 2, errors, ctx, "strfind-re");
                stack.push(Type::I64);
            }
            Operator::StrMatch => {
                self.expect_n(stack, 2, errors, ctx, "strmatch");
                stack.push(Type::Bool);
            }
            Operator::StrUpper | Operator::StrLower | Operator::StrStrip => {
                self.expect_n(stack, 1, errors, ctx, "str-case/strip");
                stack.push(Type::Str);
            }
            Operator::Range => {
                self.expect_n(stack, 2, errors, ctx, "range");
                stack.push(Type::List(Box::new(Type::I64)));
            }
            Operator::Pick(n) => {
                // 非破坏性窥视：pick 复制第 n 值（不弹任何操作数）
                let idx = n as usize; // 距栈顶第 idx 个（0=顶）
                if stack.len() > idx {
                    let t = stack[stack.len() - 1 - idx].clone();
                    stack.push(t);
                } else {
                    if stack.is_empty() {
                        errors.push(TypeError {
                            message: "pick: stack empty".into(),
                            context: ctx.into(),
                        });
                    }
                    stack.push(self.inferer.fresh_var());
                }
            }
            Operator::I64Check | Operator::StrCheck | Operator::BoolCheck | Operator::ListCheck => {
                self.expect_n(stack, 1, errors, ctx, "type-check");
                stack.push(Type::Bool);
            }
            Operator::EnvGet => {
                self.expect_n(stack, 1, errors, ctx, "env-get");
                stack.push(Type::Str);
            }
            Operator::FileExists => {
                self.expect_n(stack, 1, errors, ctx, "file-exists");
                stack.push(Type::Bool);
            }

            // Capability tokens (`@n`) used to fall through to the catch-all
            // below, which models nothing: neither consuming the operand nor
            // producing a result. Under the strict default that rejected
            // perfectly good code — `"cmd" exec 1 @nth .` reported
            // "@nth list: expected [T0], got str" because the checker still
            // believed the command string was on top. Same class of gap as
            // the http-* builtin signatures fixed in J-3.
            //
            // Only the ids the C chain can also compile are modelled here;
            // the HTTP family (2/3/6/7/8/9) stays unmodelled until Step 2b
            // gives it a C-side implementation (docs/independence-design.md).
            Operator::CapCall(id) => match id {
                0 => {
                    // @file_read: path -> contents
                    self.expect_n(stack, 1, errors, ctx, "@0 file-read");
                    stack.push(Type::Str);
                }
                1 => {
                    // @file_write: path data -> bytes written
                    self.expect_n(stack, 2, errors, ctx, "@1 file-write");
                    stack.push(Type::I64);
                }
                4 => {
                    // @env: name -> value
                    self.expect_n(stack, 1, errors, ctx, "@4 env");
                    stack.push(Type::Str);
                }
                5 => {
                    // @exec: cmd -> [status stdout stderr] — heterogeneous,
                    // so the element type is the union the language already
                    // uses for mixed lists.
                    self.expect_n(stack, 1, errors, ctx, "@5 exec");
                    stack.push(Type::List(Box::new(Type::Union(
                        Box::new(Type::I64),
                        Box::new(Type::Str),
                    ))));
                }
                _ => {}
            },

            _ => {} // CapExec, Cond, etc. unchecked
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
                if self.inferer.unify(expected, &actual).is_err() {
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
                if self.inferring {
                    // 借位：本槽是词入参，检查继续（消费即输入）
                    self.borrowed += 1;
                } else {
                    errors.push(TypeError {
                        message: format!("Stack underflow: {} needs {}", desc, expected),
                        context: ctx.to_string(),
                    });
                }
            }
        }
    }

    /// N-2 Phase 2: structural deep-copy replacing TypeVar ids with fresh
    /// vars (consistent within one instantiation).
    fn freshen(&mut self, t: &Type, mapping: &mut std::collections::HashMap<u64, Type>) -> Type {
        match t {
            Type::TypeVar(id) => {
                if let Some(f) = mapping.get(id) {
                    return f.clone();
                }
                let f = self.inferer.fresh_var();
                mapping.insert(*id, f.clone());
                f
            }
            Type::List(inner) => Type::List(Box::new(self.freshen(inner, mapping))),
            Type::Ref(i, o) => Type::Ref(
                i.iter().map(|t| self.freshen(t, mapping)).collect(),
                o.iter().map(|t| self.freshen(t, mapping)).collect(),
            ),
            Type::Signal(inner) => Type::Signal(Box::new(self.freshen(inner, mapping))),
            Type::Union(a, b) => Type::Union(
                Box::new(self.freshen(a, mapping)),
                Box::new(self.freshen(b, mapping)),
            ),
            other => other.clone(),
        }
    }

    /// N-2 Phase 1 consumer contract: arity-only checks (slot types are
    /// Phase 2 polymorphism). Unresolved TypeVar quotes pass (no false pos).
    #[allow(clippy::too_many_arguments)]
    fn expect_ref_shape(
        &mut self,
        stack: &mut Vec<Type>,
        in_provided: usize,
        out_arity: Option<usize>,
        errors: &mut Vec<TypeError>,
        ctx: &str,
        desc: &str,
    ) {
        let popped = stack.pop();
        let resolved = popped.as_ref().map(|t| self.inferer.resolve(t));
        match resolved {
            Some(Type::Ref(i, o)) => {
                if let Some(n) = out_arity {
                    // 消费者视角产出：Ref 的 out 只数到自声明 in 之上；
                    // 消费者多提供的槽（@fold 的 acc）也应计入产出底座
                    let provided = in_provided;
                    let out_eff = o.len() + provided.saturating_sub(i.len());
                    if out_eff < n {
                        errors.push(TypeError {
                            message: format!(
                                "{}: quote produces {} value(s), consumer needs at least {}",
                                desc, out_eff, n
                            ),
                            context: ctx.into(),
                        });
                    }
                }
            }
            Some(Type::TypeVar(_)) => {}
            Some(other) => errors.push(TypeError {
                message: format!("{}: expected quote, got {}", desc, other.name()),
                context: ctx.into(),
            }),
            None => errors.push(TypeError {
                message: format!("Stack underflow: {} needs quote", desc),
                context: ctx.into(),
            }),
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
        whisper_core::value::Value::Map(_) => Type::Map,
        whisper_core::value::Value::List(items) => {
            let elem = items
                .first()
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
        assert!(!errors.is_empty(), "Expected errors for: {source}");
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
            !sig.outputs.is_empty(),
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

#[cfg(test)]
mod nth_append_tests {
    use super::*;
    use whisper_parser::Parser;

    fn errors_for(source: &str) -> Vec<TypeError> {
        let ast = Parser::parse_source(source).unwrap();
        let mut tc = TypeChecker::new();
        tc.check(&ast)
    }

    #[test]
    fn nth_propagates_element_type() {
        // @nth on a list of i64 yields i64 — feeding it to `len` (which
        // wants a list) must now error instead of passing silently.
        let errs = errors_for("[1 2 3] 0 @nth len");
        assert!(!errs.is_empty(), "@nth result type must flow into len");
    }

    #[test]
    fn nth_on_homogeneous_strings() {
        let errs = errors_for("[\"a\" \"b\"] 0 @nth strlen 0 >");
        assert!(errs.is_empty(), "expected clean: {errs:?}");
    }

    #[test]
    fn append_rejects_mismatched_element() {
        let errs = errors_for("[1 2 3] \"x\" append");
        assert!(
            !errs.is_empty(),
            "appending a string to List(I64) must error"
        );
    }

    #[test]
    fn append_homogeneous_ok_and_typed() {
        // After append the list is still List(I64): @nth feeds strlen ->
        // must error, proving the element type survived.
        let errs = errors_for("[1 2 3] 4 append 0 @nth strlen");
        assert!(!errs.is_empty(), "append must keep the element type");
        let ok = errors_for("[1 2 3] 4 append 0 @nth 1 +");
        assert!(ok.is_empty(), "expected clean: {ok:?}");
    }

    #[test]
    fn occurs_check_rejects_infinite_type() {
        // The checker gives recursive calls fresh polymorphic vars (the
        // documented anti-false-positive policy), so infinite types are
        // only reachable at the unifier level: t0 := List(t0) must fail.
        let mut inf = crate::infer::TypeInferer::new();
        let t0 = inf.fresh_var();
        let t0_id = match &t0 {
            crate::types::Type::TypeVar(id) => *id,
            _ => unreachable!(),
        };
        let lhs = inf.fresh_var();
        let list_of_lhs = crate::types::Type::List(Box::new(lhs.clone()));
        let _ = inf.unify(&lhs, &list_of_lhs); // lhs := List(lhs) — ok so far
        let r = inf.unify(
            &crate::types::Type::TypeVar(t0_id),
            &crate::types::Type::List(Box::new(crate::types::Type::TypeVar(t0_id))),
        );
        assert!(r.is_err(), "occurs-check must reject t := List(t)");
    }
}

/// One-line description of a node for tc-trace (N-2 Phase 2.5 debug aid).
fn node_brief(node: &AstNode) -> String {
    match node {
        AstNode::WordRef(n) => format!("word:{n}"),
        AstNode::Literal(v) => format!("lit:{}", v.type_name()),
        AstNode::Op(o) => format!("op:{o:?}"),
        AstNode::Quote(_) => "quote".into(),
        AstNode::List(items) => format!("list[{}]", items.len()),
        other => format!("{:?}", std::mem::discriminant(other)),
    }
}
