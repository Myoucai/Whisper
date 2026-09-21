//! .wbin binary format reader/writer.
//!
//! Format specification (per design doc section 5.2):
//!   Header: 4 bytes magic "WHSP" + 4 bytes version (u32 LE)
//!   Body:   LEB128-encoded opcodes

use std::io::{Cursor, Read};
use std::rc::Rc;
use whisper_core::opcode::Opcode;

/// Magic bytes for .wbin files.
pub const WBIN_MAGIC: &[u8; 4] = b"WHSP";
/// Original flat format: one opcode stream (main only), numeric jump
/// offsets embedded. Still readable for compatibility.
pub const WBIN_VERSION_V1: u32 = 1;
/// Block format since the Block-IR rework: main + defs as block
/// programs with explicit edges — no offsets on disk; the loader
/// linearizes (`block_ir::linearize`).
pub const WBIN_VERSION_V2: u32 = 2;

/// Writer for .wbin binary format.
pub struct WbinWriter;

impl WbinWriter {
    /// Serialize a sequence of opcodes to .wbin format bytes.
    pub fn write(opcodes: &[Opcode]) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(WBIN_MAGIC);
        buf.extend_from_slice(&WBIN_VERSION_V1.to_le_bytes());

        // Body — each opcode as a single byte + optional data
        for op in opcodes {
            Self::encode_opcode(op, &mut buf);
        }

        buf
    }

    /// Write opcodes to a file.
    pub fn write_to_file(opcodes: &[Opcode], path: &std::path::Path) -> std::io::Result<()> {
        let data = Self::write(opcodes);
        std::fs::write(path, data)
    }

    fn encode_opcode(op: &Opcode, buf: &mut Vec<u8>) {
        let byte = op.to_byte();
        buf.push(byte);

        match op {
            Opcode::Pick(n) => buf.push(*n),
            Opcode::PushI64(n) => {
                leb128::write::unsigned(buf, *n as u64).ok();
            }
            Opcode::PushF64(n) => {
                buf.extend_from_slice(&n.to_le_bytes());
            }
            Opcode::PushStr(s) => {
                let bytes = s.as_bytes();
                leb128::write::unsigned(buf, bytes.len() as u64).ok();
                buf.extend_from_slice(bytes);
            }
            Opcode::PushBool(b) => {
                buf.push(if *b { 1 } else { 0 });
            }
            Opcode::Cond(offset) | Opcode::Jump(offset) | Opcode::Loop(offset) => {
                leb128::write::signed(buf, *offset as i64).ok();
            }
            Opcode::Call(name) => {
                let name_bytes = name.as_bytes();
                leb128::write::unsigned(buf, name_bytes.len() as u64).ok();
                buf.extend_from_slice(name_bytes);
            }
            Opcode::PushRef(inner) => {
                leb128::write::unsigned(buf, inner.len() as u64).ok();
                for inner_op in inner {
                    Self::encode_opcode(inner_op, buf);
                }
            }
            Opcode::CapCall(id) => {
                buf.extend_from_slice(&id.to_le_bytes());
            }
            Opcode::ConfLabel(conf) => {
                buf.extend_from_slice(&conf.to_le_bytes());
            }
            _ => {} // No additional data
        }
    }
}

/// Reader for .wbin binary format.
pub struct WbinReader;

/// A decoded .wbin file.
pub enum WbinUnit {
    /// v1: one flat opcode stream (main only; defs were not stored).
    V1(Vec<Opcode>),
    /// v2: main + defs as block programs; linearize at load time.
    V2 {
        main: crate::block_ir::BlockProgram,
        defs: Vec<(String, crate::block_ir::BlockProgram)>,
    },
}

impl WbinUnit {
    /// Linearizes to (main_bytecode, defs) ready for the VM.
    pub fn linearized(&self) -> (Vec<Opcode>, Vec<(String, Vec<Opcode>)>) {
        match self {
            WbinUnit::V1(flat) => (flat.clone(), Vec::new()),
            WbinUnit::V2 { main, defs } => {
                let main_flat = crate::block_ir::linearize(main);
                let defs_flat = defs
                    .iter()
                    .map(|(n, p)| (n.clone(), crate::block_ir::linearize(p)))
                    .collect();
                (main_flat, defs_flat)
            }
        }
    }
}

impl WbinWriter {
    /// Serialize a v2 unit: main + defs as block programs.
    /// Jump offsets never appear on disk — the loader linearizes.
    pub fn write_unit(
        main: &crate::block_ir::BlockProgram,
        defs: &[(String, crate::block_ir::BlockProgram)],
    ) -> Vec<u8> {
        use crate::block_ir::Terminator;
        let mut buf = Vec::new();
        buf.extend_from_slice(WBIN_MAGIC);
        buf.extend_from_slice(&WBIN_VERSION_V2.to_le_bytes());

        let count = 1 + defs.len();
        leb128::write::unsigned(&mut buf, count as u64).ok();

        let write_program =
            |name: &str, prog: &crate::block_ir::BlockProgram, buf: &mut Vec<u8>| {
                let name_bytes = name.as_bytes();
                leb128::write::unsigned(buf, name_bytes.len() as u64).ok();
                buf.extend_from_slice(name_bytes);
                leb128::write::unsigned(buf, prog.entry as u64).ok();
                leb128::write::unsigned(buf, prog.blocks.len() as u64).ok();
                for block in &prog.blocks {
                    leb128::write::unsigned(buf, block.instructions.len() as u64).ok();
                    for op in &block.instructions {
                        Self::encode_opcode(op, buf);
                    }
                    match &block.terminator {
                        Terminator::Halt => buf.push(0),
                        Terminator::Return => buf.push(1),
                        Terminator::Jump(t) => {
                            buf.push(2);
                            leb128::write::unsigned(buf, *t as u64).ok();
                        }
                        Terminator::Branch { then, els } => {
                            buf.push(3);
                            leb128::write::unsigned(buf, *then as u64).ok();
                            leb128::write::unsigned(buf, *els as u64).ok();
                        }
                    }
                }
            };

        write_program("", main, &mut buf);
        for (name, prog) in defs {
            write_program(name, prog, &mut buf);
        }
        buf
    }

    /// Write a v2 unit to a file.
    pub fn write_unit_to_file(
        main: &crate::block_ir::BlockProgram,
        defs: &[(String, crate::block_ir::BlockProgram)],
        path: &std::path::Path,
    ) -> std::io::Result<()> {
        std::fs::write(path, Self::write_unit(main, defs))
    }
}

impl WbinReader {
    /// Read and decode opcodes from a .wbin file.
    pub fn read_from_file(path: &std::path::Path) -> std::io::Result<Vec<Opcode>> {
        let data = std::fs::read(path)?;
        Self::decode(&data)
    }

    /// Decode .wbin bytes into opcodes (v1 flat format).
    pub fn decode(data: &[u8]) -> std::io::Result<Vec<Opcode>> {
        match Self::decode_unit(data)? {
            WbinUnit::V1(flat) => Ok(flat),
            WbinUnit::V2 { .. } => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "this file is .wbin v2 (block format) — use decode_unit",
            )),
        }
    }

    /// Decode any .wbin version: v1 flat stream, or v2 block unit.
    pub fn decode_unit(data: &[u8]) -> std::io::Result<WbinUnit> {
        if data.len() < 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "File too short for .wbin header",
            ));
        }
        if &data[0..4] != WBIN_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid magic bytes — not a .wbin file",
            ));
        }
        let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
        let mut cursor = Cursor::new(&data[8..]);
        match version {
            WBIN_VERSION_V1 => {
                let mut opcodes = Vec::new();
                loop {
                    let mut byte_buf = [0u8; 1];
                    match cursor.read_exact(&mut byte_buf) {
                        Ok(()) => {
                            let op = Self::decode_opcode(byte_buf[0], &mut cursor)?;
                            opcodes.push(op);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                        Err(e) => return Err(e),
                    }
                }
                Ok(WbinUnit::V1(opcodes))
            }
            WBIN_VERSION_V2 => {
                let count = read_leb(&mut cursor)?;
                let mut programs: Vec<(String, crate::block_ir::BlockProgram)> =
                    Vec::with_capacity(count.min(4096) as usize);
                for _ in 0..count {
                    programs.push(Self::decode_block_program(&mut cursor)?);
                }
                let mut iter = programs.into_iter();
                let Some((_, main)) = iter.next() else {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "v2 unit has no main program",
                    ));
                };
                Ok(WbinUnit::V2 {
                    main,
                    defs: iter.collect(),
                })
            }
            other => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported .wbin version: {other}"),
            )),
        }
    }

    fn decode_block_program(
        cursor: &mut Cursor<&[u8]>,
    ) -> std::io::Result<(String, crate::block_ir::BlockProgram)> {
        use crate::block_ir::{Block, BlockProgram, Terminator};
        let name_len = read_leb(cursor)? as usize;
        let mut name_buf = vec![0u8; name_len];
        cursor.read_exact(&mut name_buf)?;
        let name = String::from_utf8(name_buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let entry = read_leb(cursor)? as usize;
        let block_count = read_leb(cursor)? as usize;
        let mut blocks = Vec::with_capacity(block_count.min(1 << 20));
        for id in 0..block_count {
            let instr_count = read_leb(cursor)? as usize;
            let mut instructions = Vec::with_capacity(instr_count.min(1 << 20));
            for _ in 0..instr_count {
                let mut byte_buf = [0u8; 1];
                cursor.read_exact(&mut byte_buf)?;
                instructions.push(Self::decode_opcode(byte_buf[0], cursor)?);
            }
            let mut term_buf = [0u8; 1];
            cursor.read_exact(&mut term_buf)?;
            let terminator = match term_buf[0] {
                0 => Terminator::Halt,
                1 => Terminator::Return,
                2 => Terminator::Jump(read_leb(cursor)? as usize),
                3 => {
                    let then = read_leb(cursor)? as usize;
                    let els = read_leb(cursor)? as usize;
                    Terminator::Branch { then, els }
                }
                other => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Unknown terminator kind: {other}"),
                    ))
                }
            };
            blocks.push(Block {
                id,
                instructions,
                terminator,
                stack_in: 0,
                stack_out: 0,
            });
        }
        let program = BlockProgram { blocks, entry };
        Ok((name, program))
    }

    fn decode_opcode(byte: u8, cursor: &mut Cursor<&[u8]>) -> std::io::Result<Opcode> {
        match byte {
            // Stack ops
            0x00 => Ok(Opcode::Dup),
            0x01 => Ok(Opcode::Swap),
            0x02 => Ok(Opcode::Drop),
            0x03 => Ok(Opcode::Rot),
            0x04 => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf)?;
                Ok(Opcode::Pick(buf[0]))
            }
            0x05 => Ok(Opcode::Filter),
            0x06 => Ok(Opcode::StrSplit),
            0x07 => Ok(Opcode::Ctos),
            0x08 => Ok(Opcode::ListSlice),
            0x09 => Ok(Opcode::Shl),
            0x0A => Ok(Opcode::Shr),
            0x0B => Ok(Opcode::Args),

            // Arithmetic
            0x10 => Ok(Opcode::Add),
            0x11 => Ok(Opcode::Sub),
            0x12 => Ok(Opcode::Mul),
            0x13 => Ok(Opcode::Div),
            0x14 => Ok(Opcode::Mod),

            // Comparison
            0x18 => Ok(Opcode::Eq),
            0x19 => Ok(Opcode::Lt),
            0x1A => Ok(Opcode::Gt),
            0x1B => Ok(Opcode::Neq),
            0x1C => Ok(Opcode::Le),
            0x1D => Ok(Opcode::Ge),

            // Logic
            0x20 => Ok(Opcode::And),
            0x21 => Ok(Opcode::Or),
            0x22 => Ok(Opcode::Not),

            // Literals
            0x30 => {
                let n = leb128::read::unsigned(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::PushI64(n as i64))
            }
            0x31 => {
                let mut buf = [0u8; 8];
                cursor.read_exact(&mut buf)?;
                let n = f64::from_le_bytes(buf);
                Ok(Opcode::PushF64(n))
            }
            0x32 => {
                let len = leb128::read::unsigned(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                    as usize;
                let mut buf = vec![0u8; len];
                cursor.read_exact(&mut buf)?;
                let s = String::from_utf8(buf)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::PushStr(Rc::new(s)))
            }
            0x33 => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf)?;
                Ok(Opcode::PushBool(buf[0] != 0))
            }
            0x34 => Ok(Opcode::PushList),
            0x35 => {
                let count = leb128::read::unsigned(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                    as usize;
                let mut inner = Vec::with_capacity(count);
                for _ in 0..count {
                    let mut byte_buf = [0u8; 1];
                    cursor.read_exact(&mut byte_buf)?;
                    inner.push(Self::decode_opcode(byte_buf[0], cursor)?);
                }
                Ok(Opcode::PushRef(inner))
            }

            // List ops
            0x40 => Ok(Opcode::Nth),
            0x41 => Ok(Opcode::Append),
            0x42 => Ok(Opcode::Len),
            0x43 => Ok(Opcode::Map),
            0x44 => Ok(Opcode::Each),
            0x45 => Ok(Opcode::Fold),

            // String ops
            0x46 => Ok(Opcode::StrLen),
            0x47 => Ok(Opcode::StrCat),
            0x48 => Ok(Opcode::StrSlice),
            0x49 => Ok(Opcode::StrEq),
            0x4A => Ok(Opcode::StrLt),
            0x4B => Ok(Opcode::StrFind),
            0x4C => Ok(Opcode::StrReplace),
            0x4D => Ok(Opcode::StrToI64),
            0x4E => Ok(Opcode::I64ToStr),
            0x4F => Ok(Opcode::StrNth),
            0xB8 => Ok(Opcode::StrChars),
            0xB9 => Ok(Opcode::CharsStr),
            0xBA => Ok(Opcode::StrIter),
            0xBB => Ok(Opcode::ListFind),
            0xBC => Ok(Opcode::StrJoin),
            0xBD => Ok(Opcode::BytesNew),
            0xBE => Ok(Opcode::BytesPush),
            0xBF => Ok(Opcode::BytesLen),
            0xC0 => Ok(Opcode::BytesWriteFile),
            0xC1 => Ok(Opcode::Try),
            0xC2 => Ok(Opcode::MapNew),
            0xC3 => Ok(Opcode::MapGet),
            0xC4 => Ok(Opcode::MapSet),
            0xC5 => Ok(Opcode::MapDel),
            0xC6 => Ok(Opcode::MapKeys),
            0xC7 => Ok(Opcode::MapHas),
            0xCE => Ok(Opcode::BytesToHex),
            0xDE => Ok(Opcode::Prepend),

            // Float ops
            0xB0 => Ok(Opcode::I64ToF64),
            0xB1 => Ok(Opcode::F64ToI64),
            0xB2 => Ok(Opcode::FSqrt),
            0xB3 => Ok(Opcode::FSin),
            0xB4 => Ok(Opcode::FCos),
            0xB5 => Ok(Opcode::FTan),
            0xB6 => Ok(Opcode::JsonParse),
            0xB7 => Ok(Opcode::JsonStringify),

            // Time
            0xD0 => Ok(Opcode::Now),
            0xD1 => Ok(Opcode::Sleep),

            // Random
            0xD2 => Ok(Opcode::Random),
            0xD3 => Ok(Opcode::RandInt),

            // Float math
            0xD4 => Ok(Opcode::Floor),
            0xD5 => Ok(Opcode::Ceil),
            0xD6 => Ok(Opcode::Round),
            0xD7 => Ok(Opcode::Log),
            0xD8 => Ok(Opcode::Exp),
            0xD9 => Ok(Opcode::Atan2),

            // Bitwise
            0xDA => Ok(Opcode::BitAnd),
            0xDB => Ok(Opcode::BitOr),
            0xDC => Ok(Opcode::BitXor),
            0xDD => Ok(Opcode::BitNot),

            // Sort
            0xDF => Ok(Opcode::Sort),

            // Rev / Dedup / Take / DropN
            0xE0 => Ok(Opcode::Rev),
            0xE1 => Ok(Opcode::Dedup),
            0xE2 => Ok(Opcode::Take),
            0xE3 => Ok(Opcode::DropN),

            // Seed-runtime natives
            0xF0 => Ok(Opcode::SplitCond),
            236 => Ok(Opcode::Listen),
            237 => Ok(Opcode::Accept),
            238 => Ok(Opcode::Recv),
            239 => Ok(Opcode::Send),
            241 => Ok(Opcode::CloseSock),
            0x2A => Ok(Opcode::OpEffect),
            0x2B => Ok(Opcode::QuoteSize),
            0x2C => Ok(Opcode::WordSize),
            0x2D => Ok(Opcode::SetSeed),
            0x2E => Ok(Opcode::Rand),
            0x2F => Ok(Opcode::ConfOf),
            0x36 => Ok(Opcode::UnwrapSig),
            242 => Ok(Opcode::ReadFile),
            243 => Ok(Opcode::WriteFile),
            244 => Ok(Opcode::EnvGet),
            245 => Ok(Opcode::KvSet),
            246 => Ok(Opcode::KvGet),
            247 => Ok(Opcode::KvDel),
            248 => Ok(Opcode::KvKeys),
            255 => Ok(Opcode::ServeConcurrent),
            0xE4 => Ok(Opcode::ListCheck),
            0xE5 => Ok(Opcode::I64Check),
            0xE6 => Ok(Opcode::StrCheck),
            0xE7 => Ok(Opcode::BoolCheck),

            // Control flow
            0x50 => {
                let offset = leb128::read::signed(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::Cond(offset as i32))
            }
            0x51 => {
                let offset = leb128::read::signed(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::Jump(offset as i32))
            }
            0x52 => {
                let offset = leb128::read::signed(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::Loop(offset as i32))
            }
            0x53 => Ok(Opcode::Times),

            // Call/Return
            0x60 => {
                let len = leb128::read::unsigned(cursor)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?
                    as usize;
                let mut name_buf = vec![0u8; len];
                cursor.read_exact(&mut name_buf)?;
                let name = String::from_utf8(name_buf)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::Call(name))
            }
            0x61 => Ok(Opcode::Return),

            // Capability
            0x70 => {
                let mut buf = [0u8; 2];
                cursor.read_exact(&mut buf)?;
                let id = u16::from_le_bytes(buf);
                Ok(Opcode::CapCall(id))
            }
            0x71 => Ok(Opcode::CapExec),

            // Confidence
            0x80 => {
                let mut buf = [0u8; 8];
                cursor.read_exact(&mut buf)?;
                let conf = f64::from_le_bytes(buf);
                Ok(Opcode::ConfLabel(conf))
            }
            0x81 => Ok(Opcode::ProbChoice),

            // IO
            0x90 => Ok(Opcode::OutputTop),
            0x91 => Ok(Opcode::OutputAll),
            0x92 => Ok(Opcode::ReadInput),

            // Definitions
            0xA0 => Ok(Opcode::DefWord(String::new())),
            0xA1 => Ok(Opcode::EndDef),
            0xA2 => Ok(Opcode::Import),
            0xA3 => Ok(Opcode::Export),

            _ => Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unknown opcode byte: 0x{byte:02X}"),
            )),
        }
    }
}

fn read_leb(cursor: &mut Cursor<&[u8]>) -> std::io::Result<u64> {
    leb128::read::unsigned(cursor)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wbin_roundtrip() {
        let ops = vec![
            Opcode::PushI64(42),
            Opcode::PushI64(13),
            Opcode::Add,
            Opcode::PushI64(55),
            Opcode::Eq,
        ];
        let data = WbinWriter::write(&ops);
        let decoded = WbinReader::decode(&data).unwrap();
        assert_eq!(ops, decoded);
    }

    #[test]
    fn test_wbin_empty() {
        let ops: Vec<Opcode> = vec![];
        let data = WbinWriter::write(&ops);
        let decoded = WbinReader::decode(&data).unwrap();
        assert_eq!(ops, decoded);
    }

    #[test]
    fn test_wbin_call_roundtrip() {
        let ops = vec![
            Opcode::PushI64(10),
            Opcode::Call("double".to_string()),
            Opcode::Call("square".to_string()),
        ];
        let data = WbinWriter::write(&ops);
        let decoded = WbinReader::decode(&data).unwrap();
        assert_eq!(ops, decoded);
    }

    #[test]
    fn test_wbin_pushref_roundtrip() {
        let inner = vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return];
        let ops = vec![
            Opcode::PushI64(5),
            Opcode::PushRef(inner.clone()),
            Opcode::PushI64(10),
            Opcode::PushRef(vec![Opcode::Add, Opcode::Return]),
        ];
        let data = WbinWriter::write(&ops);
        let decoded = WbinReader::decode(&data).unwrap();
        assert_eq!(ops, decoded);
    }

    #[test]
    fn test_wbin_nested_pushref_roundtrip() {
        // PushRef containing a PushRef (nested quotations)
        let inner = vec![Opcode::Dup, Opcode::PushI64(1), Opcode::Add];
        let outer = vec![
            Opcode::PushRef(inner.clone()),
            Opcode::PushI64(10),
            Opcode::Call("apply".to_string()),
        ];
        let ops = vec![Opcode::PushRef(outer.clone())];
        let data = WbinWriter::write(&ops);
        let decoded = WbinReader::decode(&data).unwrap();
        assert_eq!(ops, decoded);
    }

    #[test]
    fn test_wbin_full_roundtrip_with_defs() {
        // Simulates a real program with Call and PushRef
        let ops = vec![
            Opcode::PushI64(10),
            Opcode::Call("double".to_string()),
            Opcode::PushStr(Rc::new("hello".to_string())),
            Opcode::PushRef(vec![
                Opcode::Dup,
                Opcode::PushI64(1),
                Opcode::Add,
                Opcode::Return,
            ]),
            Opcode::PushBool(true),
            Opcode::Cond(3),
        ];
        let data = WbinWriter::write(&ops);
        let decoded = WbinReader::decode(&data).unwrap();
        assert_eq!(ops, decoded);
    }

    // ── v2 block format ─────────────────────────────────────────

    #[test]
    fn test_wbin_v2_roundtrip_and_exec_parity() {
        use crate::block_ir::linearize;
        // loop + cond + def: control-flow-heavy program
        let src = ": sq { _ * } ; 0 { _ 2 = ?? 99 . | _ sq . ] 1 + } { _ 4 < } # drop";
        let (flat, defs_direct) = crate::block_gen::compile_block(src).unwrap();

        // Rebuild block programs via compile_unit
        let ast = whisper_parser::parser::Parser::parse_source(src).unwrap();
        let unit = crate::block_gen::compile_unit(&ast);
        let data = WbinWriter::write_unit(&unit.main, &unit.defs);

        let decoded = WbinReader::decode_unit(&data).unwrap();
        let WbinUnit::V2 { main, defs } = &decoded else {
            panic!("expected v2");
        };
        assert_eq!(linearize(main), flat);
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].0, "sq");
        // def bytecode must also match the direct compile
        let (name, code) = &defs[0];
        let flat_def = defs_direct
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c.clone())
            .expect("def present in direct compile");
        assert_eq!(linearize(code), flat_def);

        // Execution parity: v2 load + linearize == direct compile
        let (main_flat, defs_flat) = decoded.linearized();
        let mut vm = whisper_core::vm::Vm::new();
        for (n, c) in defs_flat {
            vm.define_word(n, c);
        }
        let r = vm.execute(&main_flat).unwrap();
        assert_eq!(r, None);
    }

    #[test]
    fn test_wbin_v1_still_decodable() {
        let ops = vec![Opcode::PushI64(7), Opcode::Drop];
        let data = WbinWriter::write(&ops);
        match WbinReader::decode_unit(&data).unwrap() {
            WbinUnit::V1(flat) => assert_eq!(flat, ops),
            _ => panic!("expected v1"),
        }
    }
}
