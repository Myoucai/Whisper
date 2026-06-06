/// .wbin binary format reader/writer.
///
/// Format specification (per design doc section 5.2):
///   Header: 4 bytes magic "WHSP" + 4 bytes version (u32 LE)
///   Body:   LEB128-encoded opcodes

use whisper_core::opcode::Opcode;
use std::io::{Cursor, Read};

/// Magic bytes for .wbin files.
pub const WBIN_MAGIC: &[u8; 4] = b"WHSP";
/// Current .wbin format version.
pub const WBIN_VERSION: u32 = 1;

/// Writer for .wbin binary format.
pub struct WbinWriter;

impl WbinWriter {
    /// Serialize a sequence of opcodes to .wbin format bytes.
    pub fn write(opcodes: &[Opcode]) -> Vec<u8> {
        let mut buf = Vec::new();

        // Header
        buf.extend_from_slice(WBIN_MAGIC);
        buf.extend_from_slice(&WBIN_VERSION.to_le_bytes());

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
            Opcode::Call(_name) => {
                // Call with name — encode as string length + bytes
                // For roundtrip stability, encode the name
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

impl WbinReader {
    /// Read and decode opcodes from a .wbin file.
    pub fn read_from_file(path: &std::path::Path) -> std::io::Result<Vec<Opcode>> {
        let data = std::fs::read(path)?;
        Self::decode(&data)
    }

    /// Decode .wbin bytes into opcodes.
    pub fn decode(data: &[u8]) -> std::io::Result<Vec<Opcode>> {
        if data.len() < 8 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "File too short for .wbin header",
            ));
        }

        // Verify magic
        if &data[0..4] != WBIN_MAGIC {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid magic bytes — not a .wbin file",
            ));
        }

        // Read version
        let version = u32::from_le_bytes(data[4..8].try_into().unwrap());
        if version != WBIN_VERSION {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Unsupported .wbin version: {version}"),
            ));
        }

        let mut cursor = Cursor::new(&data[8..]);
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

        Ok(opcodes)
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
                let n = leb128::read::unsigned(cursor).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                Ok(Opcode::PushI64(n as i64))
            }
            0x31 => {
                let mut buf = [0u8; 8];
                cursor.read_exact(&mut buf)?;
                let n = f64::from_le_bytes(buf);
                Ok(Opcode::PushF64(n))
            }
            0x32 => {
                let len = leb128::read::unsigned(cursor).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })? as usize;
                let mut buf = vec![0u8; len];
                cursor.read_exact(&mut buf)?;
                let s = String::from_utf8(buf)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                Ok(Opcode::PushStr(s))
            }
            0x33 => {
                let mut buf = [0u8; 1];
                cursor.read_exact(&mut buf)?;
                Ok(Opcode::PushBool(buf[0] != 0))
            }
            0x34 => Ok(Opcode::PushList),
            0x35 => Ok(Opcode::PushRef(vec![])), // placeholder, needs decoder support

            // List ops
            0x40 => Ok(Opcode::Nth),
            0x41 => Ok(Opcode::Append),
            0x42 => Ok(Opcode::Len),
            0x43 => Ok(Opcode::Map),
            0x44 => Ok(Opcode::Each),
            0x45 => Ok(Opcode::Fold),

            // Control flow
            0x50 => {
                let offset = leb128::read::signed(cursor).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                Ok(Opcode::Cond(offset as i32))
            }
            0x51 => {
                let offset = leb128::read::signed(cursor).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                Ok(Opcode::Jump(offset as i32))
            }
            0x52 => {
                let offset = leb128::read::signed(cursor).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                Ok(Opcode::Loop(offset as i32))
            }
            0x53 => Ok(Opcode::Times),

            // Call/Return
            0x60 => {
                let idx = leb128::read::unsigned(cursor).map_err(|e| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                })?;
                Ok(Opcode::Call(format!("_{idx}")))
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
}
