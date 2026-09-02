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

/// Pool of quote blocks: every PushRef becomes a C function `W_qN` whose
/// pointer is pushed on the stack; higher-order opcodes call back through
/// it (mirrors the seed emitter over runtime.c).
struct QuotePool {
    quotes: Vec<std::rc::Rc<[Opcode]>>,
    ids: HashMap<(usize, usize), usize>,
}

impl QuotePool {
    fn new() -> Self {
        QuotePool {
            quotes: Vec::new(),
            ids: HashMap::new(),
        }
    }

    fn intern(&mut self, inner: &[Opcode]) -> usize {
        let key = (inner.as_ptr() as usize, inner.len());
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.quotes.len();
        self.quotes
            .push(std::rc::Rc::from(inner.to_vec().into_boxed_slice()));
        self.ids.insert(key, id);
        id
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
            Opcode::PushStr(s) => {
                strings.intern(s);
            }
            Opcode::PushRef(inner) => {
                collect_strings(inner, strings);
            }
            _ => {}
        }
    }
}

/// Recursively register every PushRef block (including nested ones).
fn collect_quotes(opcodes: &[Opcode], pool: &mut QuotePool) {
    for op in opcodes {
        if let Opcode::PushRef(inner) = op {
            pool.intern(inner);
            collect_quotes(inner, pool);
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
fn transpile_opcodes(
    opcodes: &[Opcode],
    out: &mut String,
    strings: &mut StringPool,
    quotes: &mut QuotePool,
    indent: usize,
) {
    let pad = " ".repeat(indent);
    let labels = collect_labels(opcodes);
    let label_set: std::collections::HashSet<usize> = labels.iter().copied().collect();

    let mut i = 0;
    while i < opcodes.len() {
        // Emit label if this position is a jump target
        if label_set.contains(&i) {
            let _ = writeln!(out, "L_{i}:");
        }
        let op = &opcodes[i];
        match op {
            Opcode::OpEffect | Opcode::QuoteSize | Opcode::WordSize => {
                let _ = writeln!(out, "{pad}#error unsupported: op-effect/size-of/word-size");
            }
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
                let _ = writeln!(
                    out,
                    "{pad}  for (int64_t _i = _count - 1; _i >= 0; _i--) {{"
                );
                let _ = writeln!(out, "{pad}    _list[1 + _i] = w_pop();");
                let _ = writeln!(out, "{pad}  }}");
                let _ = writeln!(out, "{pad}  w_push((wval)_list);");
                let _ = writeln!(out, "}}");
            }
            Opcode::PushRef(inner) => {
                // Quotes are first-class: push the C function pointer.
                let id = quotes.intern(inner);
                let _ = writeln!(out, "{pad}w_push((wval)(uintptr_t)W_q{id});");
            }

            // ── Stack operations ────────────────────────────────
            Opcode::Dup => {
                let _ = writeln!(out, "{pad}w_dup();");
            }
            Opcode::Swap => {
                let _ = writeln!(out, "{pad}w_swap();");
            }
            Opcode::Drop => {
                let _ = writeln!(out, "{pad}w_drop();");
            }
            Opcode::Rot => {
                let _ = writeln!(out, "{pad}w_rot();");
            }
            Opcode::Pick(n) => {
                let _ = writeln!(out, "{pad}w_pick_n({n});");
            }

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
                let _ = writeln!(
                    out,
                    "{{ wval _a = w_pop(); w_push((wval)((int64_t)_a == 0 ? 1L : 0L)); }}"
                );
            }

            // ── Control flow ────────────────────────────────────
            Opcode::Cond(offset) => {
                let _ = writeln!(
                    out,
                    "{{ wval _cond = w_pop(); if ((int64_t)_cond == 0) goto L_{}; }}",
                    i as i32 + 1 + offset
                );
            }
            Opcode::Jump(offset) => {
                let _ = writeln!(out, "{pad}goto L_{};", i as i32 + 1 + offset);
            }
            Opcode::Loop(offset) => {
                let _ = writeln!(
                    out,
                    "{{ wval _cond = w_pop(); if ((int64_t)_cond != 0) goto L_{}; }}",
                    i as i32 + 1 + offset
                );
            }
            Opcode::Times => {
                // stack: n, quote -> call the quote n times
                let _ = writeln!(out, "{{ wfn _f = (wfn)w_pop(); int64_t _n = (int64_t)w_pop(); for (int64_t _i = 0; _i < _n; _i++) _f(); }}");
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
                let _ = writeln!(
                    out,
                    "{{ wval *_list = (wval *)w_pop(); w_push((wval)w_list_len(_list)); }}"
                );
            }
            Opcode::Map => {
                let _ = writeln!(out, "{{ wfn _f = (wfn)w_pop(); wval *_l = (wval *)w_pop(); w_push((wval)w_map_over(_l, _f)); }}");
            }
            Opcode::Each => {
                let _ = writeln!(
                    out,
                    "{{ wfn _f = (wfn)w_pop(); wval *_l = (wval *)w_pop(); w_each_over(_l, _f); }}"
                );
            }
            Opcode::Fold => {
                let _ = writeln!(out, "{{ wfn _f = (wfn)w_pop(); wval _init = w_pop(); wval *_l = (wval *)w_pop(); w_push(w_fold_over(_l, _init, _f)); }}");
            }
            Opcode::Filter => {
                let _ = writeln!(out, "{{ wfn _f = (wfn)w_pop(); wval *_l = (wval *)w_pop(); w_push((wval)w_filter_over(_l, _f)); }}");
            }
            Opcode::Sort => {
                let _ = writeln!(
                    out,
                    "{{ wval *_list = (wval *)w_pop(); w_push((wval)w_list_sort(_list)); }}"
                );
            }
            Opcode::Rev => {
                let _ = writeln!(
                    out,
                    "{{ wval *_list = (wval *)w_pop(); w_push((wval)w_list_rev(_list)); }}"
                );
            }
            Opcode::Dedup => {
                let _ = writeln!(
                    out,
                    "{{ wval *_l = (wval *)w_pop(); w_push((wval)w_list_dedup(_l)); }}"
                );
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
                let _ = writeln!(out, "{{ wval _e = w_pop(); wval *_l = (wval *)w_pop(); w_push((wval)w_listfind(_l, _e)); }}");
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
                let _ = writeln!(
                    out,
                    "{{ const char *_s = (const char *)w_pop(); w_push((wval)w_str_to_i64(_s)); }}"
                );
            }
            Opcode::I64ToStr => {
                let _ = writeln!(
                    out,
                    "{{ int64_t _n = (int64_t)w_pop(); w_push((wval)w_i64_to_str(_n)); }}"
                );
            }
            Opcode::StrNth => {
                let _ = writeln!(out, "{{ int64_t _idx = (int64_t)w_pop(); const char *_s = (const char *)w_pop(); w_push((wval)(int64_t)(unsigned char)_s[_idx]); }}");
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
                let _ = writeln!(out, "{{ wval *_list = (wval *)w_pop(); const char *_sep = (const char *)w_pop(); w_push((wval)w_strjoin(_list, _sep)); }}");
            }
            Opcode::StrSplit => {
                let _ = writeln!(out, "{{ const char *_d = (const char *)w_pop(); const char *_s = (const char *)w_pop(); w_push((wval)w_str_split(_s, _d)); }}");
            }
            Opcode::Ctos => {
                let _ = writeln!(
                    out,
                    "{{ int64_t _c = (int64_t)w_pop(); w_push((wval)rt_ctos(_c)); }}"
                );
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
                let _ = writeln!(
                    out,
                    "{{ wval *_map = (wval *)w_pop(); w_push((wval)w_map_keys(_map)); }}"
                );
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
                let _ = writeln!(
                    out,
                    "{{ int64_t _a = (int64_t)w_pop(); w_push((wval)w_bitnot(_a)); }}"
                );
            }

            // ── JSON ────────────────────────────────────────────
            Opcode::JsonParse => {
                // No JSON parser in runtime.c — fail loudly at C compile
                // time instead of silently doing nothing.
                let _ = writeln!(
                    out,
                    "#error \"json-parse is not supported by the C backend (runtime.c has no parser)\""
                );
            }
            Opcode::JsonStringify => {
                let _ = writeln!(
                    out,
                    "{{ wval _v = w_pop(); w_push((wval)w_json_stringify(_v)); }}"
                );
            }

            // ── IO ──────────────────────────────────────────────
            Opcode::OutputTop => {
                let _ = writeln!(out, "{pad}w_print_val(w_pop());");
            }
            Opcode::OutputAll => {
                let _ = writeln!(out, "{pad}w_output_all();");
            }
            Opcode::ReadInput => {
                let _ = writeln!(out, "{pad}w_push((wval)w_read_line());");
            }

            // ── Byte buffer ─────────────────────────────────────
            Opcode::BytesNew | Opcode::BytesPush | Opcode::BytesLen | Opcode::BytesWriteFile => {
                let _ = writeln!(
                    out,
                    "#error \"bytes-* opcodes are not supported by the C backend (no byte-buffer runtime)\""
                );
            }

            // ── Try ─────────────────────────────────────────────
            Opcode::Try => {
                // The C backend has no exceptions: run the quote and push
                // (true, result), matching the VM's success path.
                let _ = writeln!(
                    out,
                    "{{ wfn _f = (wfn)w_pop(); _f(); wval _r = w_pop(); w_push((wval)1); w_push(_r); }}"
                );
            }

            // ── Time/Random ─────────────────────────────────────
            Opcode::Now => {
                let _ = writeln!(out, "{pad}w_push((wval)w_now());");
            }
            Opcode::Sleep => {
                let _ = writeln!(out, "{{ int64_t _ms = (int64_t)w_pop(); w_sleep(_ms); }}");
            }
            Opcode::Random => {
                let _ = writeln!(
                    out,
                    "{{ double _f = w_random(); w_push((wval)*(uint64_t*)&_f); }}"
                );
            }
            Opcode::RandInt => {
                let _ = writeln!(out, "{{ int64_t _max = (int64_t)w_pop(); int64_t _min = (int64_t)w_pop(); w_push((wval)w_randint(_min, _max)); }}");
            }

            // ── Args ────────────────────────────────────────────
            Opcode::Args => {
                let _ = writeln!(out, "{pad}w_push((wval)w_args());");
            }

            // ── Seed-runtime natives ────────────────────────────
            Opcode::Listen => {
                let _ = writeln!(
                    out,
                    "{{ int64_t _p=(int64_t)w_pop(); w_push(w_sock_listen(_p)); }}"
                );
            }
            Opcode::Accept => {
                let _ = writeln!(out, "{{ wval _h=w_pop(); w_push(w_sock_accept(_h)); }}");
            }
            Opcode::Recv => {
                let _ = writeln!(out, "{{ wval _c=w_pop(); w_push(w_sock_recv(_c)); }}");
            }
            Opcode::Send => {
                let _ = writeln!(
                    out,
                    "{{ const char *_s=(const char*)w_pop(); wval _c=w_pop(); w_push(w_sock_send(_c,_s)); }}"
                );
            }
            Opcode::CloseSock => {
                let _ = writeln!(out, "{{ wval _c=w_pop(); w_sock_close(_c); }}");
            }
            Opcode::ReadFile => {
                let _ = writeln!(
                    out,
                    "{{ const char *_p=(const char*)w_pop(); w_push(w_read_file(_p)); }}"
                );
            }
            Opcode::WriteFile => {
                let _ = writeln!(
                    out,
                    "{{ const char *_d=(const char*)w_pop(); const char *_p=(const char*)w_pop(); w_push(w_write_file(_p,_d)); }}"
                );
            }
            Opcode::KvSet => {
                let _ = writeln!(
                    out,
                    "{{ const char *_v=(const char*)w_pop(); const char *_k=(const char*)w_pop(); w_kv_set(_k,_v); }}"
                );
            }
            Opcode::KvGet => {
                let _ = writeln!(
                    out,
                    "{{ const char *_k=(const char*)w_pop(); w_push(w_kv_get(_k)); }}"
                );
            }
            Opcode::KvDel => {
                let _ = writeln!(
                    out,
                    "{{ const char *_k=(const char*)w_pop(); w_kv_del(_k); }}"
                );
            }
            Opcode::KvKeys => {
                let _ = writeln!(out, "w_push((wval)w_kv_keys_list());");
            }
            Opcode::ServeConcurrent => {
                let _ = writeln!(
                    out,
                    "{{ wfn _f=(wfn)w_pop(); int64_t _p=(int64_t)w_pop(); w_serve_concurrent(_p,_f); }}"
                );
            }
            Opcode::BufAlloc => {
                let _ = writeln!(out, "{{ int64_t _n=(int64_t)w_pop(); w_push((wval)(int64_t)calloc((size_t)(_n>0?_n:1),1)); }}");
            }
            Opcode::BufFree => {
                let _ = writeln!(out, "{{ free((void*)(int64_t)w_pop()); }}");
            }
            Opcode::BufPeek => {
                let _ = writeln!(out, "{{ uint64_t _v=0; memcpy(&_v,(const void*)(int64_t)w_pop(),8); w_push((wval)(int64_t)_v); }}");
            }
            Opcode::BufPoke => {
                let _ = writeln!(
                    out,
                    "{{ uint64_t _v=(uint64_t)w_pop(); memcpy((void*)(int64_t)w_pop(),&_v,8); }}"
                );
            }
            Opcode::BufToStr => {
                let _ = writeln!(out, "{{ int64_t _l=(int64_t)w_pop(); const char*_p=(const char*)(int64_t)w_pop(); w_push((wval)w_str_new(_p,(uint32_t)(_l>0?_l:0))); }}");
            }
            Opcode::FfiOpen => {
                let _ = writeln!(out, "{{ w_push((wval)w_ffi_open(w_pop())); }}");
            }
            Opcode::FfiSym => {
                let _ = writeln!(
                    out,
                    "{{ const char*_n=(const char*)w_pop(); int64_t _h=(int64_t)w_pop(); w_push((wval)w_ffi_sym(_h,_n)); }}"
                );
            }
            Opcode::FfiCall => {
                let _ = writeln!(
                    out,
                    "{{ int64_t _f=(int64_t)w_pop(); const char*_fmt=(const char*)w_pop(); wval*_a=(wval*)w_pop(); w_push((wval)w_ffi_call(_f,_fmt,_a)); }}"
                );
            }
            Opcode::Range => {
                let _ = writeln!(
                    out,
                    "{{ int64_t _c=(int64_t)w_pop(); int64_t _s=(int64_t)w_pop(); wval*_l=w_list_new(_c); for(int64_t _i=0;_i<_c;_i++)_l[1+_i]=(wval)(_s+_i); w_push((wval)_l); }}"
                );
            }
            Opcode::Pack => {
                let _ = writeln!(out, "{{ w_push((wval)w_pack(w_pop(),w_pop())); }}");
            }
            Opcode::Unpack => {
                let _ = writeln!(out, "{{ w_push((wval)w_unpack(w_pop(),w_pop())); }}");
            }
            Opcode::StrMatch => {
                let _ = writeln!(
                    out,
                    "{{ const char*_p=(const char*)w_pop(); const char*_s=(const char*)w_pop(); w_push((wval)w_strmatch((wval)_s,(wval)_p)); }}"
                );
            }
            Opcode::StrFindRe => {
                let _ = writeln!(
                    out,
                    "{{ const char*_p=(const char*)w_pop(); const char*_s=(const char*)w_pop(); w_push((wval)w_strfind_re((wval)_s,(wval)_p)); }}"
                );
            }
            Opcode::ReadLine => {
                let _ = writeln!(out, "{{ w_push((wval)w_read_line()); }}");
            }
            Opcode::StrUpper => {
                let _ = writeln!(out, "{{ w_push((wval)w_str_case(w_pop(),1)); }}");
            }
            Opcode::StrLower => {
                let _ = writeln!(out, "{{ w_push((wval)w_str_case(w_pop(),0)); }}");
            }
            Opcode::StrStrip => {
                let _ = writeln!(out, "{{ w_push((wval)w_str_strip(w_pop())); }}");
            }
            Opcode::Fmt => {
                let _ = writeln!(
                    out,
                    "{{ wval _v=w_pop(); wval _f=w_pop(); w_push((wval)w_fmt(_f,_v)); }}"
                );
            }
            Opcode::ListDir => {
                let _ = writeln!(out, "{{ w_push((wval)w_listdir(w_pop())); }}");
            }
            Opcode::Mkdir => {
                let _ = writeln!(out, "{{ w_push((wval)w_mkdir(w_pop())); }}");
            }
            Opcode::FileExists => {
                let _ = writeln!(
                    out,
                    "{{ struct stat _st; w_push((wval)(stat((const char*)w_pop(),&_st)==0)); }}"
                );
            }
            Opcode::StrfTime => {
                let _ = writeln!(
                    out,
                    "{{ const char*_f=(const char*)w_pop(); int64_t _t=(int64_t)w_pop(); w_push((wval)w_strftime_utc(_t,_f)); }}"
                );
            }
            Opcode::Leave(off) => {
                let _ = writeln!(out, "goto L_{};", i + 1 + *off as usize);
            }
            Opcode::EnvGet => {
                let _ = writeln!(
                    out,
                    "{{ const char *_n=(const char*)w_pop(); w_push(w_env_get(_n)); }}"
                );
            }
            Opcode::SplitCond => {
                let _ = writeln!(out, "{pad}{{int64_t _start=(int64_t)w_pop();wval *_toks=(wval*)w_pop();wval *_th=0,*_el=0;int _next=0;split_cond(_toks,(int)_start,&_th,&_el,&_next);wval *_r=w_list_new(3);_r[1]=(wval)_th;_r[2]=(wval)_el;_r[3]=(wval)(int64_t)_next;w_push((wval)_r);}}");
            }
            Opcode::ListCheck => {
                let _ = writeln!(out, "{pad}w_push((wval)(w_is_list_val(w_pop()) ? 1 : 0));");
            }
            Opcode::I64Check => {
                let _ = writeln!(
                    out,
                    "{pad}{{wval _v=w_pop();w_push((wval)(_v < (wval)w_heap || _v >= (wval)(w_heap + HEAP_SIZE) ? 1 : 0));}}"
                );
            }
            Opcode::StrCheck => {
                let _ = writeln!(out, "{pad}w_push((wval)(wval_is_str(w_pop()) ? 1 : 0));");
            }
            Opcode::BoolCheck => {
                let _ = writeln!(
                    out,
                    "{pad}{{wval _v=w_pop();w_push((wval)(_v == 0 || _v == 1 ? 1 : 0));}}"
                );
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
pub fn transpile_to_c(main_bytecode: &[Opcode], defs: &[(String, Vec<Opcode>)]) -> String {
    let mut out = String::new();
    let mut strings = StringPool::new();
    let mut quotes = QuotePool::new();

    // ── Header ──────────────────────────────────────────────────
    let _ = writeln!(out, "// Auto-generated by Whisper C transpiler");
    let _ = writeln!(out, "#include \"runtime.c\"");
    let _ = writeln!(out);

    // ── Collect string and quote constants ──────────────────────
    collect_strings(main_bytecode, &mut strings);
    collect_quotes(main_bytecode, &mut quotes);
    for (_, code) in defs {
        collect_strings(code, &mut strings);
        collect_quotes(code, &mut quotes);
    }

    // ── Emit string constants ───────────────────────────────────
    strings.emit_declarations(&mut out);
    let _ = writeln!(out);

    // ── Forward declarations ────────────────────────────────────
    for (name, _) in defs {
        let cname = safe_c_name(name);
        let _ = writeln!(out, "static void w_{cname}(void);");
    }
    for i in 0..quotes.quotes.len() {
        let _ = writeln!(out, "static void W_q{i}(void);");
    }
    let _ = writeln!(out);

    // ── Word function definitions ───────────────────────────────
    for (name, code) in defs {
        let cname = safe_c_name(name);
        let _ = writeln!(out, "static void w_{cname}(void) {{");
        transpile_opcodes(code, &mut out, &mut strings, &mut quotes, 2);
        let _ = writeln!(out, "}}");
        let _ = writeln!(out);
    }

    // ── Quote function definitions (drain the pool: bodies may
    //    register nested quotes, so loop until stable) ───────────
    let mut emitted = 0;
    while emitted < quotes.quotes.len() {
        let take = quotes.quotes.len();
        for i in emitted..take {
            let _ = writeln!(out, "static void W_q{i}(void) {{");
            let body = std::rc::Rc::clone(&quotes.quotes[i]);
            transpile_opcodes(&body, &mut out, &mut strings, &mut quotes, 2);
            let _ = writeln!(out, "}}");
            let _ = writeln!(out);
        }
        emitted = take;
    }

    // ── main() ──────────────────────────────────────────────────
    let _ = writeln!(out, "int main(int argc, char **argv) {{");
    let _ = writeln!(out, "  w_init(argc, argv);");
    let _ = writeln!(out, "  w_init_strings();");
    transpile_opcodes(main_bytecode, &mut out, &mut strings, &mut quotes, 2);
    let _ = writeln!(out, "  return 0;");
    let _ = writeln!(out, "}}");

    out
}
