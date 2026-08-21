//! Opcode → C code transpiler.
//!
//! Each Whisper opcode is translated to one or more C statements using
//! the runtime functions defined in runtime.c.
//!
//! Output structure:
//!   - Forward declarations for word functions
//!   - Word function definitions (one per word definition)
//!   - main() function containing the translated main bytecode

use std::collections::HashMap;
use std::fmt::Write;
use whisper_core::opcode::Opcode;

/// String pool for collecting string constants during transpilation.
struct StringPool {
    strings: Vec<String>,
    indices: HashMap<String, usize>,
}

impl StringPool {
    fn new() -> Self {
        StringPool {
            strings: Vec::new(),
            indices: HashMap::new(),
        }
    }

    fn intern(&mut self, s: &str) -> usize {
        if let Some(&idx) = self.indices.get(s) {
            return idx;
        }
        let idx = self.strings.len();
        self.strings.push(s.to_string());
        self.indices.insert(s.to_string(), idx);
        idx
    }

    fn emit_declarations(&self, out: &mut String) {
        for (i, _s) in self.strings.iter().enumerate() {
            let _ = writeln!(out, "static const char *w_str_{i} = NULL;");
        }
        let _ = writeln!(out, "static void w_init_strings(void) {{");
        for (i, s) in self.strings.iter().enumerate() {
            let escaped = escape_c_string(s);
            let _ = writeln!(out, "  w_str_{i} = w_str_new(\"{escaped}\", {});", s.len());
        }
        let _ = writeln!(out, "}}");
    }
}

/// Escape a string for C string literal.
fn escape_c_string(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7E => out.push(b as char),
            _ => {
                let _ = write!(out, "\\x{:02x}", b);
            }
        }
    }
    out
}

// Private helper — see public transpile_to_c below for the full pipeline.

/// Collect all string constants from a bytecode sequence.
fn collect_strings(opcodes: &[Opcode], strings: &mut StringPool) {
    for op in opcodes {
        match op {
            Opcode::PushStr(s) => { strings.intern(s); }
            Opcode::PushRef(inner) => { collect_strings(inner, strings); }
            _ => {}
        }
    }
}

/// Make a string safe for use as a C identifier.
fn safe_c_name(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
    }
    out
}

/// Translate a sequence of opcodes to C statements.
fn transpile_opcodes(opcodes: &[Opcode], out: &mut String, strings: &mut StringPool, indent: usize) {
    let pad = " ".repeat(indent);
    let labels = collect_labels(opcodes);
    let label_set: std::collections::HashSet<usize> = labels.iter().copied().collect();

    let max_label = labels.last().copied().unwrap_or(0);

    let mut i = 0;
    while i < opcodes.len() {
        // Emit label if this position is a jump target
        if label_set.contains(&i) {
            let _ = writeln!(out, "L_{i}:");
        }
        let op = &opcodes[i];
        match op {
            // ── Literals ────────────────────────────────────────
            Opcode::PushI64(n) => {
                let _ = writeln!(out, "{pad}w_push((wval)(int64_t){n}L);");
            }
            Opcode::PushF64(n) => {
                let bits = n.to_bits();
                let _ = writeln!(out, "{pad}w_push((wval){bits}ULL); // f64: {n}");
            }
            Opcode::PushBool(b) => {
                let _ = writeln!(out, "{pad}w_push((wval){}L);", if *b { 1 } else { 0 });
            }
            Opcode::PushStr(s) => {
                let idx = strings.intern(s);
                let _ = writeln!(out, "{pad}w_push((wval)w_str_{idx});");
            }
            Opcode::PushList => {
                // Previous opcodes should have pushed count + elements
                // The runtime expects: elements on stack, then count on top
                // w_list_new allocates, then we copy from stack
                let _ = writeln!(out, "{{");
                let _ = writeln!(out, "{pad}  int64_t _count = (int64_t)w_pop();");
                let _ = writeln!(out, "{pad}  wval *_list = w_list_new(_count);");
                let _ = writeln!(out, "{pad}  for (int64_t _i = _count - 1; _i >= 0; _i--) {{");
                let _ = writeln!(out, "{pad}    _list[1 + _i] = w_pop();");
                let _ = writeln!(out, "{pad}  }}");
                let _ = writeln!(out, "{pad}  w_push((wval)_list);");
                let _ = writeln!(out, "}}");
            }
            Opcode::PushRef(inner) => {
                // For inline quotes, we need to execute them as a block
                // We'll use a label + goto pattern or a nested function
                // For simplicity, we'll inline the block with a marker
                let _ = writeln!(out, "{pad}// PushRef: inline quote block");
                transpile_opcodes(inner, out, strings, indent);
            }

            // ── Stack operations ────────────────────────────────
            Opcode::Dup => { let _ = writeln!(out, "{pad}w_dup();"); }
            Opcode::Swap => { let _ = writeln!(out, "{pad}w_swap();"); }
            Opcode::Drop => { let _ = writeln!(out, "{pad}w_drop();"); }
            Opcode::Rot => { let _ = writeln!(out, "{pad}w_rot();"); }
            Opcode::Pick(n) => { let _ = writeln!(out, "{pad}w_pick_n({n});"); }

            // ── Arithmetic ──────────────────────────────────────
            Opcode::Add => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a + (int64_t)_b)); }}");
            }
            Opcode::Sub => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a - (int64_t)_b)); }}");
            }
            Opcode::Mul => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a * (int64_t)_b)); }}");
            }
            Opcode::Div => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); if ((int64_t)_b == 0) {{ fprintf(stderr, \"division by zero\\n\"); exit(1); }} w_push((wval)((int64_t)_a / (int64_t)_b)); }}");
            }
            Opcode::Mod => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); if ((int64_t)_b == 0) {{ fprintf(stderr, \"division by zero\\n\"); exit(1); }} w_push((wval)((int64_t)_a % (int64_t)_b)); }}");
            }

            // ── Comparison ──────────────────────────────────────
            Opcode::Eq => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a == (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Neq => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a != (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Lt => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a < (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Gt => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a > (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Le => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a <= (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Ge => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a >= (int64_t)_b ? 1L : 0L)); }}");
            }

            // ── Logic ───────────────────────────────────────────
            Opcode::And => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a && (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Or => {
                let _ = writeln!(out, "{{ wval _b = w_pop(), _a = w_pop(); w_push((wval)((int64_t)_a || (int64_t)_b ? 1L : 0L)); }}");
            }
            Opcode::Not => {
                let _ = writeln!(out, "{{ wval _a = w_pop(); w_push((wval)((int64_t)_a == 0 ? 1L : 0L)); }}");
            }

            // ── Control flow ────────────────────────────────────
            Opcode::Cond(offset) => {
                let _ = writeln!(out, "{{ wval _cond = w_pop(); if ((int64_t)_cond == 0) goto L_{}; }}", i as i32 + 1 + offset);
            }
            Opcode::Jump(offset) => {
                let _ = writeln!(out, "{pad}goto L_{};", i as i32 + 1 + offset);
            }
            Opcode::Loop(offset) => {
                let _ = writeln!(out, "{{ wval _cond = w_pop(); if ((int64_t)_cond != 0) goto L_{}; }}", i as i32 + 1 + offset);
            }
            Opcode::Times => {
                // n {body} @times — body was already pushed as PushRef
                // For now, generate a for loop
                let _ = writeln!(out, "{pad}// @times: body should be inline before this");
                let _ = writeln!(out, "{pad}{{ int64_t _n = (int64_t)w_pop(); for (int64_t _ti = 0; _ti < _n; _ti++) {{");
                // The body was inlined before this opcode
                let _ = writeln!(out, "{pad}}}}}");
            }

            // ── Call/Return ─────────────────────────────────────
            Opcode::Call(name) => {
                let cname = safe_c_name(name);
                let _ = writeln!(out, "{pad}w_{cname}();");
            }
            Opcode::Return => {
                let _ = writeln!(out, "{pad}return;");
            }

            // ── List operations ─────────────────────────────────
            Opcode::Nth => {
                let _ = writeln!(out, "{{ int64_t _idx = (int64_t)w_pop(); wval *_list = (wval *)w_pop(); w_push(w_list_nth(_list, _idx)); }}");
            }
            Opcode::Append => {
                let _ = writeln!(out, "{{ wval _elem = w_pop(); wval *_list = (wval *)w_pop(); w_push((wval)w_list_append(_list, _elem)); }}");
            }
            Opcode::Prepend => {
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); wval _elem = w_pop(); w_push((wval)w_list_prepend(_elem, _list)); }}");
            }
            Opcode::Len => {
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); w_push((wval)w_list_len(_list)); }}");
            }
            Opcode::Map => {
                // list {fn} @map — fn was inlined as PushRef before this
                let _ = writeln!(out, "{pad}// @map: requires callback function");
                let _ = writeln!(out, "{pad}{{ wval *_fn_ref = (wval *)w_pop(); wval *_list = (wval *)w_pop(); /* TODO: execute fn for each element */ w_push((wval)w_list_new(0)); }}");
            }
            Opcode::Each => {
                let _ = writeln!(out, "{pad}// @each: requires callback function");
            }
            Opcode::Fold => {
                let _ = writeln!(out, "{pad}// @fold: requires callback function");
            }
            Opcode::Filter => {
                let _ = writeln!(out, "{pad}// @filter: requires callback function");
            }
            Opcode::Sort => {
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); w_push((wval)w_list_sort(_list)); }}");
            }
            Opcode::Rev => {
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); w_push((wval)w_list_rev(_list)); }}");
            }
            Opcode::Dedup => {
                let _ = writeln!(out, "{pad}// @dedup: TODO");
            }
            Opcode::Take => {
                let _ = writeln!(out, "{{ int64_t _n = (int64_t)w_pop(); wval *_list = (wval *)w_pop(); w_push((wval)w_list_take(_list, _n)); }}");
            }
            Opcode::DropN => {
                let _ = writeln!(out, "{{ int64_t _n = (int64_t)w_pop(); wval *_list = (wval *)w_pop(); w_push((wval)w_list_drop(_list, _n)); }}");
            }
            Opcode::ListSlice => {
                let _ = writeln!(out, "{{ int64_t _len = (int64_t)w_pop(); int64_t _start = (int64_t)w_pop(); wval *_list = (wval *)w_pop(); w_push((wval)w_list_slice(_list, _start, _len)); }}");
            }
            Opcode::ListFind => {
                let _ = writeln!(out, "{pad}// @listfind: TODO");
            }

            // ── String operations ───────────────────────────────
            Opcode::StrLen => {
                let _ = writeln!(out, "{{ const char *_s = (const char *)w_pop(); w_push((wval)(int64_t)w_str_len(_s)); }}");
            }
            Opcode::StrCat => {
                let _ = writeln!(out, "{{ const char *_b = (const char *)w_pop(); const char *_a = (const char *)w_pop(); w_push((wval)w_str_concat(_a, _b)); }}");
            }
            Opcode::StrSlice => {
                let _ = writeln!(out, "{{ int64_t _len = (int64_t)w_pop(); int64_t _start = (int64_t)w_pop(); const char *_s = (const char *)w_pop(); w_push((wval)w_str_slice(_s, _start, _len)); }}");
            }
            Opcode::StrEq => {
                let _ = writeln!(out, "{{ const char *_b = (const char *)w_pop(); const char *_a = (const char *)w_pop(); w_push((wval)w_str_eq(_a, _b)); }}");
            }
            Opcode::StrLt => {
                let _ = writeln!(out, "{{ const char *_b = (const char *)w_pop(); const char *_a = (const char *)w_pop(); w_push((wval)w_str_lt(_a, _b)); }}");
            }
            Opcode::StrFind => {
                let _ = writeln!(out, "{{ const char *_needle = (const char *)w_pop(); const char *_hay = (const char *)w_pop(); w_push((wval)w_str_find(_hay, _needle)); }}");
            }
            Opcode::StrReplace => {
                let _ = writeln!(out, "{{ const char *_new = (const char *)w_pop(); const char *_old = (const char *)w_pop(); const char *_src = (const char *)w_pop(); w_push((wval)w_str_replace(_src, _old, _new)); }}");
            }
            Opcode::StrToI64 => {
                let _ = writeln!(out, "{{ const char *_s = (const char *)w_pop(); w_push((wval)w_str_to_i64(_s)); }}");
            }
            Opcode::I64ToStr => {
                let _ = writeln!(out, "{{ int64_t _n = (int64_t)w_pop(); w_push((wval)w_i64_to_str(_n)); }}");
            }
            Opcode::StrNth => {
                let _ = writeln!(out, "{{ int64_t _idx = (int64_t)w_pop(); const char *_s = (const char *)w_pop(); w_push((wval)(int64_t)(_s[_idx])); }}");
            }
            Opcode::StrChars => {
                let _ = writeln!(out, "{{ const char *_s = (const char *)w_pop(); uint32_t _len = w_str_len(_s); wval *_list = w_list_new(_len); for (uint32_t _i = 0; _i < _len; _i++) _list[1+_i] = (wval)(int64_t)_s[_i]; w_push((wval)_list); }}");
            }
            Opcode::CharsStr => {
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); int64_t _count = w_list_len(_list); char *_buf = w_alloc(_count + 1); for (int64_t _i = 0; _i < _count; _i++) _buf[_i] = (char)(int64_t)_list[1+_i]; w_push((wval)w_str_new(_buf, _count)); }}");
            }
            Opcode::StrIter => {
                let _ = writeln!(out, "{{ const char *_s = (const char *)w_pop(); if (w_str_len(_s) == 0) {{ w_push((wval)-1LL); w_push((wval)w_str_new(\"\", 0)); }} else {{ w_push((wval)(int64_t)_s[0]); w_push((wval)w_str_new(_s+1, w_str_len(_s)-1)); }} }}");
            }
            Opcode::StrJoin => {
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); int64_t _count = w_list_len(_list); size_t _total = 0; for (int64_t _i = 0; _i < _count; _i++) _total += w_str_len((const char*)_list[1+_i]); char *_buf = w_alloc(_total); size_t _pos = 0; for (int64_t _i = 0; _i < _count; _i++) {{ const char *_s = (const char*)_list[1+_i]; uint32_t _l = w_str_len(_s); memcpy(_buf+_pos, _s, _l); _pos += _l; }} w_push((wval)w_str_new(_buf, _total)); }}");
            }
            Opcode::StrSplit => {
                let _ = writeln!(out, "{pad}// @strsplit: TODO");
            }
            Opcode::Ctos => {
                let _ = writeln!(out, "{{ int64_t _c = (int64_t)w_pop(); w_push((wval)rt_ctos(_c)); }}");
            }

            // ── Map operations ──────────────────────────────────
            Opcode::MapNew => {
                let _ = writeln!(out, "{pad}w_push((wval)w_map_new());");
            }
            Opcode::MapGet => {
                let _ = writeln!(out, "{{ const char *_key = (const char *)w_pop(); wval *_map = (wval *)w_pop(); w_push(w_map_get(_map, _key)); }}");
            }
            Opcode::MapSet => {
                let _ = writeln!(out, "{{ wval _val = w_pop(); const char *_key = (const char *)w_pop(); wval *_map = (wval *)w_pop(); w_push((wval)w_map_set(_map, _key, _val)); }}");
            }
            Opcode::MapDel => {
                let _ = writeln!(out, "{{ const char *_key = (const char *)w_pop(); wval *_map = (wval *)w_pop(); w_push((wval)w_map_del(_map, _key)); }}");
            }
            Opcode::MapKeys => {
                let _ = writeln!(out, "{{ wval *_map = (wval *)w_pop(); w_push((wval)w_map_keys(_map)); }}");
            }
            Opcode::MapHas => {
                let _ = writeln!(out, "{{ const char *_key = (const char *)w_pop(); wval *_map = (wval *)w_pop(); w_push((wval)w_map_has(_map, _key)); }}");
            }

            // ── Float operations ────────────────────────────────
            Opcode::I64ToF64 => {
                let _ = writeln!(out, "{{ int64_t _n = (int64_t)w_pop(); double _f = (double)_n; w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::F64ToI64 => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; w_push((wval)(int64_t)_f); }}");
            }
            Opcode::FSqrt => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = w_fsqrt(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::FSin => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = w_fsin(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::FCos => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = w_fcos(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::FTan => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = w_ftan(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::Floor => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = floor(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::Ceil => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = ceil(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::Round => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = round(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::Log => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = w_flog(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::Exp => {
                let _ = writeln!(out, "{{ uint64_t _bits = (uint64_t)w_pop(); double _f = *(double*)&_bits; _f = w_fexp(_f); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::Atan2 => {
                let _ = writeln!(out, "{{ uint64_t _bx = (uint64_t)w_pop(); uint64_t _by = (uint64_t)w_pop(); double _x = *(double*)&_bx; double _y = *(double*)&_by; double _r = w_fatan2(_y, _x); w_push((wval)*(uint64_t*)&_r); }}");
            }

            // ── Bitwise operations ──────────────────────────────
            Opcode::Shl => {
                let _ = writeln!(out, "{{ int64_t _b = (int64_t)w_pop(); int64_t _a = (int64_t)w_pop(); w_push((wval)w_shl(_a, _b)); }}");
            }
            Opcode::Shr => {
                let _ = writeln!(out, "{{ int64_t _b = (int64_t)w_pop(); int64_t _a = (int64_t)w_pop(); w_push((wval)w_shr(_a, _b)); }}");
            }
            Opcode::BitAnd => {
                let _ = writeln!(out, "{{ int64_t _b = (int64_t)w_pop(); int64_t _a = (int64_t)w_pop(); w_push((wval)w_bitand(_a, _b)); }}");
            }
            Opcode::BitOr => {
                let _ = writeln!(out, "{{ int64_t _b = (int64_t)w_pop(); int64_t _a = (int64_t)w_pop(); w_push((wval)w_bitor(_a, _b)); }}");
            }
            Opcode::BitXor => {
                let _ = writeln!(out, "{{ int64_t _b = (int64_t)w_pop(); int64_t _a = (int64_t)w_pop(); w_push((wval)w_bitxor(_a, _b)); }}");
            }
            Opcode::BitNot => {
                let _ = writeln!(out, "{{ int64_t _a = (int64_t)w_pop(); w_push((wval)w_bitnot(_a)); }}");
            }

            // ── JSON ────────────────────────────────────────────
            Opcode::JsonParse => {
                let _ = writeln!(out, "{pad}// json_parse: TODO (simplified)");
            }
            Opcode::JsonStringify => {
                let _ = writeln!(out, "{{ wval _v = w_pop(); w_push((wval)w_json_stringify(_v)); }}");
            }

            // ── IO ──────────────────────────────────────────────
            Opcode::OutputTop => {
                let _ = writeln!(out, "{pad}w_print_val(w_pop());");
            }
            Opcode::OutputAll => {
                let _ = writeln!(out, "{pad}{{ printf(\"Stack: \"); for (int _i = 0; _i < w_sp; _i++) printf(\"%ld \", (int64_t)w_stack[_i]); printf(\"\\n\"); }}");
            }
            Opcode::ReadInput => {
                let _ = writeln!(out, "{pad}w_push((wval)w_read_line());");
            }

            // ── Byte buffer ─────────────────────────────────────
            Opcode::BytesNew => {
                let _ = writeln!(out, "{pad}w_push((wval)w_list_new(0)); // byte buffer as list");
            }
            Opcode::BytesPush => {
                let _ = writeln!(out, "{pad}// bytes_push: TODO");
            }
            Opcode::BytesLen => {
                let _ = writeln!(out, "{pad}// bytes_len: TODO");
            }
            Opcode::BytesWriteFile => {
                let _ = writeln!(out, "{pad}// bytes_write_file: TODO");
            }

            // ── Try ─────────────────────────────────────────────
            Opcode::Try => {
                let _ = writeln!(out, "{pad}// try: TODO (simplified — just execute)");
            }

            // ── Time/Random ─────────────────────────────────────
            Opcode::Now => {
                let _ = writeln!(out, "{pad}w_push((wval)w_now());");
            }
            Opcode::Sleep => {
                let _ = writeln!(out, "{{ int64_t _ms = (int64_t)w_pop(); w_sleep(_ms); }}");
            }
            Opcode::Random => {
                let _ = writeln!(out, "{{ double _f = w_random(); w_push((wval)*(uint64_t*)&_f); }}");
            }
            Opcode::RandInt => {
                let _ = writeln!(out, "{{ int64_t _max = (int64_t)w_pop(); int64_t _min = (int64_t)w_pop(); w_push((wval)w_randint(_min, _max)); }}");
            }

            // ── Args ────────────────────────────────────────────
            Opcode::Args => {
                let _ = writeln!(out, "{pad}w_push((wval)w_args());");
            }

            // ── Capability (no-op in native) ────────────────────
            Opcode::CapCall(_) => {
                let _ = writeln!(out, "{pad}// cap_call: no-op in native mode");
            }
            Opcode::CapExec => {
                let _ = writeln!(out, "{pad}// cap_exec: no-op in native mode");
            }

            // ── Confidence (simplified — pass through value) ────
            Opcode::ConfLabel(_) => {
                let _ = writeln!(out, "{pad}// conf_label: pass through value");
            }
            Opcode::ProbChoice => {
                let _ = writeln!(out, "{pad}// prob_choice: always execute alt1");
            }

            // ── Definition markers (no-op) ──────────────────────
            Opcode::DefWord(_) | Opcode::EndDef | Opcode::Import | Opcode::Export => {
                // These are compile-time only
            }
        }

        // Emit label for jumps targeting this position
        // (labels are emitted before the instruction for forward jumps)
        i += 1;
    }
    // Emit trailing labels (jump targets beyond the last instruction)
    for &lbl in &labels {
        if lbl >= opcodes.len() {
            let _ = writeln!(out, "L_{lbl}: ;");
        }
    }
}

/// Generate labels for all jump/cond/loop targets in the bytecode.
/// This is needed so we can emit `goto L_N;` labels.
fn collect_labels(opcodes: &[Opcode]) -> Vec<usize> {
    let mut labels = std::collections::HashSet::new();
    let mut i = 0;
    while i < opcodes.len() {
        match &opcodes[i] {
            Opcode::Cond(offset) => {
                let target = (i as i32 + 1 + offset) as usize;
                labels.insert(target);
            }
            Opcode::Jump(offset) => {
                let target = (i as i32 + 1 + offset) as usize;
                labels.insert(target);
            }
            Opcode::Loop(offset) => {
                let target = (i as i32 + 1 + offset) as usize;
                labels.insert(target);
            }
            _ => {}
        }
        i += 1;
    }
    let mut labels: Vec<usize> = labels.into_iter().collect();
    labels.sort();
    labels
}

/// Generate a complete C program from bytecodes and definitions.
/// This is the main entry point for the C transpiler.
pub fn transpile_to_c(
    main_bytecode: &[Opcode],
    defs: &[(String, Vec<Opcode>)],
) -> String {
    let mut out = String::new();
    let mut strings = StringPool::new();

    // ── Header ──────────────────────────────────────────────────
    let _ = writeln!(out, "// Auto-generated by Whisper C transpiler");
    let _ = writeln!(out, "#include \"runtime.c\"");
    let _ = writeln!(out);

    // ── Collect string constants ────────────────────────────────
    collect_strings(main_bytecode, &mut strings);
    for (_, code) in defs {
        collect_strings(code, &mut strings);
    }

    // ── Emit string constants ───────────────────────────────────
    strings.emit_declarations(&mut out);
    let _ = writeln!(out);

    // ── Forward declarations ────────────────────────────────────
    for (name, _) in defs {
        let cname = safe_c_name(name);
        let _ = writeln!(out, "static void w_{cname}(void);");
    }
    let _ = writeln!(out);

    // ── Word function definitions ───────────────────────────────
    for (name, code) in defs {
        let cname = safe_c_name(name);
        let _ = writeln!(out, "static void w_{cname}(void) {{");
        transpile_opcodes(code, &mut out, &mut strings, 2);
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);
    }

    // ── main() ──────────────────────────────────────────────────
    let _ = writeln!(out, "int main(int argc, char **argv) {{");
    let _ = writeln!(out, "  w_init(argc, argv);");
    let _ = writeln!(out, "  w_init_strings();");
    transpile_opcodes(main_bytecode, &mut out, &mut strings, 2);
    let _ = writeln!(out, "  return 0;");
    let _ = writeln!(out, "}}");

    out
}
