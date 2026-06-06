//! Shared WASM binary encoding utilities.
//!
//! Used by both wasm_gen (interpreter-in-WASM) and wasm_compiler (direct WASM).

/// Encode a complete WASM section: id + LEB128(len) + payload.
pub fn section(id: u8, payload: &[u8]) -> Vec<u8> {
    let mut v = vec![id];
    v.extend_from_slice(&vec_u8(payload));
    v
}

/// Encode bytes with a LEB128 length prefix.
pub fn vec_u8(data: &[u8]) -> Vec<u8> {
    let mut v = Vec::new();
    leb128_u(&mut v, data.len() as u64);
    v.extend_from_slice(data);
    v
}

/// Write an unsigned LEB128 value.
pub fn leb128_u(buf: &mut Vec<u8>, mut n: u64) {
    loop {
        let mut byte = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        buf.push(byte);
        if n == 0 {
            break;
        }
    }
}

/// Write a signed LEB128 value.
pub fn leb128_s(buf: &mut Vec<u8>, mut n: i64) {
    loop {
        let byte = (n & 0x7F) as u8;
        n >>= 7;
        if (n == 0 && (byte & 0x40) == 0) || (n == -1 && (byte & 0x40) != 0) {
            buf.push(byte);
            break;
        }
        buf.push(byte | 0x80);
    }
}

/// Write a WASM export entry: name + kind + index.
pub fn export_entry(buf: &mut Vec<u8>, name: &str, kind: u8, idx: u32) {
    buf.extend_from_slice(&vec_u8(name.as_bytes()));
    buf.push(kind);
    leb128_u(buf, idx as u64);
}
