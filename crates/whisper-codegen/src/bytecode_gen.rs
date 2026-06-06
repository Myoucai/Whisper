/// Bytecode generator: AST → Vec<Opcode>.
///
/// Compiles Whisper AST nodes into a linear bytecode sequence
/// that can be executed by the whisper-core VM.

use whisper_core::opcode::Opcode;
use whisper_parser::ast::{AstNode, Operator};

/// The bytecode generator.
pub struct BytecodeGenerator {
    /// Accumulated bytecode
    bytecode: Vec<Opcode>,
    /// Counter for generating unique word indices
    word_counter: u32,
}

impl BytecodeGenerator {
    pub fn new() -> Self {
        BytecodeGenerator {
            bytecode: Vec::new(),
            word_counter: 0,
        }
    }

    /// Compile a sequence of AST nodes into bytecode.
    pub fn compile(&mut self, nodes: &[AstNode]) -> Vec<Opcode> {
        for node in nodes {
            self.compile_node(node);
        }
        std::mem::take(&mut self.bytecode)
    }

    fn compile_node(&mut self, node: &AstNode) {
        match node {
            AstNode::Literal(val) => {
                match val {
                    whisper_core::value::Value::I64(n) => {
                        self.emit(Opcode::PushI64(*n));
                    }
                    whisper_core::value::Value::F64(n) => {
                        self.emit(Opcode::PushF64(*n));
                    }
                    whisper_core::value::Value::Bool(b) => {
                        self.emit(Opcode::PushBool(*b));
                    }
                    whisper_core::value::Value::Str(s) => {
                        self.emit(Opcode::PushStr(s.as_ref().clone()));
                    }
                    whisper_core::value::Value::List(items) => {
                        for item in items.iter() {
                            self.compile_node(&AstNode::Literal(item.clone()));
                        }
                        self.emit(Opcode::PushList);
                    }
                    _ => {}
                }
            }

            AstNode::WordRef(_name) => {
                // For now, emit a Call; word resolution happens at link time
                // In a full implementation, this would be resolved to a direct index
                let idx = self.word_counter;
                self.word_counter += 1;
                self.emit(Opcode::Call(idx));
            }

            AstNode::Op(op) => {
                self.compile_operator(*op);
            }

            AstNode::Quote(body) => {
                // Compile the body as inline bytecode
                let mut sub_gen = BytecodeGenerator::new();
                let inner_code = sub_gen.compile(body);
                let _code_len = inner_code.len() as u32;
                // PushRef expects the bytecode length prefix
                self.emit(Opcode::PushRef);
                // In actual .wbin, the ref body follows PushRef with a length prefix
                self.bytecode.extend(inner_code);
            }

            AstNode::List(items) => {
                for item in items {
                    self.compile_node(item);
                }
                self.emit(Opcode::PushList);
            }

            AstNode::Cond {
                then_branch,
                else_branch,
            } => {
                // Compile: condition should already be on stack
                // Layout: <cond_code> Cond(jump_over_then) <then_code> Jump(jump_over_else) <else_code>
                let mut then_gen = BytecodeGenerator::new();
                let then_code = then_gen.compile(then_branch);

                let else_code = else_branch.as_ref().map(|eb| {
                    let mut else_gen = BytecodeGenerator::new();
                    else_gen.compile(eb)
                });

                let then_len = then_code.len() as i32;
                let else_len = else_code.as_ref().map_or(0, |c| c.len()) as i32;

                if let Some(ref ec) = else_code {
                    // Cond jumps to else if false; then jumps over else at end
                    self.emit(Opcode::Cond(then_len + 1)); // skip then + 1 for Jump
                    self.bytecode.extend(then_code);
                    self.emit(Opcode::Jump(else_len + 1)); // skip else + 1
                    self.bytecode.extend(ec.clone());
                } else {
                    self.emit(Opcode::Cond(then_len + 1)); // +1 for the Cond itself? No, offset from next ip
                    // Actually: if false, skip then_code
                    self.emit(Opcode::Cond(then_len as i32));
                    self.bytecode.extend(then_code);
                }
            }

            AstNode::CondArrow { then_branch: _ } => {
                // cond {then} ?-> : pop cond, if true execute then block
                self.emit(Opcode::Cond(1)); // placeholder
            }

            AstNode::Loop { body, condition } => {
                let loop_start = self.bytecode.len();
                let mut body_gen = BytecodeGenerator::new();
                let body_code = body_gen.compile(body);
                self.bytecode.extend(body_code);

                let mut cond_gen = BytecodeGenerator::new();
                let cond_code = cond_gen.compile(condition);
                self.bytecode.extend(cond_code);

                let offset = loop_start as i32 - self.bytecode.len() as i32;
                self.emit(Opcode::Loop(offset));
            }

            AstNode::Times { body } => {
                let mut body_gen = BytecodeGenerator::new();
                let body_code = body_gen.compile(body);
                self.bytecode.extend(body_code);
                self.emit(Opcode::Times);
            }

            AstNode::Def { name: _, body: _ } => {
                // Word definitions are handled at the top level
                // For now, skip — the compiler pipeline resolves them
            }

            AstNode::Import(_path) => {
                // Module import — resolved at load time
            }

            AstNode::Export(_name) => {
                // Export — resolved at link time
            }

            AstNode::ConfidenceLabel { body, confidence } => {
                let mut sub_gen = BytecodeGenerator::new();
                let inner = sub_gen.compile(body);
                self.bytecode.extend(inner);
                self.emit(Opcode::ConfLabel(confidence.0));
            }

            AstNode::ProbChoice { alt1, alt2 } => {
                let mut gen1 = BytecodeGenerator::new();
                let code1 = gen1.compile(alt1);
                let mut gen2 = BytecodeGenerator::new();
                let code2 = gen2.compile(alt2);
                // Push alt2 first (will be used as second alternative)
                self.bytecode.extend(code2);
                self.bytecode.extend(code1);
                self.emit(Opcode::ProbChoice);
            }
        }
    }

    fn compile_operator(&mut self, op: Operator) {
        let opcode = match op {
            Operator::Dup => Opcode::Dup,
            Operator::Swap => Opcode::Swap,
            Operator::Drop => Opcode::Drop,
            Operator::Rot => Opcode::Rot,
            Operator::Pick(n) => Opcode::Pick(n),
            Operator::Add => Opcode::Add,
            Operator::Sub => Opcode::Sub,
            Operator::Mul => Opcode::Mul,
            Operator::Div => Opcode::Div,
            Operator::Mod => Opcode::Mod,
            Operator::Eq => Opcode::Eq,
            Operator::Lt => Opcode::Lt,
            Operator::Gt => Opcode::Gt,
            Operator::Neq => Opcode::Neq,
            Operator::Le => Opcode::Le,
            Operator::Ge => Opcode::Ge,
            Operator::And => Opcode::And,
            Operator::Or => Opcode::Or,
            Operator::Not => Opcode::Not,
            Operator::Nth => Opcode::Nth,
            Operator::Append => Opcode::Append,
            Operator::Len => Opcode::Len,
            Operator::Map => Opcode::Map,
            Operator::Each => Opcode::Each,
            Operator::Fold => Opcode::Fold,
            Operator::CondQ => Opcode::Cond(0), // placeholder
            Operator::CondArrow => Opcode::Cond(0),
            Operator::Hash => Opcode::Loop(0),
            Operator::AtTimes => Opcode::Times,
            Operator::CapCall(n) => Opcode::CapCall(n),
            Operator::CapExec => Opcode::CapExec,
            Operator::ConfLabel(c) => Opcode::ConfLabel(c),
            Operator::ProbChoice => Opcode::ProbChoice,
            Operator::OutputTop => Opcode::OutputTop,
            Operator::OutputAll => Opcode::OutputAll,
            Operator::ReadInput => Opcode::ReadInput,
        };
        self.emit(opcode);
    }

    fn emit(&mut self, op: Opcode) {
        self.bytecode.push(op);
    }
}

impl Default for BytecodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}
