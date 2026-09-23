//! Bytecode generator: AST → Vec<Opcode>.
//!
//! Two-pass compilation:
//!   Pass 1: Pre-scan all Def nodes, compile their bodies, build word_dict.
//!   Pass 2: Compile main program, resolving WordRef by inlining from word_dict.

use std::collections::HashMap;
use std::rc::Rc;
use whisper_core::opcode::Opcode;
use whisper_parser::ast::{AstNode, Operator};

/// Compiled bytecode: the main program plus every named word definition.
/// Named because the inline spelling trips clippy's `type_complexity`.
pub type CompiledBytecode = (Vec<Opcode>, HashMap<String, Vec<Opcode>>);

/// The bytecode generator.
pub struct BytecodeGenerator {
    /// Accumulated bytecode for main program
    bytecode: Vec<Opcode>,
    /// Dictionary of compiled word definitions (name → bytecode)
    word_dict: HashMap<String, Vec<Opcode>>,
    /// Pending word definitions to include in result
    pending_defs: Vec<(String, Vec<Opcode>)>,
}

impl BytecodeGenerator {
    pub fn new() -> Self {
        BytecodeGenerator {
            bytecode: Vec::new(),
            word_dict: HashMap::new(),
            pending_defs: Vec::new(),
        }
    }

    /// Compile a sequence of AST nodes into (main_bytecode, word_definitions).
    ///
    /// All control flow compiles through Block-IR (blocks + explicit
    /// edges, linearized once at the end). Numeric jump offsets are
    /// computed in exactly one place: `block_ir::linearize`.
    ///
    /// Stack-effect contracts are enforced: word bodies are checked
    /// relatively (branch arms equal, loop bodies net-zero, edges
    /// consistent) and the main program additionally rejects static
    /// underflow. Word net effects accumulate so `Call` sites of
    /// previously-defined words stay checkable.
    pub fn compile(
        &mut self,
        nodes: &[AstNode],
    ) -> Result<CompiledBytecode, Vec<crate::block_ir::ContractError>> {
        let unit = crate::block_gen::compile_unit(nodes);

        // Contract policy: the corpus (notably the self-hosted whisperc
        // sources) still contains branch arms with unequal net effects
        // that rely on the VM's lenient dynamics, so violations warn by
        // default. WHISPER_CONTRACTS_STRICT=1 flips them to compile
        // errors — the target state once the corpus is cleaned.
        if !unit.warnings.is_empty() {
            if std::env::var_os("WHISPER_CONTRACTS_STRICT").is_some() {
                return Err(unit.warnings);
            }
            for w in &unit.warnings {
                eprintln!("Contract warning: {w}");
            }
        }

        let main = crate::block_ir::linearize(&unit.main);
        let mut defs = HashMap::new();
        for (name, program) in &unit.defs {
            let code = crate::block_ir::linearize(program);
            self.word_dict.insert(name.clone(), code.clone());
            self.pending_defs.push((name.clone(), code));
        }
        for (name, code) in std::mem::take(&mut self.pending_defs) {
            defs.insert(name, code);
        }
        Ok((main, defs))
    }

    /// Compile to the Block-IR unit (main + defs as block programs).
    /// The natural input for .wbin v2 serialization.
    pub fn compile_to_unit(
        &mut self,
        nodes: &[AstNode],
    ) -> Result<crate::block_gen::CompileUnit, Vec<crate::block_ir::ContractError>> {
        let unit = crate::block_gen::compile_unit(nodes);
        if !unit.warnings.is_empty() {
            if std::env::var_os("WHISPER_CONTRACTS_STRICT").is_some() {
                return Err(unit.warnings);
            }
            for w in &unit.warnings {
                eprintln!("Contract warning: {w}");
            }
        }
        Ok(unit)
    }

    /// Compile and return only the main bytecode (convenience method).
    pub fn compile_main(
        &mut self,
        nodes: &[AstNode],
    ) -> Result<Vec<Opcode>, Vec<crate::block_ir::ContractError>> {
        Ok(self.compile(nodes)?.0)
    }

    pub fn take_bytecode(&mut self) -> Vec<Opcode> {
        std::mem::take(&mut self.bytecode)
    }

    pub fn compile_node_public(&mut self, node: &AstNode) {
        self.compile_node(node)
    }

    fn compile_node(&mut self, node: &AstNode) {
        // Only leaf nodes reach this generator — everything structural
        // (control flow, quotes, lists, labels) is handled at the
        // Block-IR level in block_gen.
        match node {
            AstNode::Literal(val) => self.compile_literal(val),
            AstNode::WordRef(name) => self.compile_word_ref(name),
            AstNode::Op(op) => self.compile_operator(*op),
            AstNode::Def { .. } | AstNode::Import(_) | AstNode::Export(_) => {
                // Compile-time directives — no runtime effect
            }
            AstNode::Cond { .. }
            | AstNode::Loop { .. }
            | AstNode::CondArrow { .. }
            | AstNode::Quote(_)
            | AstNode::List(_)
            | AstNode::Times { .. }
            | AstNode::ProbChoice { .. }
            | AstNode::ConfidenceLabel { .. } => {
                unreachable!("structural nodes are handled at the Block-IR level")
            }
        }
    }

    fn compile_literal(&mut self, val: &whisper_core::value::Value) {
        match val {
            whisper_core::value::Value::I64(n) => self.emit(Opcode::PushI64(*n)),
            whisper_core::value::Value::F64(n) => self.emit(Opcode::PushF64(*n)),
            whisper_core::value::Value::Bool(b) => self.emit(Opcode::PushBool(*b)),
            whisper_core::value::Value::Str(s) => self.emit(Opcode::PushStr(Rc::clone(s))),
            whisper_core::value::Value::List(items) => {
                // Push elements first, then count, then PushList
                // Count goes LAST so PushList pops it first (LIFO)
                for item in items.iter() {
                    self.compile_literal(item);
                }
                self.emit(Opcode::PushI64(items.len() as i64));
                self.emit(Opcode::PushList);
            }
            whisper_core::value::Value::Ref(code) => {
                self.emit(Opcode::PushRef(code.iter().cloned().collect()));
            }
            _ => {}
        }
    }

    fn compile_word_ref(&mut self, name: &str) {
        // Always use Call — VM looks up word_dict at runtime
        // This supports recursion and mutual recursion correctly
        self.emit(Opcode::Call(name.to_string()));
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
            Operator::Prepend => Opcode::Prepend,
            Operator::Len => Opcode::Len,
            Operator::Map => Opcode::Map,
            Operator::Each => Opcode::Each,
            Operator::Fold => Opcode::Fold,
            Operator::Filter => Opcode::Filter,
            Operator::StrSplit => Opcode::StrSplit,
            Operator::Ctos => Opcode::Ctos,
            Operator::ListSlice => Opcode::ListSlice,
            Operator::Shl => Opcode::Shl,
            Operator::Shr => Opcode::Shr,
            Operator::Args => Opcode::Args,
            Operator::SplitCond => Opcode::SplitCond,
            Operator::ListCheck => Opcode::ListCheck,
            Operator::I64Check => Opcode::I64Check,
            Operator::StrCheck => Opcode::StrCheck,
            Operator::BoolCheck => Opcode::BoolCheck,
            Operator::Return => Opcode::Return,
            Operator::Listen => Opcode::Listen,
            Operator::Accept => Opcode::Accept,
            Operator::Recv => Opcode::Recv,
            Operator::Send => Opcode::Send,
            Operator::CloseSock => Opcode::CloseSock,
            Operator::ReadFile => Opcode::ReadFile,
            Operator::WriteFile => Opcode::WriteFile,
            Operator::EnvGet => Opcode::EnvGet,
            Operator::OpEffect => Opcode::OpEffect,
            Operator::QuoteSize => Opcode::QuoteSize,
            Operator::WordSize => Opcode::WordSize,
            Operator::SetSeed => Opcode::SetSeed,
            Operator::Rand => Opcode::Rand,
            Operator::ConfOf => Opcode::ConfOf,
            Operator::UnwrapSig => Opcode::UnwrapSig,
            Operator::KvSet => Opcode::KvSet,
            Operator::KvGet => Opcode::KvGet,
            Operator::KvDel => Opcode::KvDel,
            Operator::KvKeys => Opcode::KvKeys,
            Operator::ServeConcurrent => Opcode::ServeConcurrent,
            Operator::ReadLine => Opcode::ReadLine,
            Operator::StrUpper => Opcode::StrUpper,
            Operator::StrLower => Opcode::StrLower,
            Operator::StrStrip => Opcode::StrStrip,
            Operator::Fmt => Opcode::Fmt,
            Operator::ListDir => Opcode::ListDir,
            Operator::Mkdir => Opcode::Mkdir,
            Operator::FileExists => Opcode::FileExists,
            Operator::StrfTime => Opcode::StrfTime,
            Operator::Leave => Opcode::Leave(0),
            Operator::StrMatch => Opcode::StrMatch,
            Operator::StrFindRe => Opcode::StrFindRe,
            Operator::Pack => Opcode::Pack,
            Operator::Unpack => Opcode::Unpack,
            Operator::Range => Opcode::Range,
            Operator::FfiOpen => Opcode::FfiOpen,
            Operator::FfiSym => Opcode::FfiSym,
            Operator::FfiCall => Opcode::FfiCall,
            Operator::BufAlloc => Opcode::BufAlloc,
            Operator::BufFree => Opcode::BufFree,
            Operator::BufPeek => Opcode::BufPeek,
            Operator::BufPoke => Opcode::BufPoke,
            Operator::BufToStr => Opcode::BufToStr,
            Operator::StrLen => Opcode::StrLen,
            Operator::StrCat => Opcode::StrCat,
            Operator::StrSlice => Opcode::StrSlice,
            Operator::StrEq => Opcode::StrEq,
            Operator::StrLt => Opcode::StrLt,
            Operator::StrFind => Opcode::StrFind,
            Operator::StrReplace => Opcode::StrReplace,
            Operator::StrToI64 => Opcode::StrToI64,
            Operator::I64ToStr => Opcode::I64ToStr,
            Operator::StrNth => Opcode::StrNth,
            Operator::StrChars => Opcode::StrChars,
            Operator::CharsStr => Opcode::CharsStr,
            Operator::StrIter => Opcode::StrIter,
            Operator::ListFind => Opcode::ListFind,
            Operator::StrJoin => Opcode::StrJoin,
            Operator::BytesNew => Opcode::BytesNew,
            Operator::BytesPush => Opcode::BytesPush,
            Operator::BytesLen => Opcode::BytesLen,
            Operator::BytesWriteFile => Opcode::BytesWriteFile,
            Operator::BytesToHex => Opcode::BytesToHex,
            Operator::Try => Opcode::Try,
            Operator::MapNew => Opcode::MapNew,
            Operator::MapGet => Opcode::MapGet,
            Operator::MapSet => Opcode::MapSet,
            Operator::MapDel => Opcode::MapDel,
            Operator::MapKeys => Opcode::MapKeys,
            Operator::MapHas => Opcode::MapHas,
            Operator::I64ToF64 => Opcode::I64ToF64,
            Operator::F64ToI64 => Opcode::F64ToI64,
            Operator::FSqrt => Opcode::FSqrt,
            Operator::FSin => Opcode::FSin,
            Operator::FCos => Opcode::FCos,
            Operator::FTan => Opcode::FTan,
            Operator::JsonParse => Opcode::JsonParse,
            Operator::JsonStringify => Opcode::JsonStringify,
            Operator::Now => Opcode::Now,
            Operator::Sleep => Opcode::Sleep,
            Operator::Random => Opcode::Random,
            Operator::RandInt => Opcode::RandInt,
            Operator::Floor => Opcode::Floor,
            Operator::Ceil => Opcode::Ceil,
            Operator::Round => Opcode::Round,
            Operator::Log => Opcode::Log,
            Operator::Exp => Opcode::Exp,
            Operator::Atan2 => Opcode::Atan2,
            Operator::BitAnd => Opcode::BitAnd,
            Operator::BitOr => Opcode::BitOr,
            Operator::BitXor => Opcode::BitXor,
            Operator::BitNot => Opcode::BitNot,
            Operator::Sort => Opcode::Sort,
            Operator::Rev => Opcode::Rev,
            Operator::Dedup => Opcode::Dedup,
            Operator::Take => Opcode::Take,
            Operator::DropN => Opcode::DropN,
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
            Operator::Print => Opcode::Print,
            Operator::Exit => Opcode::Exit,
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
