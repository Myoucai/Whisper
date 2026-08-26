//! UTC timestamp formatting (v1.0.4 `strftime`).
//!
//! Hand-rolled on both implementations (Rust and runtime.c) so the two
//! chains stay byte-identical without platform strftime/locale quirks.
//! Supported specifiers: %Y %m %d %H %M %S %%; anything else copies the
//! '%' and the character through. Multibyte UTF-8 passes through intact
//! (byte-wise scan only intercepts ASCII '%').

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn format_utc(ts: i64, fmt: &str) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (y, mo, d) = civil_from_days(days);
    let (h, mi, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);

    let mut out: Vec<u8> = Vec::with_capacity(fmt.len() + 8);
    let b = fmt.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 1 < b.len() {
            let spec = match b[i + 1] {
                b'Y' => Some(format!("{y:04}")),
                b'm' => Some(format!("{mo:02}")),
                b'd' => Some(format!("{d:02}")),
                b'H' => Some(format!("{h:02}")),
                b'M' => Some(format!("{mi:02}")),
                b'S' => Some(format!("{s:02}")),
                b'%' => Some("%".to_string()),
                _ => None,
            };
            if let Some(t) = spec {
                out.extend_from_slice(t.as_bytes());
                i += 2;
                continue;
            }
            out.push(b'%'); // unknown specifier: copy through
            i += 1;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8(out).unwrap_or_default()
}
