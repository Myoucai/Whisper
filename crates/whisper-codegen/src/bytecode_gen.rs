/// Bytecode generator: AST → Vec<Opcode>.
///
/// Two-pass compilation:
///   Pass 1: Pre-scan all Def nodes, compile their bodies, build word_dict.
///   Pass 2: Compile main program, resolving WordRef by inlining from word_dict.

use std::collections::HashMap;
use whisper_core::opcode::Opcode;
use whisper_parser::ast::{AstNode, Operator};

/// The bytecode generator.
pub struct BytecodeGenerator {
    /// Accumulated bytecode for main program
    bytecode: Vec<Opcode>,
    /// Dictionary of compiled word definitions (name → bytecode)
    word_dict: HashMap<String, Vec<Opcode>>,
    /// Counter for generating unique word indices
    word_counter: u32,
    /// Pending word definitions to include in result
    pending_defs: Vec<(String, Vec<Opcode>)>,
}

impl BytecodeGenerator {
    pub fn new() -> Self {
        BytecodeGenerator {
            bytecode: Vec::new(),
            word_dict: HashMap::new(),
            word_counter: 0,
            pending_defs: Vec::new(),
        }
    }

    /// Compile a sequence of AST nodes into (main_bytecode, word_definitions).
    pub fn compile(&mut self, nodes: &[AstNode]) -> (Vec<Opcode>, HashMap<String, Vec<Opcode>>) {
        // Pass 1: Register all word definitions (process in order so
        // later defs can reference earlier ones)
        for node in nodes {
            if let AstNode::Def { name, body } = node {
                // Use parent's word_dict so references to previous defs resolve
                let mut sub_gen = BytecodeGenerator::new();
                sub_gen.word_dict = self.word_dict.clone();
                let (body_code, _) = sub_gen.compile(body);
                let mut def_code = body_code;
                def_code.push(Opcode::Return);
                self.word_dict.insert(name.clone(), def_code.clone());
                self.pending_defs.push((name.clone(), def_code));
            }
        }

        // Pass 2: Compile main program
        for node in nodes {
            if matches!(node, AstNode::Def { .. }) {
                continue; // Skip defs — they're already processed
            }
            self.compile_node(node);
        }

        let main = std::mem::take(&mut self.bytecode);
        let mut defs = HashMap::new();
        for (name, code) in std::mem::take(&mut self.pending_defs) {
            defs.insert(name, code);
        }
        (main, defs)
    }

    /// Compile and return only the main bytecode (convenience method).
    pub fn compile_main(&mut self, nodes: &[AstNode]) -> Vec<Opcode> {
        let (main, _) = self.compile(nodes);
        main
    }

    fn compile_node(&mut self, node: &AstNode) {
        match node {
            AstNode::Literal(val) => self.compile_literal(val),
            AstNode::WordRef(name) => self.compile_word_ref(name),
            AstNode::Op(op) => self.compile_operator(*op),
            AstNode::Quote(body) => self.compile_quote(body),
            AstNode::List(items) => {
                // Pre-construct list at compile time
                let mut values = Vec::new();
                for item in items {
                    // Compile to a temporary to extract literal values
                    values.push(ast_node_to_value(item));
                }
                let list_val = whisper_core::value::Value::List(
                    std::rc::Rc::new(values),
                );
                self.compile_literal(&list_val);
            }
            AstNode::Cond { then_branch, else_branch } => {
                self.compile_cond(then_branch, else_branch.as_deref());
            }
            AstNode::Loop { body, condition } => {
                self.compile_loop(body, condition);
            }
            AstNode::Times { body } => {
                self.compile_node(&AstNode::Quote(body.clone()));
                // n {body} @times — the n and quote should already be on stack
                // In practice, @times is handled as an operator
                self.emit(Opcode::Times);
            }
            AstNode::Def { .. } => {
                // Already processed in Pass 1 — skip
            }
            AstNode::Import(_) | AstNode::Export(_) => {
                // Compile-time directives — no runtime effect
            }
            AstNode::ConfidenceLabel { body, confidence } => {
                for n in body { self.compile_node(n); }
                self.emit(Opcode::ConfLabel(confidence.0));
            }
            AstNode::ProbChoice { alt1, alt2 } => {
                self.compile_node(&AstNode::Quote(alt2.clone()));
                self.compile_node(&AstNode::Quote(alt1.clone()));
                self.emit(Opcode::ProbChoice);
            }
            AstNode::CondArrow { then_branch } => {
                // cond {then} ?-> : pop cond, if true execute then
                self.emit(Opcode::Cond(
                    then_branch.len() as i32 + 2
                ));
                for n in then_branch { self.compile_node(n); }
            }
        }
    }

    fn compile_literal(&mut self, val: &whisper_core::value::Value) {
        match val {
            whisper_core::value::Value::I64(n) => self.emit(Opcode::PushI64(*n)),
            whisper_core::value::Value::F64(n) => self.emit(Opcode::PushF64(*n)),
            whisper_core::value::Value::Bool(b) => self.emit(Opcode::PushBool(*b)),
            whisper_core::value::Value::Str(s) => self.emit(Opcode::PushStr(s.as_ref().clone())),
            whisper_core::value::Value::List(items) => {
                // Push count first, then elements, then PushList op
                self.emit(Opcode::PushI64(items.len() as i64));
                for item in items.iter() {
                    self.compile_literal(item);
                }
                self.emit(Opcode::PushList);
            }
            _ => {}
        }
    }

    fn compile_word_ref(&mut self, name: &str) {
        if let Some(def_code) = self.word_dict.get(name).cloned() {
            // Inline the word's bytecode
            for op in &def_code[..def_code.len() - 1] {
                // Skip the Return at end
                self.bytecode.push(op.clone());
            }
        } else {
            // Unknown word — emit Call with index (fallback)
            let idx = self.word_counter;
            self.word_counter += 1;
            self.emit(Opcode::Call(idx));
        }
    }

    fn compile_quote(&mut self, body: &[AstNode]) {
        // Pre-construct the quotation as a Ref value at compile time
        let ref_val = ast_node_to_value(&AstNode::Quote(body.to_vec()));
        self.compile_literal(&ref_val);
    }

    fn compile_cond(&mut self, then_branch: &[AstNode], else_branch: Option<&[AstNode]>) {
        // condition should already be on stack
        let mut then_gen = BytecodeGenerator::new();
        let (then_code, _) = then_gen.compile(then_branch);

        let else_code = else_branch.map(|eb| {
            let mut else_gen = BytecodeGenerator::new();
            let (code, _) = else_gen.compile(eb);
            code
        });

        let then_len = then_code.len() as i32;

        if let Some(ref else_c) = else_code {
            let else_len = else_c.len() as i32;
            // If false, jump past then_branch to else
            self.emit(Opcode::Cond(then_len + 2)); // +2 for Jump after then
            self.bytecode.extend(then_code);
            self.emit(Opcode::Jump(else_len + 1));
            self.bytecode.extend(else_c.clone());
        } else {
            self.emit(Opcode::Cond(then_len as i32));
            self.bytecode.extend(then_code);
        }
    }

    fn compile_loop(&mut self, body: &[AstNode], condition: &[AstNode]) {
        let loop_start = self.bytecode.len();

        // Compile body
        let mut body_gen = BytecodeGenerator::new();
        let (body_code, _) = body_gen.compile(body);
        self.bytecode.extend(body_code);

        // Compile condition (pushes bool onto stack)
        let mut cond_gen = BytecodeGenerator::new();
        let (cond_code, _) = cond_gen.compile(condition);
        let cond_end = self.bytecode.len() + cond_code.len();
        self.bytecode.extend(cond_code);

        // Loop: if true, jump back to loop_start
        let offset = loop_start as i32 - cond_end as i32 - 1;
        self.emit(Opcode::Loop(offset));
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

/// Extract a literal Value from an AST node (compile-time evaluation).
fn ast_node_to_value(node: &AstNode) -> whisper_core::value::Value {
    match node {
        AstNode::Literal(v) => v.clone(),
        AstNode::List(items) => {
            let values: Vec<_> = items.iter().map(ast_node_to_value).collect();
            whisper_core::value::Value::List(std::rc::Rc::new(values))
        }
        AstNode::Quote(body) => {
            // Compile the body and store as a Ref
            let mut gen = BytecodeGenerator::new();
            let (code, _) = gen.compile(body);
            whisper_core::value::Value::Ref(std::rc::Rc::from(code.into_boxed_slice()))
        }
        AstNode::Op(Operator::Sub) => {
            // For negative numbers, the literal is handled by the lexer
            whisper_core::value::Value::I64(0)
        }
        _ => whisper_core::value::Value::I64(0), // fallback
    }
}

impl Default for BytecodeGenerator {
    fn default() -> Self {
        Self::new()
    }
}
