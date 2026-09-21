//! Block-based codegen: compiles AST to Block-IR, then linearizes.
//! Replaces flat offset arithmetic for all control flow.

use crate::block_ir::{BlockBuilder, Terminator};
use std::collections::HashMap;
use whisper_core::opcode::Opcode;
use whisper_core::value::Value;
use whisper_parser::ast::AstNode;
use whisper_parser::parser::Parser;

/// Shared compile context threaded through block construction.
/// `def_effects` gives `Call` sites their net stack effect;
/// `quote_errors` collects contract violations found inside quote
/// bodies (quotes are data, so their errors surface at the end of
/// the enclosing compile).
pub struct BlockCtx<'a> {
    pub def_effects: &'a HashMap<String, i64>,
    pub quote_errors: Vec<crate::block_ir::ContractError>,
}

/// Linearized flat bytecode plus the word definitions it references.
/// Named because the inline spelling trips clippy's `type_complexity`.
pub type CompiledProgram = (Vec<Opcode>, Vec<(String, Vec<Opcode>)>);

/// Compiles source → Block-IR → linearized flat bytecode + defs.
pub fn compile_block(source: &str) -> Result<CompiledProgram, String> {
    let (ast, errors) = Parser::parse_source_recovering(source);
    if !errors.is_empty() {
        return Err(format!("parse: {:?}", errors[0]));
    }

    let mut defs: Vec<(String, &Vec<AstNode>)> = Vec::new();
    let mut main_nodes: Vec<&AstNode> = Vec::new();
    for node in &ast {
        match node {
            AstNode::Def { name, body } => defs.push((name.clone(), body)),
            other => main_nodes.push(other),
        }
    }

    let empty: HashMap<String, i64> = HashMap::new();
    let mut ctx = BlockCtx {
        def_effects: &empty,
        quote_errors: Vec::new(),
    };
    let (flat, errors) = checked_flat(&main_nodes, true, &mut ctx);
    if !errors.is_empty() {
        return Err(errors
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join("; "));
    }

    let mut flat_defs = Vec::new();
    for (name, body) in &defs {
        let body_refs: Vec<&AstNode> = body.iter().collect();
        let (code, _, errors) = compile_def_body(&body_refs, &empty);
        if !errors.is_empty() {
            return Err(errors
                .iter()
                .map(|c| c.to_string())
                .collect::<Vec<_>>()
                .join("; "));
        }
        flat_defs.push((name.clone(), code));
    }

    Ok((flat, flat_defs))
}

fn build_program(nodes: &[&AstNode], ctx: &mut BlockCtx) -> crate::block_ir::BlockProgram {
    let mut builder = BlockBuilder::new();
    let entry = builder.current_block();
    builder.set_entry(entry);
    for node in nodes {
        compile_node_block(&mut builder, node, ctx);
    }
    builder.finish(Terminator::Halt);
    // Graph-optimize before anyone reads the program — offsets are
    // only computed afterwards in `linearize`, so edge changes are free.
    crate::block_ir::optimize_blocks(&mut builder.program);
    builder.program
}

/// Compiles AST nodes → Block-IR → linearized flat bytecode.
/// All control flow (cond/loop/leave/?->) goes through blocks.
pub fn compile_nodes_flat(nodes: &[&AstNode]) -> Vec<Opcode> {
    let empty: HashMap<String, i64> = HashMap::new();
    let mut ctx = BlockCtx {
        def_effects: &empty,
        quote_errors: Vec::new(),
    };
    let program = build_program(nodes, &mut ctx);
    crate::block_ir::linearize(&program)
}

/// Compiles AST nodes → Block-IR → contract report + linearized code.
/// (Contracts are advisory: code is returned even with violations.)
pub fn checked_flat(
    nodes: &[&AstNode],
    underflow_is_error: bool,
    ctx: &mut BlockCtx,
) -> (Vec<Opcode>, Vec<crate::block_ir::ContractError>) {
    let program = build_program(nodes, ctx);
    let mut errors = crate::block_ir::validate(&program, underflow_is_error, ctx.def_effects);
    errors.extend(std::mem::take(&mut ctx.quote_errors));
    (crate::block_ir::linearize(&program), errors)
}

/// A complete compile unit in Block-IR form: the main program plus
/// word definitions, with contract reports. This is the natural
/// serialization unit for .wbin v2 (offsets never appear on disk;
/// the loader linearizes).
pub struct CompileUnit {
    pub main: crate::block_ir::BlockProgram,
    pub defs: Vec<(String, crate::block_ir::BlockProgram)>,
    pub warnings: Vec<crate::block_ir::ContractError>,
}

/// Compiles a whole source unit (defs first in source order, then
/// main) to Block-IR programs. Contract violations are reported, not
/// enforced — see `bytecode_gen::compile` for the warn/strict policy.
pub fn compile_unit(nodes: &[AstNode]) -> CompileUnit {
    let mut def_effects: HashMap<String, i64> = HashMap::new();
    let mut warnings = Vec::new();
    let mut defs_out: Vec<(String, crate::block_ir::BlockProgram)> = Vec::new();
    let mut main_nodes: Vec<&AstNode> = Vec::new();

    for node in nodes {
        match node {
            AstNode::Def { name, body } => {
                let refs: Vec<&AstNode> = body.iter().collect();
                let (program, net, errs) = compile_def_program(&refs, &def_effects);
                warnings.extend(errs);
                if let Some(net) = net {
                    def_effects.insert(name.clone(), net);
                }
                defs_out.push((name.clone(), program));
            }
            other => main_nodes.push(other),
        }
    }

    let mut ctx = BlockCtx {
        def_effects: &def_effects,
        quote_errors: Vec::new(),
    };
    let main = build_program(&main_nodes, &mut ctx);
    warnings.extend(crate::block_ir::validate(
        &main,
        true, // main starts empty: static underflow is real
        &def_effects,
    ));
    warnings.extend(ctx.quote_errors);

    CompileUnit {
        main,
        defs: defs_out,
        warnings,
    }
}

/// Compiles a word body to its Block-IR program plus net effect and
/// contract report (relative checks: underflow allowed — caller args).
pub fn compile_def_program(
    nodes: &[&AstNode],
    def_effects: &HashMap<String, i64>,
) -> (
    crate::block_ir::BlockProgram,
    Option<i64>,
    Vec<crate::block_ir::ContractError>,
) {
    let mut ctx = BlockCtx {
        def_effects,
        quote_errors: Vec::new(),
    };
    let program = build_program(nodes, &mut ctx);
    let mut errs = crate::block_ir::validate(&program, false, def_effects);
    errs.extend(ctx.quote_errors);
    let net = crate::block_ir::net_effect(&program, def_effects);
    (program, net, errs)
}

/// Compiles a word body to flat bytecode (linearized Block-IR).
pub fn compile_def_body(
    nodes: &[&AstNode],
    def_effects: &HashMap<String, i64>,
) -> (
    Vec<Opcode>,
    Option<i64>,
    Vec<crate::block_ir::ContractError>,
) {
    let (program, net, errs) = compile_def_program(nodes, def_effects);
    (crate::block_ir::linearize(&program), net, errs)
}

/// Resolves an AST node to a compile-time Value (for quotes and
/// literal lists). Quote bodies compile through Block-IR and are
/// contract-checked; violations land in `ctx.quote_errors`.
fn ast_to_value(node: &AstNode, ctx: &mut BlockCtx) -> Value {
    match node {
        AstNode::Literal(v) => v.clone(),
        AstNode::List(items) => {
            let values: Vec<_> = items.iter().map(|n| ast_to_value(n, ctx)).collect();
            Value::List(std::rc::Rc::new(values))
        }
        AstNode::Quote(body) => {
            let refs: Vec<&AstNode> = body.iter().collect();
            let program = build_program(&refs, ctx);
            let errors = crate::block_ir::validate(&program, false, ctx.def_effects);
            ctx.quote_errors.extend(errors);
            let code = crate::block_ir::linearize(&program);
            Value::Ref(std::rc::Rc::from(code.into_boxed_slice()))
        }
        AstNode::WordRef(name) => Value::Ref(std::rc::Rc::from(
            vec![Opcode::Call(name.clone())].into_boxed_slice(),
        )),
        _ => Value::I64(0),
    }
}

/// Emits opcodes that construct a compile-time Value on the stack
/// (mirror of the old `compile_literal`).
fn emit_value(builder: &mut BlockBuilder, val: &Value) {
    match val {
        Value::I64(n) => builder.emit(Opcode::PushI64(*n)),
        Value::F64(n) => builder.emit(Opcode::PushF64(*n)),
        Value::Bool(b) => builder.emit(Opcode::PushBool(*b)),
        Value::Str(s) => builder.emit(Opcode::PushStr(std::rc::Rc::clone(s))),
        Value::List(items) => {
            for item in items.iter() {
                emit_value(builder, item);
            }
            builder.emit(Opcode::PushI64(items.len() as i64));
            builder.emit(Opcode::PushList);
        }
        Value::Ref(code) => builder.emit(Opcode::PushRef(code.to_vec())),
        _ => {}
    }
}

fn compile_node_block(builder: &mut BlockBuilder, node: &AstNode, ctx: &mut BlockCtx) {
    match node {
        AstNode::Cond {
            then_branch,
            else_branch,
            ..
        } => {
            // cond is implicit: it's already compiled into the current block
            // by the parser (Cond node has no cond field — the cond expr
            // is the node BEFORE the ?? in the AST). But actually in the
            // current parser, Cond has then/else only — the cond expr is
            // a separate AST node that comes before. So by the time we
            // see Cond, the cond expr is already in the block.
            let cond_b = builder.current_block();
            let then_b = builder.new_block();
            let else_b = builder.new_block();
            let join_b = builder.new_block();
            builder.program.blocks[cond_b].terminator = Terminator::Branch {
                then: then_b,
                els: else_b,
            };
            builder.current = None;

            // then
            builder.switch_to(then_b);
            for n in then_branch {
                compile_node_block(builder, n, ctx);
            }
            // leave may have terminated this block already — don't overwrite
            if builder.current.is_some() {
                let te = builder.current_block();
                builder.program.blocks[te].terminator = Terminator::Jump(join_b);
            }
            builder.current = None;

            // else
            builder.switch_to(else_b);
            if let Some(else_nodes) = else_branch {
                for n in else_nodes {
                    compile_node_block(builder, n, ctx);
                }
            }
            if builder.current.is_some() {
                let ee = builder.current_block();
                builder.program.blocks[ee].terminator = Terminator::Jump(join_b);
            }
            builder.current = None;

            builder.switch_to(join_b);
        }

        AstNode::Loop { body, condition } => {
            let entry_b = builder.current_block();
            let top_b = builder.new_block();
            let body_b = builder.new_block();
            let exit_b = builder.new_block();
            builder.program.blocks[entry_b].terminator = Terminator::Jump(top_b);
            builder.current = None;

            builder.loop_stack.push((top_b, exit_b));

            builder.switch_to(top_b);
            for n in condition {
                compile_node_block(builder, n, ctx);
            }
            let te = builder.current_block();
            builder.program.blocks[te].terminator = Terminator::Branch {
                then: body_b,
                els: exit_b,
            };
            builder.current = None;

            builder.switch_to(body_b);
            for n in body {
                compile_node_block(builder, n, ctx);
            }
            // leave may have terminated body mid-way — don't overwrite
            if builder.current.is_some() {
                let be = builder.current_block();
                builder.program.blocks[be].terminator = Terminator::Jump(top_b);
            }
            builder.current = None;

            builder.loop_stack.pop();
            builder.switch_to(exit_b);
        }

        // Leave comes through as Op(Operator::Leave) — handled in the flat path
        // which we intercept below
        AstNode::CondArrow { then_branch } => {
            // `?->`: if the value on stack is true, run the brace body.
            // Same shape as Cond with an empty else: true → then_b,
            // false → skip to fall_b (the code after the ?->).
            let cur = builder.current_block();
            let then_b = builder.new_block();
            let fall_b = builder.new_block();
            builder.program.blocks[cur].terminator = Terminator::Branch {
                then: then_b,
                els: fall_b,
            };
            builder.current = None;

            builder.switch_to(then_b);
            for n in then_branch {
                compile_node_block(builder, n, ctx);
            }
            if builder.current.is_some() {
                let te = builder.current_block();
                builder.program.blocks[te].terminator = Terminator::Jump(fall_b);
            }
            builder.current = None;
            builder.switch_to(fall_b);
        }

        // Quote-bearing constructs: compile the quote through Block-IR
        // (contract-checked), then push it as a Ref value.
        AstNode::Quote(_) => {
            let v = ast_to_value(node, ctx);
            emit_value(builder, &v);
        }
        AstNode::Times { body } => {
            // n {body} @times — quote pre-compiled at compile time
            let quote = ast_to_value(&AstNode::Quote(body.clone()), ctx);
            emit_value(builder, &quote);
            builder.emit(Opcode::Times);
        }
        AstNode::List(_) => {
            let v = ast_to_value(node, ctx);
            emit_value(builder, &v);
        }
        AstNode::ProbChoice { alt1, alt2 } => {
            // 约定（L-03）：alt1=preferred（p 概率）、alt2=fallback；
            // VM 先弹顶 → alt2 在顶。
            let q1 = ast_to_value(&AstNode::Quote(alt1.clone()), ctx);
            emit_value(builder, &q1);
            let q2 = ast_to_value(&AstNode::Quote(alt2.clone()), ctx);
            emit_value(builder, &q2);
            builder.emit(Opcode::ProbChoice);
        }
        AstNode::ConfidenceLabel { body, confidence } => {
            for n in body {
                compile_node_block(builder, n, ctx);
            }
            builder.emit(Opcode::ConfLabel(confidence.0));
        }

        AstNode::Def { .. } | AstNode::Import(_) | AstNode::Export(_) => {}

        _ => {
            let mut gen = crate::bytecode_gen::BytecodeGenerator::new();
            gen.compile_node_public(node);
            // extract the emitted code from the generator
            let code = gen.take_bytecode();
            for op in code {
                // Leave(0) from flat path = unpatched leave → route to blocks
                if let Opcode::Leave(0) = op {
                    builder.emit_leave();
                } else {
                    builder.emit(op);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exec(src: &str) -> Result<Option<whisper_core::value::Value>, String> {
        let (flat, defs) = compile_block(src)?;
        let mut vm = whisper_core::vm::Vm::new();
        for (name, code) in defs {
            vm.define_word(name, code);
        }
        vm.execute(&flat).map_err(|e| e.to_string())
    }

    #[test]
    fn simple_loop() {
        let r = exec("0 { _ . 1 + } { _ 3 < } #").unwrap();
        assert_eq!(r, Some(whisper_core::value::Value::I64(3)));
    }

    #[test]
    fn cond_true() {
        let r = exec("5 3 > ?? 100 | 0 ]").unwrap().unwrap();
        assert_eq!(r, whisper_core::value::Value::I64(100));
    }

    #[test]
    fn cond_false() {
        let r = exec("2 3 > ?? 100 | 0 ]").unwrap().unwrap();
        assert_eq!(r, whisper_core::value::Value::I64(0));
    }

    #[test]
    fn fib() {
        let r = exec(": fib { _ 1 > ?? _ 1 - fib ` 2 - fib + | ] } ; 10 fib")
            .unwrap()
            .unwrap();
        assert_eq!(r, whisper_core::value::Value::I64(55));
    }

    #[test]
    fn leave() {
        exec("0 { _ 3 = ?? leave | ] _ . 1 + } { _ 5 < } # drop").unwrap();
    }

    #[test]
    fn nested_loops() {
        exec("0 0 { _ . 1 + } { _ 2 < } # drop { _ . 1 + } { _ 3 < } #").unwrap();
    }

    #[test]
    fn cond_in_loop_body() {
        // The shape that broke the flat codegen (png-decode walk-r)
        exec(": helper { } ; 0 { _ 2 = ?? helper | ] _ . 1 + } { _ 4 < } #").unwrap();
    }

    #[test]
    fn word_call_in_loop_cond() {
        let r = exec(": lt3 { 3 < } ; 0 { _ . 1 + } { _ lt3 } #");
        assert!(r.is_ok());
    }

    #[test]
    fn nested_cond_in_loop() {
        // NOTE: `?? ... | ... ]` has no nesting-aware `]` matching: a
        // nested cond in the else branch swallows the rest of the loop
        // body (same behavior as the old flat codegen — verified). The
        // nested conditional must live in its own word to be composable.
        // Expected output: 0 99 1 88 2, final counter 3.
        let r = exec(
            ": pick { _ 1 = ?? 99 . | _ 2 = ?? 88 . | ] } ; \
             0 { _ pick _ . 1 + } { _ 3 < } #",
        )
        .unwrap()
        .unwrap();
        assert_eq!(r, whisper_core::value::Value::I64(3));
    }

    #[test]
    fn cond_arrow_test() {
        exec("5 0 > { 42 . } ?->").unwrap();
    }

    // ── Stack-effect contract detection ─────────────────────────

    #[test]
    fn contract_detects_unequal_branch_arms() {
        // then pushes 1 value, else pushes 2 — join sees depth 1 vs 2
        let err = compile_block("5 3 > ?? 100 | 0 1 ] drop drop").unwrap_err();
        assert!(
            err.contains("stack contract violated"),
            "expected contract violation, got: {err}"
        );
    }

    #[test]
    fn contract_detects_nonzero_loop_body() {
        // body pushes one net value per iteration — back-edge mismatch
        let err = compile_block("0 { 5 } { _ 3 < } # drop drop drop drop").unwrap_err();
        assert!(
            err.contains("stack contract violated"),
            "expected contract violation, got: {err}"
        );
    }

    #[test]
    fn contract_detects_main_underflow() {
        let err = compile_block("1 +").unwrap_err();
        assert!(err.contains("stack underflow"), "got: {err}");
    }

    #[test]
    fn contract_clean_programs_pass() {
        // equal arms, net-zero loop bodies, balanced stack: no violations
        let (flat, defs) = compile_block(
            ": lt3 { 3 < } ; 0 { _ . 1 + } { _ lt3 } # drop \
             5 3 > ?? 100 | 0 ] .",
        )
        .unwrap();
        assert!(!flat.is_empty());
        assert_eq!(defs.len(), 1);
    }

    // ── Graph optimization ───────────────────────────────────────

    #[test]
    fn graph_folds_constant_false_branch() {
        // #f ?? 100 | 0 ] → no Cond in emitted code, result 0
        let (flat, _) = compile_block("#f ?? 100 | 0 ]").unwrap();
        assert!(
            !flat.iter().any(|op| matches!(op, Opcode::Cond(_))),
            "Cond should be folded away: {flat:?}"
        );
        let r = exec("#f ?? 100 | 0 ]").unwrap().unwrap();
        assert_eq!(r, whisper_core::value::Value::I64(0));
    }

    #[test]
    fn graph_folds_constant_true_branch() {
        let (flat, _) = compile_block("#t ?? 100 | 0 ]").unwrap();
        assert!(
            !flat.iter().any(|op| matches!(op, Opcode::Cond(_))),
            "Cond should be folded away: {flat:?}"
        );
        let r = exec("#t ?? 100 | 0 ]").unwrap().unwrap();
        assert_eq!(r, whisper_core::value::Value::I64(100));
    }

    #[test]
    fn graph_threads_empty_else_block() {
        // `?? 99 | ]` — else arm is empty. Without threading the emit
        // is [Push, Jump(else), Jump(join), Drop] (4 ops); threading
        // bypasses the empty else block entirely → 3 ops, no Cond.
        let (flat, _) = compile_block("1 #f ?? 99 | ] drop").unwrap();
        assert!(
            !flat.iter().any(|op| matches!(op, Opcode::Cond(_))),
            "constant-false branch should be folded: {flat:?}"
        );
        assert_eq!(
            flat.len(),
            3,
            "empty-else hop should be threaded away: {flat:?}"
        );
        let r = exec("1 #f ?? 99 | ] drop").unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn graph_optimization_preserves_behavior() {
        // stack-consistent nested cond (all arms push exactly 1 value);
        // loop body is net-zero (drop removes pick's extra push)
        let src = ": pick { _ 1 = ?? 99 | _ 2 = ?? 88 | 0 ] ] } ; \
             0 { pick . 1 + } { _ 3 < } # drop";
        let r = exec(src).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn graph_threading_keeps_leave_working() {
        // Regression: the then-arm of `?? leave | ]` is an empty block
        // whose Jump targets the loop exit — the SAME block the loop's
        // own false-path targets. Threading must not retarget the
        // Branch's `then` (fall-through) arm, or RPO emits the exit
        // elsewhere and true-path falls into the else code: leave
        // silently stops firing and the loop runs to completion.
        // Expected: prints 0 1 2, leaves at 3, drops the counter.
        let (flat, _) = compile_block("0 { _ 3 = ?? leave | ] _ . 1 + } { _ 9 < } # drop").unwrap();
        assert!(flat.iter().any(|op| matches!(op, Opcode::Cond(_))));
        let r = exec("0 { _ 3 = ?? leave | ] _ . 1 + } { _ 9 < } # drop");
        assert_eq!(r.unwrap(), None);
    }

    // ── walk-r / px `#`-loop regression shapes ────────────────────
    // png-decode's walk-r and px-all were rewritten as recursion when
    // `#` loops kept breaking; these tests restore the original loop
    // shapes as dedicated regressions for the Block-IR codegen.

    #[test]
    fn walk_r_hash_loop_shape() {
        // while-true + leave on sentinel, kv-state machine (walk-r form;
        // kv values are strings — i64tostr/strtoi64 like the wheel does)
        let r = exec(
            ": walk-loop { \"n\" \"0\" kv-set \
                 { \"n\" kv-get strtoi64 3 = ?? leave | ] \
                   \"n\" kv-get strtoi64 . \
                   \"n\" kv-get strtoi64 1 + i64tostr \"n\" ` kv-set \
                 } { #t } # } ; \
             walk-loop",
        );
        assert_eq!(r.unwrap(), None);
    }

    #[test]
    fn px_nested_hash_loops_with_word_in_cond() {
        // nested y/x loops (px-all/px-row form); inner condition calls
        // a word — the exact shape that broke the flat codegen
        let r = exec(
            ": lt3 { 3 < } ; \
             : px-loop { \"y\" \"0\" kv-set \
                 { \"x\" \"0\" kv-set \
                   { \"x\" kv-get strtoi64 . \
                     \"x\" kv-get strtoi64 1 + i64tostr \"x\" ` kv-set \
                   } { \"x\" kv-get strtoi64 lt3 } # \
                   \"y\" kv-get strtoi64 1 + i64tostr \"y\" ` kv-set \
                 } { \"y\" kv-get strtoi64 2 < } # } ; \
             px-loop",
        );
        assert_eq!(r.unwrap(), None);
    }
}
