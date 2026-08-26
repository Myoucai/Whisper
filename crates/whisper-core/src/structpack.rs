//! Binary struct packing (v1.0.6), little-endian.
//!
//! Format chars: `i` i32, `I` u32, `h` i16, `H` u16, `b` i8, `B` u8,
//! `q` i64, `Q` u64, `d` f64 (bit pattern), `s` NUL-terminated string.
//! A leading `<` is accepted (LE is the only endianness in v1.0.6);
//! anything else in the format is an error. Integers wider than the
//! field truncate like a C cast (wrapping).

use crate::value::Value;

fn put_u64(out: &mut Vec<u8>, v: u64, width: usize) {
    for k in 0..width {
        out.push((v >> (8 * k)) as u8);
    }
}

fn get_u64(b: &[u8], width: usize) -> u64 {
    let mut v: u64 = 0;
    for (k, byte) in b.iter().take(width).enumerate() {
        v |= (*byte as u64) << (8 * k);
    }
    v
}

fn width_of(c: u8) -> Option<usize> {
    match c {
        b'i' | b'I' => Some(4),
        b'h' | b'H' => Some(2),
        b'b' | b'B' => Some(1),
        b'q' | b'Q' | b'd' => Some(8),
        b's' => None, // variable width (NUL-terminated)
        _ => None,
    }
}

/// values fmt -> packed bytes (None on arity/type/format error)
pub fn pack(values: &[Value], fmt: &[u8]) -> Option<Vec<u8>> {
    let f = if !fmt.is_empty() && fmt[0] == b'<' {
        &fmt[1..]
    } else {
        fmt
    };
    let mut out = Vec::new();
    let mut vi = 0usize;
    let mut i = 0usize;
    while i < f.len() {
        let c = f[i];
        i += 1;
        if c == b's' {
            let v = values.get(vi)?;
            vi += 1;
            match v {
                Value::Str(s) => {
                    out.extend_from_slice(s.as_bytes());
                    out.push(0);
                }
                _ => return None,
            }
            continue;
        }
        let w = width_of(c)?;
        let v = values.get(vi)?;
        vi += 1;
        let raw: u64 = match v {
            Value::I64(n) => *n as u64,
            Value::Bool(b) => *b as u64,
            Value::F64(x) => x.to_bits(),
            _ => return None,
        };
        if c == b'd' && !matches!(v, Value::F64(_)) {
            return None;
        }
        put_u64(&mut out, raw, w);
    }
    if vi != values.len() {
        return None; // leftover values
    }
    Some(out)
}

/// bytes fmt -> values (None on truncation or bad format)
pub fn unpack(bytes: &[u8], fmt: &[u8]) -> Option<Vec<Value>> {
    use std::rc::Rc;
    let f = if !fmt.is_empty() && fmt[0] == b'<' {
        &fmt[1..]
    } else {
        fmt
    };
    let mut out = Vec::new();
    let mut pos = 0usize;
    let mut i = 0usize;
    while i < f.len() {
        let c = f[i];
        i += 1;
        if c == b's' {
            let start = pos;
            while pos < bytes.len() && bytes[pos] != 0 {
                pos += 1;
            }
            if pos == bytes.len() {
                return None; // missing NUL
            }
            out.push(Value::Str(Rc::new(
                String::from_utf8(bytes[start..pos].to_vec()).ok()?,
            )));
            pos += 1; // consume NUL
            continue;
        }
        let w = width_of(c)?;
        if pos + w > bytes.len() {
            return None; // truncated
        }
        let raw = get_u64(&bytes[pos..], w);
        pos += w;
        let v = match c {
            b'i' | b'h' | b'b' | b'q' => {
                // sign-extend from the field width
                let shift = 64 - 8 * w;
                Value::I64(((raw << shift) as i64) >> shift)
            }
            b'd' => Value::F64(f64::from_bits(raw)),
            _ => Value::I64(raw as i64),
        };
        out.push(v);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_ints() {
        let p = pack(&[Value::I64(1), Value::I64(2)], b"<ih").unwrap();
        assert_eq!(p, vec![1, 0, 0, 0, 2, 0]);
    }

    #[test]
    fn roundtrip_ints() {
        let vals = [Value::I64(-2), Value::I64(300), Value::I64(-30000)];
        let p = pack(&vals, b"<bih").unwrap();
        let u = unpack(&p, b"<bih").unwrap();
        assert_eq!(u, vals.to_vec());
    }

    #[test]
    fn f64_bits() {
        let p = pack(&[Value::F64(3.5)], b"<d").unwrap();
        assert_eq!(p.len(), 8);
        let u = unpack(&p, b"<d").unwrap();
        match &u[0] {
            Value::F64(x) => assert_eq!(*x, 3.5),
            other => panic!("expected f64, got {other:?}"),
        }
    }

    #[test]
    fn strings() {
        // value 7 is not a string for 's' — must fail
        assert!(pack(&[Value::I64(7)], b"<bs").is_none());
        let p = pack(
            &[
                Value::I64(7),
                crate::value::Value::Str(std::rc::Rc::new("hi".into())),
            ],
            b"<bs",
        )
        .unwrap();
        assert_eq!(p, vec![7, b'h', b'i', 0]);
        let u = unpack(&p, b"<bs").unwrap();
        assert_eq!(u[0], Value::I64(7));
        match &u[1] {
            Value::Str(s) => assert_eq!(s.as_ref(), "hi"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn truncation_and_sign() {
        let p = pack(&[Value::I64(0x1FF)], b"<b").unwrap();
        assert_eq!(p, vec![0xFF]); // wraps like a C cast
        let u = unpack(&p, b"<b").unwrap();
        assert_eq!(u[0], Value::I64(-1)); // sign-extended
        assert!(unpack(&[1, 2], b"<i").is_none()); // truncated
    }
}
