/// Stack-based virtual machine for Whisper.
///
/// The VM maintains:
/// - data_stack: main operand stack for values
/// - call_stack: call frames for word/block invocation
/// - word_dict: dictionary of defined words (name -> bytecode)
/// - capability_table: bound IO capabilities
/// - memory: linear memory for complex data structures
use std::collections::HashMap;
use std::rc::Rc;

use crate::capability::CapabilityTable;
use crate::opcode::Opcode;
use crate::value::Value;
use crate::VmError;

/// Simple xorshift64* PRNG for probabilistic choice (`?|`).
/// Returns a random f64 in [0.0, 1.0).
fn xorshift64_next(state: &mut u64) -> f64 {
    *state ^= *state >> 12;
    *state ^= *state << 25;
    *state ^= *state >> 27;
    // Use the upper 53 bits to get a uniformly distributed f64 in [0, 1)
    let bits = state.wrapping_mul(0x2545F4914F6CDD1D) >> 11;
    bits as f64 / (1u64 << 53) as f64
}

/// A call frame representing an active word/block invocation.
#[derive(Debug, Clone)]
pub struct CallFrame {
    /// Name of the word being executed (for debugging)
    pub word_name: Option<String>,
    /// The bytecode being executed
    pub code: Rc<[Opcode]>,
    /// Instruction pointer into the code
    pub ip: usize,
    /// Base pointer for local stack frame
    pub base: usize,
}

/// The Whisper stack-based virtual machine.
pub struct Vm {
    /// Main data stack (operand stack)
    pub data_stack: Vec<Value>,
    /// Call stack for nested word/block invocations
    pub call_stack: Vec<CallFrame>,
    /// Dictionary of user-defined words
    pub word_dict: HashMap<String, Rc<[Opcode]>>,
    /// Capability table (IO capabilities bound at init)
    pub capability_table: CapabilityTable,
    /// Whether to trace execution (debug mode)
    pub trace: bool,
    /// Current quotation-nesting depth (execute_ref), for recursion guard.
    ref_depth: usize,
    /// PRNG state for probabilistic choice (`?|`)
    rng_state: u64,
    /// 4-entry word lookup cache: fixed-size array with round-robin replacement.
    /// Stores Rc<[Opcode]> to avoid clone+allocation on cache hits.
    word_cache: [Option<(String, Rc<[Opcode]>)>; 4],
    word_cache_pos: usize,
    /// Byte buffers for binary output (BytesNew, BytesPush, etc.).
    byte_buffers: Vec<Vec<u8>>,
    /// CLI arguments set before execution (used by the Args opcode).
    pub program_args: Vec<Value>,
    /// TCP listeners/streams behind the native IO handle opcodes
    /// (Listen/Accept/Recv/Send/CloseSock); handles are indices.
    net_listeners: Vec<std::net::TcpListener>,
    net_streams: Vec<std::net::TcpStream>,
    /// Shared key-value store (KvSet/KvGet/KvDel/KvKeys).
    kv: std::cell::RefCell<std::collections::HashMap<String, String>>,
}

fn generate_rng_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0xDEAD_BEEF)
        .wrapping_mul(0x2545F4914F6CDD1D)
}

impl Vm {
    /// Create a new VM with empty stacks and default settings.
    pub fn new() -> Self {
        Vm {
            data_stack: Vec::new(),
            call_stack: Vec::new(),
            word_dict: HashMap::new(),
            capability_table: CapabilityTable::new(),
            trace: false,
            ref_depth: 0,
            rng_state: generate_rng_seed(),
            word_cache: [
                const { None },
                const { None },
                const { None },
                const { None },
            ],
            word_cache_pos: 0,
            byte_buffers: Vec::new(),
            program_args: Vec::new(),
            net_listeners: Vec::new(),
            net_streams: Vec::new(),
            kv: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// Create a VM with a pre-bound capability table.
    pub fn with_capabilities(capability_table: CapabilityTable) -> Self {
        Vm {
            data_stack: Vec::new(),
            call_stack: Vec::new(),
            word_dict: HashMap::new(),
            capability_table,
            trace: false,
            ref_depth: 0,
            rng_state: generate_rng_seed(),
            word_cache: [
                const { None },
                const { None },
                const { None },
                const { None },
            ],
            word_cache_pos: 0,
            byte_buffers: Vec::new(),
            program_args: Vec::new(),
            net_listeners: Vec::new(),
            net_streams: Vec::new(),
            kv: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// Define a word in the dictionary. Invalidate all cache entries.
    pub fn define_word(&mut self, name: String, code: Vec<Opcode>) {
        self.word_cache = [
            const { None },
            const { None },
            const { None },
            const { None },
        ];
        self.word_cache_pos = 0;
        self.word_dict.insert(name, code.into());
    }

    /// Execute a program (sequence of opcodes).
    /// Returns the top value on the stack after execution, or an error.
    pub fn execute(&mut self, program: &[Opcode]) -> Result<Option<Value>, VmError> {
        let code: Rc<[Opcode]> = Rc::from(program.to_vec().into_boxed_slice());
        let frame = CallFrame {
            word_name: Some("<main>".into()),
            code,
            ip: 0,
            base: self.data_stack.len(),
        };
        self.call_stack.push(frame);

        while let Some(frame) = self.call_stack.last() {
            let ip = frame.ip;
            let code_len = frame.code.len();

            if ip >= code_len {
                // Frame finished
                self.call_stack.pop();
                continue;
            }

            // Clone the Rc (cheap refcount bump), not the Opcode (may allocate).
            let code = Rc::clone(&frame.code);
            self.call_stack.last_mut().unwrap().ip = ip + 1;

            if self.trace {
                eprintln!(
                    "[trace] {} | op={:?} | stack={:?}",
                    self.call_stack
                        .last()
                        .and_then(|f| f.word_name.as_deref())
                        .unwrap_or("?"),
                    code[ip].name(),
                    self.data_stack
                );
            }

            self.step(&code[ip])?;
        }

        Ok(self.data_stack.pop())
    }

    /// Execute a single opcode.
    pub fn step(&mut self, op: &Opcode) -> Result<(), VmError> {
        match op {
            // === Stack operations ===
            Opcode::Dup => {
                let v = self.pop()?;
                let cloned = v.clone();
                self.data_stack.push(v);
                self.data_stack.push(cloned);
            }
            Opcode::Swap => {
                let a = self.pop()?;
                let b = self.pop()?;
                self.data_stack.push(a);
                self.data_stack.push(b);
            }
            Opcode::Drop => {
                self.pop()?;
            }
            Opcode::Rot => {
                let a = self.pop()?; // top
                let b = self.pop()?; // second
                let c = self.pop()?; // third
                self.data_stack.push(b);
                self.data_stack.push(a);
                self.data_stack.push(c);
            }
            Opcode::Pick(n) => {
                let idx = self.data_stack.len().checked_sub(1 + *n as usize).ok_or(
                    VmError::StackUnderflow {
                        expected: *n as usize + 1,
                        actual: self.data_stack.len(),
                    },
                )?;
                let v = self.data_stack[idx].clone();
                self.data_stack.push(v);
            }

            // === New operations ===
            Opcode::Filter => {
                let quot = self.pop_ref()?;
                let list = self.pop_list()?;
                let mut results = Vec::with_capacity(list.len());
                let parent = std::mem::take(&mut self.data_stack);
                for item in list.iter() {
                    // Predicate sees only the item
                    self.data_stack.clear();
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                    let keep = self.pop_bool()?;
                    if keep {
                        results.push(item.clone());
                    }
                }
                self.data_stack = parent;
                self.data_stack.push(Value::List(Rc::new(results)));
            }
            Opcode::StrSplit => {
                let delim = self.pop_str()?;
                let s = self.pop_str()?;
                let parts: Vec<Value> = if delim.is_empty() {
                    s.chars()
                        .map(|c| Value::Str(Rc::new(c.to_string())))
                        .collect()
                } else {
                    s.split(delim.as_ref())
                        .map(|part| Value::Str(Rc::new(part.to_string())))
                        .collect()
                };
                self.data_stack.push(Value::List(Rc::new(parts)));
            }
            Opcode::Ctos => {
                // Raw byte semantics (matches runtime.c's rt_ctos): the code
                // is a byte, not a codepoint — %XX escapes must concatenate
                // as bytes so multibyte UTF-8 survives url-decoding.
                // from_utf8_unchecked: intermediate states may hold a partial
                // UTF-8 sequence; the completed sequence is valid UTF-8.
                let code = self.pop_i64()?;
                let byte = (code & 0xFF) as u8;
                let s = unsafe { String::from_utf8_unchecked(vec![byte]) };
                self.data_stack.push(Value::Str(Rc::new(s)));
            }
            Opcode::ListSlice => {
                let len = self.pop_i64()?;
                let start = self.pop_i64()?;
                let list = self.pop_list()?;
                let start = start.max(0) as usize;
                let len = len.max(0) as usize;
                let end = (start + len).min(list.len());
                let start = start.min(list.len());
                let sub = list[start..end].to_vec();
                self.data_stack.push(Value::List(Rc::new(sub)));
            }
            Opcode::Shl => {
                let shift = self.pop_i64()?;
                let val = self.pop_i64()?;
                self.data_stack
                    .push(Value::I64(val.wrapping_shl(shift as u32)));
            }
            Opcode::Shr => {
                let shift = self.pop_i64()?;
                let val = self.pop_i64()?;
                self.data_stack
                    .push(Value::I64(val.wrapping_shr(shift as u32)));
            }
            Opcode::Args => {
                let args = self.program_args.clone();
                self.data_stack.push(Value::List(Rc::new(args)));
            }
            // Split `?? then | else ]` tokens at depth 1 (seed-compatible):
            // tokens start → [then, else, next-index-past-closing-]
            Opcode::SplitCond => {
                let start = self.pop_i64()?.max(0) as usize;
                let tokens = self.pop_list()?;
                let mut then_branch: Vec<Value> = Vec::new();
                let mut else_branch: Vec<Value> = Vec::new();
                let mut depth: i64 = 1;
                let mut in_else = false;
                let mut i = start;
                while i < tokens.len() && depth > 0 {
                    let tok = tokens[i].clone();
                    let s = match &tok {
                        Value::Str(s) => Some(s.as_str()),
                        _ => None,
                    };
                    let dst = if in_else {
                        &mut else_branch
                    } else {
                        &mut then_branch
                    };
                    match s {
                        Some("??") => {
                            depth += 1;
                            dst.push(tok);
                        }
                        Some("]") => {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                            dst.push(tok);
                        }
                        Some("|") if depth == 1 => in_else = true,
                        _ => dst.push(tok),
                    }
                    i += 1;
                }
                let next = (i + 1) as i64;
                self.data_stack.push(Value::List(Rc::new(vec![
                    Value::List(Rc::new(then_branch)),
                    Value::List(Rc::new(else_branch)),
                    Value::I64(next),
                ])));
            }
            Opcode::ListCheck => {
                let v = self.data_stack.pop().ok_or(VmError::StackUnderflow {
                    expected: 1,
                    actual: self.data_stack.len(),
                })?;
                let is_list = matches!(v, Value::List(_));
                self.data_stack.push(Value::Bool(is_list));
            }
            Opcode::I64Check => {
                let v = self.data_stack.pop().ok_or(VmError::StackUnderflow {
                    expected: 1,
                    actual: self.data_stack.len(),
                })?;
                let is_int = matches!(v, Value::I64(_));
                self.data_stack.push(Value::Bool(is_int));
            }
            Opcode::StrCheck => {
                let v = self.data_stack.pop().ok_or(VmError::StackUnderflow {
                    expected: 1,
                    actual: self.data_stack.len(),
                })?;
                let is_str = matches!(v, Value::Str(_));
                self.data_stack.push(Value::Bool(is_str));
            }
            Opcode::BoolCheck => {
                let v = self.data_stack.pop().ok_or(VmError::StackUnderflow {
                    expected: 1,
                    actual: self.data_stack.len(),
                })?;
                let is_bool = matches!(v, Value::Bool(_));
                self.data_stack.push(Value::Bool(is_bool));
            }

            // === Native IO (v1.0.1 vocabulary) — mirrors runtime.c ===
            Opcode::Listen => {
                let port = self.pop_i64()?;
                let listener = std::net::TcpListener::bind(("127.0.0.1", port as u16));
                match listener {
                    Ok(l) => {
                        self.net_listeners.push(l);
                        self.data_stack
                            .push(Value::I64((self.net_listeners.len() - 1) as i64));
                    }
                    Err(_) => self.data_stack.push(Value::I64(-1)),
                }
            }
            Opcode::Accept => {
                let idx = self.pop_i64()?;
                let conn = self
                    .net_listeners
                    .get_mut(idx as usize)
                    .and_then(|l| l.accept().ok())
                    .map(|(s, _)| s);
                match conn {
                    Some(s) => {
                        self.net_streams.push(s);
                        self.data_stack
                            .push(Value::I64((self.net_streams.len() - 1) as i64));
                    }
                    None => self.data_stack.push(Value::I64(-1)),
                }
            }
            Opcode::Recv => {
                let idx = self.pop_i64()?;
                let mut buf = Vec::new();
                let ok = self
                    .net_streams
                    .get_mut(idx as usize)
                    .map(|s| {
                        use std::io::Read;
                        let _ = s.set_read_timeout(Some(std::time::Duration::from_secs(2)));
                        let mut chunk = [0u8; 4096];
                        loop {
                            if buf.len() >= 1_048_576 {
                                break;
                            }
                            if buf.ends_with(b"\r\n\r\n") {
                                // Read the Content-Length body if declared
                                // (mirrors runtime.c's w_hdr_content_length).
                                let headers = &buf[..buf.len()];
                                let clen = headers
                                    .windows(15)
                                    .position(|w| w.eq_ignore_ascii_case(b"content-length:"))
                                    .and_then(|i| {
                                        let rest = &headers[i + 15..];
                                        let end = rest
                                            .iter()
                                            .position(|b| !b.is_ascii_digit())
                                            .unwrap_or(rest.len());
                                        std::str::from_utf8(&rest[..end])
                                            .ok()
                                            .and_then(|n| n.parse::<usize>().ok())
                                    })
                                    .unwrap_or(0)
                                    .min(1_048_576);
                                let need = buf.len() + clen;
                                while buf.len() < need {
                                    match s.read(&mut chunk) {
                                        Ok(0) | Err(_) => break,
                                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                                    }
                                }
                                break;
                            }
                            match s.read(&mut chunk) {
                                Ok(0) | Err(_) => break,
                                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                            }
                        }
                        true
                    })
                    .unwrap_or(false);
                let _ = ok;
                let s = String::from_utf8_lossy(&buf).to_string();
                self.data_stack.push(Value::Str(Rc::new(s)));
            }
            Opcode::Send => {
                let data = self.pop_str()?;
                let idx = self.pop_i64()?;
                let n = self
                    .net_streams
                    .get_mut(idx as usize)
                    .and_then(|s| {
                        use std::io::Write;
                        s.write_all(data.as_bytes()).ok()?;
                        Some(data.len() as i64)
                    })
                    .unwrap_or(-1);
                self.data_stack.push(Value::I64(n));
            }
            Opcode::CloseSock => {
                let idx = self.pop_i64()?;
                if let Some(s) = self.net_streams.get_mut(idx as usize) {
                    let _ = s.shutdown(std::net::Shutdown::Both);
                }
            }
            Opcode::ReadFile => {
                // byte semantics (consistent with strlen/Ctos): binary
                // files survive read-file; UTF-8 validation is NOT applied
                let path = self.pop_str()?;
                let content = std::fs::read(path.as_ref())
                    .map(|b| unsafe { String::from_utf8_unchecked(b) })
                    .unwrap_or_default();
                self.data_stack.push(Value::Str(Rc::new(content)));
            }
            Opcode::WriteFile => {
                let data = self.pop_str()?;
                let path = self.pop_str()?;
                let n = std::fs::write(path.as_ref(), data.as_bytes())
                    .map(|_| data.len() as i64)
                    .unwrap_or(-1);
                self.data_stack.push(Value::I64(n));
            }
            Opcode::EnvGet => {
                let name = self.pop_str()?;
                let v = std::env::var(name.as_ref()).unwrap_or_default();
                self.data_stack.push(Value::Str(Rc::new(v)));
            }

            // === Shared KV + concurrent serve (v1.0.2) ===
            Opcode::KvSet => {
                let val = self.pop_str()?;
                let key = self.pop_str()?;
                self.kv
                    .borrow_mut()
                    .insert(key.as_ref().to_string(), val.as_ref().to_string());
            }
            Opcode::KvGet => {
                let key = self.pop_str()?;
                let v = self
                    .kv
                    .borrow()
                    .get(key.as_ref())
                    .cloned()
                    .unwrap_or_default();
                self.data_stack.push(Value::Str(Rc::new(v)));
            }
            Opcode::KvDel => {
                let key = self.pop_str()?;
                self.kv.borrow_mut().remove(key.as_ref());
            }
            Opcode::KvKeys => {
                let keys: Vec<Value> = self
                    .kv
                    .borrow()
                    .keys()
                    .map(|k| Value::Str(Rc::new(k.clone())))
                    .collect();
                self.data_stack.push(Value::List(Rc::new(keys)));
            }
            Opcode::BufAlloc => {
                let n = self.pop_i64()?.max(0) as usize;
                let buf = vec![0u8; n].into_boxed_slice();
                let ptr = Box::into_raw(buf) as *mut u8 as i64;
                self.data_stack.push(Value::I64(ptr));
            }
            Opcode::BufFree => {
                let ptr = self.pop_i64()? as *mut u8;
                if !ptr.is_null() {
                    unsafe { drop(Vec::from_raw_parts(ptr, 0, 0)) };
                }
            }
            Opcode::BufPeek => {
                let ptr = self.pop_i64()? as *const u8;
                let mut v = [0u8; 8];
                unsafe { std::ptr::copy_nonoverlapping(ptr, v.as_mut_ptr(), 8) };
                self.data_stack
                    .push(Value::I64(u64::from_le_bytes(v) as i64));
            }
            Opcode::BufPoke => {
                let val = self.pop_i64()? as u64;
                let ptr = self.pop_i64()? as *mut u8;
                unsafe { std::ptr::copy_nonoverlapping(val.to_le_bytes().as_ptr(), ptr, 8) };
            }
            Opcode::BufToStr => {
                let len = self.pop_i64()?.max(0) as usize;
                let ptr = self.pop_i64()? as *const u8;
                let mut v = vec![0u8; len];
                unsafe { std::ptr::copy_nonoverlapping(ptr, v.as_mut_ptr(), len) };
                // byte semantics: binary payload survives as raw bytes
                let s = unsafe { String::from_utf8_unchecked(v) };
                self.data_stack.push(Value::Str(Rc::new(s)));
            }
            Opcode::FfiOpen => {
                let path = self.pop_str()?;
                let h = crate::ffi::open(path.as_ref());
                self.data_stack.push(Value::I64(h));
            }
            Opcode::FfiSym => {
                let name = self.pop_str()?;
                let handle = self.pop_i64()?;
                let s = crate::ffi::sym(handle, name.as_ref());
                self.data_stack.push(Value::I64(s));
            }
            Opcode::FfiCall => {
                let fnptr = self.pop_i64()?;
                let fmt = self.pop_str()?;
                let args_val = self.pop()?;
                // keep the args list alive for the whole call: `p` args
                // borrow string bytes owned by it
                let args_list = match &args_val {
                    Value::List(l) => Rc::clone(l),
                    _ => {
                        return Err(VmError::TypeMismatch {
                            expected: "[T]".into(),
                            actual: args_val.type_name().into(),
                        })
                    }
                };
                // map by FMT char: 'p' accepts raw pointers (i64) and
                // strings (byte-data pointer) alike — buffers come in
                // as buf-alloc integer addresses
                let fbytes = fmt.as_bytes();
                let fargs: Vec<crate::ffi::FArg> = args_list
                    .iter()
                    .enumerate()
                    .map(|(i, v)| match (fbytes.get(i + 1), v) {
                        (Some(b'p'), Value::Str(s)) => Ok(crate::ffi::FArg::P(s.as_ptr() as usize)),
                        (Some(b'p'), Value::I64(n)) => Ok(crate::ffi::FArg::P(*n as usize)),
                        (_, Value::I64(n)) => Ok(crate::ffi::FArg::I(*n)),
                        (_, Value::F64(x)) => Ok(crate::ffi::FArg::D(*x)),
                        (Some(b'p'), other) => Err(VmError::TypeMismatch {
                            expected: "str|i64 ptr".into(),
                            actual: other.type_name().into(),
                        }),
                        (_, other) => Err(VmError::TypeMismatch {
                            expected: "i64|f64".into(),
                            actual: other.type_name().into(),
                        }),
                    })
                    .collect::<Result<_, _>>()?;
                let r =
                    crate::ffi::call(fnptr, fmt.as_ref(), &fargs).map_err(VmError::ProgramError)?;
                self.data_stack.push(Value::I64(r));
            }
            Opcode::Range => {
                let count = self.pop_i64()?;
                let start = self.pop_i64()?;
                let v: Vec<Value> = if count > 0 {
                    (start..start + count).map(Value::I64).collect()
                } else {
                    Vec::new()
                };
                self.data_stack.push(Value::List(Rc::new(v)));
            }
            Opcode::Pack => {
                let fmt = self.pop_str()?;
                let vals = self.pop_list()?;
                let packed = crate::structpack::pack(&vals, fmt.as_bytes()).unwrap_or_default();
                // bytes are a byte-semantic string
                let s = unsafe { String::from_utf8_unchecked(packed) };
                self.data_stack.push(Value::Str(std::rc::Rc::new(s)));
            }
            Opcode::Unpack => {
                let fmt = self.pop_str()?;
                let bytes = self.pop_str()?;
                let vals =
                    crate::structpack::unpack(bytes.as_bytes(), fmt.as_bytes()).unwrap_or_default();
                self.data_stack.push(Value::List(std::rc::Rc::new(vals)));
            }
            Opcode::StrMatch => {
                let pat = self.pop_str()?;
                let s = self.pop_str()?;
                let r = crate::rematch::str_match(s.as_bytes(), pat.as_bytes());
                self.data_stack.push(Value::Bool(r));
            }
            Opcode::StrFindRe => {
                let pat = self.pop_str()?;
                let s = self.pop_str()?;
                let r = crate::rematch::str_find(s.as_bytes(), pat.as_bytes());
                self.data_stack.push(Value::I64(r));
            }
            Opcode::ListDir => {
                let path = self.pop_str()?;
                let mut names: Vec<Value> = Vec::new();
                if let Ok(rd) = std::fs::read_dir(path.as_ref()) {
                    for e in rd.flatten() {
                        let n = e.file_name().to_string_lossy().to_string();
                        names.push(Value::Str(Rc::new(n)));
                    }
                }
                names.sort_by_key(|v| match v {
                    Value::Str(s) => s.as_ref().clone(),
                    other => format!("{other:?}"),
                });
                self.data_stack.push(Value::List(Rc::new(names)));
            }
            Opcode::Mkdir => {
                let path = self.pop_str()?;
                let r = std::fs::create_dir(path.as_ref())
                    .map(|_| 0i64)
                    .unwrap_or(-1);
                self.data_stack.push(Value::I64(r));
            }
            Opcode::FileExists => {
                let path = self.pop_str()?;
                let r = std::fs::metadata(path.as_ref()).is_ok();
                self.data_stack.push(Value::Bool(r));
            }
            Opcode::StrfTime => {
                let fmt = self.pop_str()?;
                let ts = self.pop_i64()?;
                let out = crate::strftime::format_utc(ts, fmt.as_ref());
                self.data_stack.push(Value::Str(Rc::new(out)));
            }
            Opcode::Leave(offset) => {
                // same jump semantics as Jump; compile_loop backpatches
                // the offset to the instruction after the Loop
                let frame = self.call_stack.last_mut().unwrap();
                frame.ip = (frame.ip as i64 + *offset as i64) as usize;
            }
            Opcode::ReadLine => {
                let mut line = String::new();
                let n = std::io::stdin().read_line(&mut line).unwrap_or(0);
                let s = if n == 0 {
                    String::new()
                } else {
                    line.trim_end_matches(['\n', '\r']).to_string()
                };
                self.data_stack.push(Value::Str(Rc::new(s)));
            }
            Opcode::StrUpper => {
                let s = self.pop_str()?;
                let v: Vec<u8> = s.as_ref().bytes().map(|b| b.to_ascii_uppercase()).collect();
                let out = String::from_utf8(v).unwrap_or_default();
                self.data_stack.push(Value::Str(Rc::new(out)));
            }
            Opcode::StrLower => {
                let s = self.pop_str()?;
                let v: Vec<u8> = s.as_ref().bytes().map(|b| b.to_ascii_lowercase()).collect();
                let out = String::from_utf8(v).unwrap_or_default();
                self.data_stack.push(Value::Str(Rc::new(out)));
            }
            Opcode::StrStrip => {
                let s = self.pop_str()?;
                let v = s.as_ref().as_bytes();
                let ws = |b: &u8| matches!(b, b' ' | b'\t' | b'\r' | b'\n');
                let start = v.iter().position(|b| !ws(b)).unwrap_or(v.len());
                let end = v.iter().rposition(|b| !ws(b)).map_or(start, |i| i + 1);
                let out = String::from_utf8(v[start..end].to_vec()).unwrap_or_default();
                self.data_stack.push(Value::Str(Rc::new(out)));
            }
            Opcode::Fmt => {
                let value = self.data_stack.pop().ok_or(VmError::StackUnderflow {
                    expected: 1,
                    actual: self.data_stack.len(),
                })?;
                let fstr = self.pop_str()?;
                let vs = match &value {
                    Value::I64(n) => n.to_string(),
                    Value::F64(f) => f.to_string(),
                    Value::Bool(b) => (if *b { "#t" } else { "#f" }).to_string(),
                    Value::Str(s) => s.as_ref().clone(),
                    other => format!("{other:?}"),
                };
                let out = fstr.as_ref().replacen("{}", &vs, 1);
                self.data_stack.push(Value::Str(Rc::new(out)));
            }
            Opcode::ServeConcurrent => {
                // The VM is single-threaded (Rc values): serve serially —
                // name-based (C chain emits a w_find_word registry);
                // handler receives the conn handle and owns its lifecycle.
                let name = self.pop_str()?;
                let port = self.pop_i64()?;
                let listener = std::net::TcpListener::bind(("127.0.0.1", port as u16))
                    .map_err(|e| VmError::IoError(e.to_string()))?;
                self.net_listeners.push(listener);
                let lidx = self.net_listeners.len() - 1;
                let handler = self
                    .word_dict
                    .get(name.as_ref())
                    .cloned()
                    .ok_or_else(|| VmError::UndefinedWord(name.as_ref().to_string()))?;
                let call = vec![Opcode::Call(name.as_ref().to_string())];
                let _ = handler;
                loop {
                    let conn = self
                        .net_listeners
                        .get_mut(lidx)
                        .and_then(|l| l.accept().ok())
                        .map(|(s, _)| s);
                    match conn {
                        Some(s) => {
                            self.net_streams.push(s);
                            let cidx = self.net_streams.len() - 1;
                            self.data_stack.push(Value::I64(cidx as i64));
                            self.execute(&call)?;
                        }
                        None => break,
                    }
                }
            }

            // === Arithmetic ===
            Opcode::Add => {
                self.binary_num_op(|a, b| Ok(Value::I64(a + b)), |a, b| Ok(Value::F64(a + b)))?
            }
            Opcode::Sub => {
                self.binary_num_op(|a, b| Ok(Value::I64(a - b)), |a, b| Ok(Value::F64(a - b)))?
            }
            Opcode::Mul => {
                self.binary_num_op(|a, b| Ok(Value::I64(a * b)), |a, b| Ok(Value::F64(a * b)))?
            }
            Opcode::Div => self.binary_num_op(
                |a, b| {
                    if b == 0 {
                        Err(VmError::DivisionByZero)
                    } else {
                        Ok(Value::I64(a / b))
                    }
                },
                |a, b| {
                    if b == 0.0 {
                        Err(VmError::DivisionByZero)
                    } else {
                        Ok(Value::F64(a / b))
                    }
                },
            )?,
            Opcode::Mod => self.binary_num_op(
                |a, b| {
                    if b == 0 {
                        Err(VmError::DivisionByZero)
                    } else {
                        Ok(Value::I64(a % b))
                    }
                },
                |_a, _b| {
                    Err(VmError::TypeMismatch {
                        expected: "i64".into(),
                        actual: "f64".into(),
                    })
                },
            )?,

            // === Comparison ===
            Opcode::Eq => {
                let a = self.pop()?;
                let b = self.pop()?;
                let result = a.equals(&b);
                self.data_stack.push(Value::Bool(result));
            }
            Opcode::Lt => self.compare_op(|a, b| a < b)?,
            Opcode::Gt => self.compare_op(|a, b| a > b)?,
            Opcode::Neq => {
                let a = self.pop()?;
                let b = self.pop()?;
                self.data_stack.push(Value::Bool(!a.equals(&b)));
            }
            Opcode::Le => self.compare_op(|a, b| a <= b)?,
            Opcode::Ge => self.compare_op(|a, b| a >= b)?,

            // === Logic ===
            Opcode::And => {
                let a = self.pop_bool()?;
                let b = self.pop_bool()?;
                self.data_stack.push(Value::Bool(a && b));
            }
            Opcode::Or => {
                let a = self.pop_bool()?;
                let b = self.pop_bool()?;
                self.data_stack.push(Value::Bool(a || b));
            }
            Opcode::Not => {
                let a = self.pop_bool()?;
                self.data_stack.push(Value::Bool(!a));
            }

            // === Literals ===
            Opcode::PushI64(n) => self.data_stack.push(Value::I64(*n)),
            Opcode::PushF64(n) => self.data_stack.push(Value::F64(*n)),
            Opcode::PushStr(s) => {
                // Rc<String> allocated once at compile time — just bump refcount.
                self.data_stack.push(Value::Str(Rc::clone(s)));
            }
            Opcode::PushBool(b) => self.data_stack.push(Value::Bool(*b)),
            Opcode::PushList => {
                // Pop count from stack, then pop that many elements
                let count = self.pop_i64()?;
                if count < 0 {
                    return Err(VmError::ProgramError(format!(
                        "PushList: negative count {count}"
                    )));
                }
                let count = count as usize;
                // Reasonable upper bound to prevent memory exhaustion
                const MAX_LIST_SIZE: usize = 1_048_576; // 2^20
                if count > MAX_LIST_SIZE {
                    return Err(VmError::ProgramError(format!(
                        "PushList: count {count} exceeds max list size {MAX_LIST_SIZE}"
                    )));
                }
                let mut items = Vec::with_capacity(count);
                for _ in 0..count {
                    items.push(self.pop()?);
                }
                items.reverse(); // elements were pushed in order
                self.data_stack.push(Value::List(Rc::new(items)));
            }
            Opcode::PushRef(ref code) => {
                self.data_stack
                    .push(Value::Ref(Rc::from(code.clone().into_boxed_slice())));
            }

            // === List operations ===
            Opcode::Nth => {
                let n = self.pop_i64()?;
                let list = self.pop_list()?;
                if n < 0 {
                    return Err(VmError::NegativeValue(n));
                }
                let n = n as usize;
                if n >= list.len() {
                    return Err(VmError::IndexOutOfBounds {
                        index: n as i64,
                        length: list.len(),
                    });
                }
                self.data_stack.push(list[n].clone());
            }
            Opcode::Append => {
                let elem = self.pop()?;
                let list = self.pop_list()?;
                let mut new_list = (*list).clone();
                new_list.push(elem);
                self.data_stack.push(Value::List(Rc::new(new_list)));
            }
            Opcode::Len => {
                let list = self.pop_list()?;
                self.data_stack.push(Value::I64(list.len() as i64));
            }
            Opcode::Map => {
                let quot = self.pop_ref()?;
                let list = self.pop_list()?;
                let mut results = Vec::with_capacity(list.len());
                // Stack isolation: park the parent stack (O(1) move), give the
                // body a fresh frame per element (clear keeps capacity).
                let parent = std::mem::take(&mut self.data_stack);
                for item in list.iter() {
                    self.data_stack.clear();
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                    if let Some(result) = self.data_stack.pop() {
                        results.push(result);
                    }
                }
                self.data_stack = parent;
                self.data_stack.push(Value::List(Rc::new(results)));
            }
            Opcode::Each => {
                let quot = self.pop_ref()?;
                let list = self.pop_list()?;
                let parent = std::mem::take(&mut self.data_stack);
                for item in list.iter() {
                    self.data_stack.clear();
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                }
                self.data_stack = parent;
            }
            Opcode::Fold => {
                let quot = self.pop_ref()?;
                let init = self.pop()?;
                let list = self.pop_list()?;
                let mut acc = init;
                let parent = std::mem::take(&mut self.data_stack);
                for item in list.iter() {
                    // Body sees only acc and item
                    self.data_stack.clear();
                    self.data_stack.push(acc);
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                    acc = self.pop()?;
                }
                self.data_stack = parent;
                self.data_stack.push(acc);
            }
            Opcode::Prepend => {
                let list = self.pop_list()?;
                let elem = self.pop()?;
                let mut new_vec = Vec::with_capacity(list.len() + 1);
                new_vec.push(elem);
                new_vec.extend(list.iter().cloned());
                self.data_stack.push(Value::List(Rc::new(new_vec)));
            }

            // === String operations ===
            Opcode::StrLen => {
                let s = self.pop_str()?;
                self.data_stack.push(Value::I64(s.len() as i64));
            }
            Opcode::StrCat => {
                let s2 = self.pop_str()?;
                let s1 = self.pop_str()?;
                let result = format!("{s1}{s2}");
                self.data_stack.push(Value::Str(Rc::new(result)));
            }
            Opcode::StrSlice => {
                let len = self.pop_i64()?;
                let start = self.pop_i64()?;
                let s = self.pop_str()?;
                if start < 0 {
                    return Err(VmError::NegativeValue(start));
                }
                let start = start as usize;
                let len = len.max(0) as usize;
                let end = (start + len).min(s.len());
                let start = start.min(s.len());
                // byte semantics (§3.5): slices index bytes; binary data
                // (PNG headers etc.) slices cleanly. Char-boundary errors
                // rejected binary files entirely before this change.
                let substr: String =
                    unsafe { String::from_utf8_unchecked(s.as_bytes()[start..end].to_vec()) };
                self.data_stack.push(Value::Str(Rc::new(substr)));
            }
            Opcode::StrEq => {
                let s2 = self.pop_str()?;
                let s1 = self.pop_str()?;
                self.data_stack
                    .push(Value::Bool(s1.as_ref() == s2.as_ref()));
            }
            Opcode::StrLt => {
                let s2 = self.pop_str()?;
                let s1 = self.pop_str()?;
                self.data_stack.push(Value::Bool(s1.as_ref() < s2.as_ref()));
            }
            Opcode::StrFind => {
                let pat = self.pop_str()?;
                let s = self.pop_str()?;
                let idx = s.find(pat.as_ref()).map(|i| i as i64).unwrap_or(-1);
                self.data_stack.push(Value::I64(idx));
            }
            Opcode::StrReplace => {
                let new = self.pop_str()?;
                let old = self.pop_str()?;
                let s = self.pop_str()?;
                let result = s.replace(old.as_ref(), new.as_ref());
                self.data_stack.push(Value::Str(Rc::new(result)));
            }
            Opcode::StrToI64 => {
                let s = self.pop_str()?;
                let n = s
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| VmError::ProgramError(format!("Cannot parse '{}' as i64", s)))?;
                self.data_stack.push(Value::I64(n));
            }
            Opcode::I64ToStr => {
                let n = self.pop_i64()?;
                self.data_stack.push(Value::Str(Rc::new(n.to_string())));
            }
            Opcode::StrNth => {
                // byte semantics (§3.5): index bytes, return the byte code;
                // out of range → -1. chars().nth() panicked on binary data
                // (non-char-boundary) — bytes survive every str op now.
                let idx = self.pop_i64()?;
                let s = self.pop_str()?;
                let b = s.as_bytes().get(idx as usize).copied().unwrap_or(u8::MAX);
                self.data_stack
                    .push(Value::I64(if idx < 0 || idx as usize >= s.len() {
                        -1
                    } else {
                        b as i64
                    }));
            }
            Opcode::StrChars => {
                let s = self.pop_str()?;
                let chars: Vec<Value> = s.chars().map(|c| Value::I64(c as i64)).collect();
                self.data_stack.push(Value::List(Rc::new(chars)));
            }
            Opcode::CharsStr => {
                let list = self.pop_list()?;
                let s: String = list
                    .iter()
                    .filter_map(|v| match v.unwrap_signal_ref() {
                        Value::I64(n) => std::char::from_u32(*n as u32),
                        _ => None,
                    })
                    .collect();
                self.data_stack.push(Value::Str(Rc::new(s)));
            }
            Opcode::StrIter => {
                let s = self.pop_str()?;
                if s.is_empty() {
                    self.data_stack.push(Value::I64(-1));
                    self.data_stack.push(Value::Str(Rc::new(String::new())));
                } else {
                    let mut chars = s.chars();
                    let first = chars.next().unwrap() as i64;
                    let rest: String = chars.collect();
                    self.data_stack.push(Value::I64(first));
                    self.data_stack.push(Value::Str(Rc::new(rest)));
                }
            }
            Opcode::ListFind => {
                let key = self.pop()?;
                let list = self.pop_list()?;
                let mut found = false;
                let mut value = Value::I64(0);
                for item in list.iter() {
                    if let Value::List(pair) = item {
                        if pair.len() == 2 && pair[0].equals(&key) {
                            found = true;
                            value = pair[1].clone();
                            break;
                        }
                    }
                }
                self.data_stack.push(Value::Bool(found));
                self.data_stack.push(value);
            }
            Opcode::StrJoin => {
                let list = self.pop_list()?;
                let sep = self.pop_str()?;
                let parts: Vec<String> = list
                    .iter()
                    .filter_map(|v| match v.unwrap_signal_ref() {
                        Value::Str(s) => Some(s.as_ref().clone()),
                        _ => None,
                    })
                    .collect();
                self.data_stack
                    .push(Value::Str(Rc::new(parts.join(sep.as_ref()))));
            }

            // === Binary output ===
            Opcode::BytesNew => {
                self.byte_buffers.push(Vec::new());
                let handle = (self.byte_buffers.len() - 1) as i64;
                self.data_stack.push(Value::I64(handle));
            }
            Opcode::BytesPush => {
                let byte = self.pop_i64()? as u8;
                let handle = self.pop_i64()? as usize;
                if handle >= self.byte_buffers.len() {
                    return Err(VmError::ProgramError(format!(
                        "BytesPush: invalid handle {handle}"
                    )));
                }
                self.byte_buffers[handle].push(byte);
                self.data_stack.push(Value::I64(handle as i64));
            }
            Opcode::BytesLen => {
                let handle = self.pop_i64()? as usize;
                if handle >= self.byte_buffers.len() {
                    return Err(VmError::ProgramError(format!(
                        "BytesLen: invalid handle {handle}"
                    )));
                }
                let len = self.byte_buffers[handle].len() as i64;
                self.data_stack.push(Value::I64(len));
            }
            Opcode::BytesWriteFile => {
                let filename = self.pop_str()?;
                let handle = self.pop_i64()? as usize;
                if handle >= self.byte_buffers.len() {
                    return Err(VmError::ProgramError(format!(
                        "BytesWriteFile: invalid handle {handle}"
                    )));
                }
                let data = std::mem::take(&mut self.byte_buffers[handle]);
                std::fs::write(filename.as_ref(), &data)
                    .map_err(|e| VmError::IoError(e.to_string()))?;
            }
            Opcode::Try => {
                let quot = self.pop_ref()?;
                match self.execute_ref(&quot) {
                    Ok(()) => {
                        let val = self.data_stack.pop().unwrap_or(Value::I64(0));
                        self.data_stack.push(Value::Bool(true));
                        self.data_stack.push(val);
                    }
                    Err(e) => {
                        self.data_stack.push(Value::Bool(false));
                        self.data_stack.push(Value::Str(Rc::new(e.to_string())));
                    }
                }
            }

            // === Map operations ===
            Opcode::MapNew => {
                let map = HashMap::new();
                self.data_stack.push(Value::Map(Rc::new(map)));
            }
            Opcode::MapGet => {
                let key = self.pop_str()?;
                let map = self.pop_map()?;
                let val = map.get(key.as_ref()).cloned().unwrap_or(Value::I64(0));
                self.data_stack.push(val);
            }
            Opcode::MapSet => {
                let val = self.pop()?;
                let key = self.pop_str()?;
                let map = self.pop_map()?;
                let mut new_map = (*map).clone();
                new_map.insert(key.as_ref().clone(), val);
                self.data_stack.push(Value::Map(Rc::new(new_map)));
            }
            Opcode::MapDel => {
                let key = self.pop_str()?;
                let map = self.pop_map()?;
                let mut new_map = (*map).clone();
                new_map.remove(key.as_ref());
                self.data_stack.push(Value::Map(Rc::new(new_map)));
            }
            Opcode::MapKeys => {
                let map = self.pop_map()?;
                let keys: Vec<Value> = map.keys().map(|k| Value::Str(Rc::new(k.clone()))).collect();
                self.data_stack.push(Value::List(Rc::new(keys)));
            }
            Opcode::MapHas => {
                let key = self.pop_str()?;
                let map = self.pop_map()?;
                let has = map.contains_key(key.as_ref());
                self.data_stack.push(Value::Bool(has));
            }

            // === Float operations ===
            Opcode::I64ToF64 => {
                let n = self.pop_i64()?;
                self.data_stack.push(Value::F64(n as f64));
            }
            Opcode::F64ToI64 => {
                let f = self.pop_f64()?;
                self.data_stack.push(Value::I64(f as i64));
            }
            Opcode::FSqrt => {
                let f = self.pop_f64()?;
                self.data_stack.push(Value::F64(f.sqrt()));
            }
            Opcode::FSin => {
                let f = self.pop_f64()?;
                self.data_stack.push(Value::F64(f.sin()));
            }
            Opcode::FCos => {
                let f = self.pop_f64()?;
                self.data_stack.push(Value::F64(f.cos()));
            }
            Opcode::FTan => {
                let f = self.pop_f64()?;
                self.data_stack.push(Value::F64(f.tan()));
            }

            // === Time operations ===
            Opcode::Now => {
                let duration = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default();
                self.data_stack.push(Value::I64(duration.as_secs() as i64));
            }
            Opcode::Sleep => {
                let ms = self.pop_i64()?;
                if ms > 0 {
                    std::thread::sleep(std::time::Duration::from_millis(ms as u64));
                }
            }

            // === Random operations ===
            Opcode::Random => {
                let bits = self.xorshift64_next();
                let val = (bits >> 11) as f64 / (1u64 << 53) as f64;
                self.data_stack.push(Value::F64(val));
            }
            Opcode::RandInt => {
                let max = self.pop_i64()?;
                let min = self.pop_i64()?;
                if min >= max {
                    self.data_stack.push(Value::I64(min));
                } else {
                    let range = (max - min) as u64;
                    let bits = self.xorshift64_next();
                    let val = min + (bits % range) as i64;
                    self.data_stack.push(Value::I64(val));
                }
            }

            // === Float math operations ===
            Opcode::Floor => {
                let val = self.pop_f64()?;
                self.data_stack.push(Value::F64(val.floor()));
            }
            Opcode::Ceil => {
                let val = self.pop_f64()?;
                self.data_stack.push(Value::F64(val.ceil()));
            }
            Opcode::Round => {
                let val = self.pop_f64()?;
                self.data_stack.push(Value::F64(val.round()));
            }
            Opcode::Log => {
                let val = self.pop_f64()?;
                self.data_stack.push(Value::F64(val.ln()));
            }
            Opcode::Exp => {
                let val = self.pop_f64()?;
                self.data_stack.push(Value::F64(val.exp()));
            }
            Opcode::Atan2 => {
                let x = self.pop_f64()?;
                let y = self.pop_f64()?;
                self.data_stack.push(Value::F64(y.atan2(x)));
            }

            // === Bitwise operations ===
            Opcode::BitAnd => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.data_stack.push(Value::I64(a & b));
            }
            Opcode::BitOr => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.data_stack.push(Value::I64(a | b));
            }
            Opcode::BitXor => {
                let b = self.pop_i64()?;
                let a = self.pop_i64()?;
                self.data_stack.push(Value::I64(a ^ b));
            }
            Opcode::BitNot => {
                let a = self.pop_i64()?;
                self.data_stack.push(Value::I64(!a));
            }

            // === Sort operation ===
            Opcode::Sort => {
                let list = self.pop_list()?;
                let mut items: Vec<Value> = list.iter().cloned().collect();
                // Simple insertion sort that handles both i64 and f64
                items.sort_by(|a, b| {
                    let cmp_f64 = |a: &Value, b: &Value| -> Option<std::cmp::Ordering> {
                        match (a, b) {
                            (Value::I64(a), Value::I64(b)) => a.partial_cmp(b),
                            (Value::F64(a), Value::F64(b)) => a.partial_cmp(b),
                            (Value::I64(a), Value::F64(b)) => (*a as f64).partial_cmp(b),
                            (Value::F64(a), Value::I64(b)) => a.partial_cmp(&(*b as f64)),
                            (Value::Str(a), Value::Str(b)) => a.partial_cmp(b),
                            _ => None,
                        }
                    };
                    cmp_f64(a, b).unwrap_or(std::cmp::Ordering::Equal)
                });
                self.data_stack.push(Value::List(Rc::new(items)));
            }
            Opcode::Rev => {
                let list = self.pop_list()?;
                let mut items: Vec<Value> = list.iter().cloned().collect();
                items.reverse();
                self.data_stack.push(Value::List(Rc::new(items)));
            }
            Opcode::Dedup => {
                let list = self.pop_list()?;
                let mut items: Vec<Value> = Vec::new();
                for item in list.iter() {
                    if items.last().map(|last| last != item).unwrap_or(true) {
                        items.push(item.clone());
                    }
                }
                self.data_stack.push(Value::List(Rc::new(items)));
            }
            Opcode::Take => {
                let n = self.pop_i64()?;
                let list = self.pop_list()?;
                if n < 0 {
                    return Err(VmError::NegativeValue(n));
                }
                let taken: Vec<Value> = list.iter().take(n as usize).cloned().collect();
                self.data_stack.push(Value::List(Rc::new(taken)));
            }
            Opcode::DropN => {
                let n = self.pop_i64()?;
                let list = self.pop_list()?;
                if n < 0 {
                    return Err(VmError::NegativeValue(n));
                }
                let dropped: Vec<Value> = list.iter().skip(n as usize).cloned().collect();
                self.data_stack.push(Value::List(Rc::new(dropped)));
            }

            // === JSON ===
            Opcode::JsonParse => {
                let s = self.pop_str()?;
                let v: serde_json::Value = serde_json::from_str(&s)
                    .map_err(|e| VmError::ProgramError(format!("JSON parse: {e}")))?;
                self.data_stack.push(json_to_whisper(&v));
            }
            Opcode::JsonStringify => {
                let val = self.pop()?.unwrap_signal();
                let j = whisper_to_json(&val)?;
                let s = serde_json::to_string(&j)
                    .map_err(|e| VmError::ProgramError(format!("JSON stringify: {e}")))?;
                self.data_stack.push(Value::Str(Rc::new(s)));
            }

            // === Control flow ===
            Opcode::Cond(offset) => {
                let cond = self.pop_bool()?;
                if !cond {
                    // Jump forward by offset (skip then-branch)
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = (frame.ip as i64 + *offset as i64) as usize;
                }
            }
            Opcode::Jump(offset) => {
                let frame = self.call_stack.last_mut().unwrap();
                frame.ip = (frame.ip as i64 + *offset as i64) as usize;
            }
            Opcode::Loop(offset) => {
                // Check condition at top of stack; if true, jump back
                let cond = self.pop_bool()?;
                if cond {
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = (frame.ip as i64 + *offset as i64) as usize;
                }
                // If false, fall through (exit loop)
            }
            Opcode::Times => {
                let quot = self.pop_ref()?;
                let n = self.pop_i64()?;
                for _ in 0..n {
                    self.execute_ref(&quot)?;
                }
            }

            // === Call/Return ===
            Opcode::Call(name) => {
                // Check 4-slot cache first (Rc::clone is just a refcount bump)
                let word_code = if let Some(code) = self.word_cache_lookup(name) {
                    code
                } else {
                    let code: Rc<[Opcode]> = Rc::clone(
                        self.word_dict
                            .get(name)
                            .ok_or_else(|| VmError::UndefinedWord(name.clone()))?,
                    );
                    self.word_cache_insert(name.clone(), code.clone());
                    code
                };
                let frame = CallFrame {
                    word_name: Some(name.clone()),
                    code: word_code,
                    ip: 0,
                    base: self.data_stack.len(),
                };
                self.call_stack.push(frame);
            }
            Opcode::Return => {
                // Pop current frame
                self.call_stack.pop();
            }

            // === Capability ===
            Opcode::CapCall(id) => {
                let cap_id = *id;
                // Collect args - convention: args are on stack
                let args: Vec<Value> = vec![self.pop()?];
                self.capability_table
                    .call(cap_id, &mut self.data_stack, &args)?;
            }
            Opcode::CapExec => {
                let cap_val = self.pop()?;
                match cap_val {
                    Value::Cap(id, _) => {
                        let args: Vec<Value> = vec![self.pop()?];
                        self.capability_table
                            .call(id, &mut self.data_stack, &args)?;
                    }
                    Value::Ref(code) => {
                        self.execute_ref(&code)?;
                    }
                    other => {
                        return Err(VmError::TypeMismatch {
                            expected: "cap or ref".into(),
                            actual: other.type_name().into(),
                        });
                    }
                }
            }

            // === Confidence ===
            Opcode::ConfLabel(conf) => {
                let v = self.pop()?;
                let confidence = conf.clamp(0.0, 1.0);
                self.data_stack.push(Value::Signal(Box::new(v), confidence));
            }
            Opcode::ProbChoice => {
                // Stack: ... value {alt2} {alt1}
                // Pop both alternatives (alt1 on top, pushed second by codegen)
                let branch_true = self.pop_ref()?; // alt1 (preferred branch)
                let branch_false = self.pop_ref()?; // alt2 (fallback branch)
                                                    // Peek confidence from the value below (don't pop — branch uses it)
                let confidence = self
                    .data_stack
                    .last()
                    .map(|v| v.confidence())
                    .unwrap_or(1.0);
                // Choose branch probabilistically
                let r = xorshift64_next(&mut self.rng_state);
                if r < confidence {
                    self.execute_ref(&branch_true)?;
                } else {
                    self.execute_ref(&branch_false)?;
                }
            }

            // === IO ===
            Opcode::OutputTop => {
                let v = self.pop()?;
                println!("{v}");
            }
            Opcode::OutputAll => {
                println!("Stack: {:?}", self.data_stack);
            }
            Opcode::ReadInput => {
                let mut input = String::new();
                std::io::stdin()
                    .read_line(&mut input)
                    .map_err(|e| VmError::IoError(e.to_string()))?;
                let trimmed = input.trim_end_matches('\n').trim_end_matches('\r');
                self.data_stack
                    .push(Value::Str(Rc::new(trimmed.to_string())));
            }

            // === Definitions (compiler directives, no-op at runtime) ===
            Opcode::DefWord(_) | Opcode::EndDef | Opcode::Import | Opcode::Export => {
                // These are compiler directives, handled before VM execution
            }
        }
        Ok(())
    }

    // === Helper methods ===

    /// Xorshift64* PRNG returning raw 64-bit value (for Random/RandInt).
    fn xorshift64_next(&mut self) -> u64 {
        self.rng_state ^= self.rng_state >> 12;
        self.rng_state ^= self.rng_state << 25;
        self.rng_state ^= self.rng_state >> 27;
        self.rng_state.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// Look up a word in the 4-slot cache. Returns None on miss.
    fn word_cache_lookup(&self, name: &str) -> Option<Rc<[Opcode]>> {
        for (cached_name, cached_code) in self.word_cache.iter().flatten() {
            if cached_name == name {
                return Some(cached_code.clone()); // Rc::clone is just a refcount bump
            }
        }
        None
    }

    /// Insert a word into the cache using round-robin replacement.
    fn word_cache_insert(&mut self, name: String, code: Rc<[Opcode]>) {
        let pos = self.word_cache_pos;
        self.word_cache[pos] = Some((name, code));
        self.word_cache_pos = (pos + 1) % 4;
    }

    fn pop(&mut self) -> Result<Value, VmError> {
        self.data_stack.pop().ok_or(VmError::StackUnderflow {
            expected: 1,
            actual: 0,
        })
    }

    fn pop_i64(&mut self) -> Result<i64, VmError> {
        match self.pop()?.unwrap_signal() {
            Value::I64(n) => Ok(n),
            other => Err(VmError::TypeMismatch {
                expected: "i64".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    fn pop_f64(&mut self) -> Result<f64, VmError> {
        match self.pop()?.unwrap_signal() {
            Value::F64(n) => Ok(n),
            Value::I64(n) => Ok(n as f64), // auto-coerce i64 → f64
            other => Err(VmError::TypeMismatch {
                expected: "f64".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    fn pop_bool(&mut self) -> Result<bool, VmError> {
        // C-style truthiness (seed-runtime compatible): integers/floats are
        // true when non-zero, so guard words can return 0/1 flags.
        match self.pop()?.unwrap_signal() {
            Value::Bool(b) => Ok(b),
            Value::I64(n) => Ok(n != 0),
            Value::F64(x) => Ok(x != 0.0),
            other => Err(VmError::TypeMismatch {
                expected: "bool".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    fn pop_list(&mut self) -> Result<Rc<Vec<Value>>, VmError> {
        match self.pop()? {
            Value::List(l) => Ok(l),
            Value::Signal(v, _) => match *v {
                Value::List(l) => Ok(l),
                other => Err(VmError::TypeMismatch {
                    expected: "[T]".into(),
                    actual: other.type_name().into(),
                }),
            },
            other => Err(VmError::TypeMismatch {
                expected: "[T]".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    fn pop_str(&mut self) -> Result<Rc<String>, VmError> {
        match self.pop()?.unwrap_signal() {
            Value::Str(s) => Ok(s),
            other => Err(VmError::TypeMismatch {
                expected: "str".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    fn pop_ref(&mut self) -> Result<Rc<[Opcode]>, VmError> {
        match self.pop()? {
            Value::Ref(code) => Ok(code),
            Value::Signal(v, _) => match *v {
                Value::Ref(code) => Ok(code),
                other => Err(VmError::TypeMismatch {
                    expected: "ref".into(),
                    actual: other.type_name().into(),
                }),
            },
            other => Err(VmError::TypeMismatch {
                expected: "ref".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    fn pop_map(&mut self) -> Result<Rc<HashMap<String, Value>>, VmError> {
        match self.pop()? {
            Value::Map(m) => Ok(m),
            Value::Signal(v, _) => match *v {
                Value::Map(m) => Ok(m),
                other => Err(VmError::TypeMismatch {
                    expected: "map".into(),
                    actual: other.type_name().into(),
                }),
            },
            other => Err(VmError::TypeMismatch {
                expected: "map".into(),
                actual: other.type_name().into(),
            }),
        }
    }

    /// Execute a quotation (ref block) by pushing a new call frame.
    pub fn execute_ref(&mut self, code: &Rc<[Opcode]>) -> Result<(), VmError> {
        // Guard against runaway nesting (e.g. nested @map): fail with a
        // proper error instead of overflowing the host stack.
        const MAX_REF_DEPTH: usize = 512;
        if self.ref_depth >= MAX_REF_DEPTH {
            return Err(VmError::RecursionLimit(MAX_REF_DEPTH));
        }
        self.ref_depth += 1;
        let result = self.execute_ref_inner(code);
        self.ref_depth -= 1;
        result
    }

    fn execute_ref_inner(&mut self, code: &Rc<[Opcode]>) -> Result<(), VmError> {
        let initial_depth = self.call_stack.len();
        let frame = CallFrame {
            word_name: Some("<block>".into()),
            code: Rc::clone(code),
            ip: 0,
            base: self.data_stack.len(),
        };
        self.call_stack.push(frame);

        // Execute only until the ref frame completes (back to initial depth).
        // Must not continue executing the parent frame — that belongs to the
        // caller (e.g. execute, Map, Fold, etc.).
        while self.call_stack.len() > initial_depth {
            let current_frame = self.call_stack.last_mut().unwrap();
            let ip = current_frame.ip;
            let code_len = current_frame.code.len();

            if ip >= code_len {
                self.call_stack.pop();
                continue;
            }

            // Clone the Rc (cheap refcount bump), not the Opcode (may allocate).
            let code = Rc::clone(&current_frame.code);
            self.call_stack.last_mut().unwrap().ip = ip + 1;

            if self.trace {
                eprintln!(
                    "[trace] <block> op={:?} stack={:?}",
                    code[ip].name(),
                    self.data_stack
                );
            }

            self.step(&code[ip])?;
        }

        Ok(())
    }

    /// Binary operation on numeric stack values.
    fn binary_num_op(
        &mut self,
        i64_op: impl FnOnce(i64, i64) -> Result<Value, VmError>,
        f64_op: impl FnOnce(f64, f64) -> Result<Value, VmError>,
    ) -> Result<(), VmError> {
        let b = self.pop()?;
        let a = self.pop()?;

        let a_conf = a.confidence();
        let b_conf = b.confidence();
        let a_val = a.unwrap_signal();
        let b_val = b.unwrap_signal();

        let result = match (&a_val, &b_val) {
            (Value::I64(a), Value::I64(b)) => i64_op(*a, *b)?,
            (Value::I64(a), Value::F64(b)) => f64_op(*a as f64, *b)?,
            (Value::F64(a), Value::I64(b)) => f64_op(*a, *b as f64)?,
            (Value::F64(a), Value::F64(b)) => f64_op(*a, *b)?,
            (a, b) => {
                return Err(VmError::TypeMismatch {
                    expected: "i64 or f64".into(),
                    actual: format!("{} and {}", a.type_name(), b.type_name()),
                });
            }
        };

        let conf = (a_conf * b_conf).clamp(0.0, 1.0);
        if conf < 1.0 {
            self.data_stack.push(Value::Signal(Box::new(result), conf));
        } else {
            self.data_stack.push(result);
        }
        Ok(())
    }

    /// Comparison operation.
    fn compare_op(&mut self, cmp: impl FnOnce(f64, f64) -> bool) -> Result<(), VmError> {
        let b_val = self.pop()?;
        let a_val = self.pop()?;

        let a_num = value_to_f64(&a_val)?;
        let b_num = value_to_f64(&b_val)?;
        let result = cmp(a_num, b_num);

        let conf = (a_val.confidence() * b_val.confidence()).clamp(0.0, 1.0);
        if conf < 1.0 {
            self.data_stack
                .push(Value::Signal(Box::new(Value::Bool(result)), conf));
        } else {
            self.data_stack.push(Value::Bool(result));
        }
        Ok(())
    }
}

impl Default for Vm {
    fn default() -> Self {
        Self::new()
    }
}

// ── JSON conversion helpers ──────────────────────────────────────────

/// Convert a serde_json Value to a Whisper Value.
fn json_to_whisper(v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::I64(0),
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::I64(i)
            } else if let Some(f) = n.as_f64() {
                Value::F64(f)
            } else {
                Value::I64(0)
            }
        }
        serde_json::Value::String(s) => Value::Str(Rc::new(s.clone())),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(json_to_whisper).collect();
            Value::List(Rc::new(items))
        }
        serde_json::Value::Object(obj) => {
            let mut map: std::collections::HashMap<String, Value> =
                std::collections::HashMap::with_capacity(obj.len());
            for (k, v) in obj {
                map.insert(k.clone(), json_to_whisper(v));
            }
            Value::Map(Rc::new(map))
        }
    }
}

/// Convert a Whisper Value to a serde_json Value.
fn whisper_to_json(v: &Value) -> Result<serde_json::Value, VmError> {
    match v {
        Value::I64(n) => Ok(serde_json::Value::Number((*n).into())),
        Value::F64(n) => {
            // serde_json doesn't support f64 directly as Number, use as_f64
            Ok(serde_json::json!(*n))
        }
        Value::Bool(b) => Ok(serde_json::Value::Bool(*b)),
        Value::Str(s) => Ok(serde_json::Value::String(s.as_ref().clone())),
        Value::List(items) => {
            // Lists ALWAYS serialize as arrays. The legacy heuristic
            // (all-2-elem-str-head lists → object) corrupted list-of-
            // key-lists kv-JSON round-trips; objects are Maps since L-08.
            let arr: Result<Vec<_>, _> = items.iter().map(whisper_to_json).collect();
            Ok(serde_json::Value::Array(arr?))
        }
        Value::Signal(inner, _) => whisper_to_json(inner),
        Value::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map.iter() {
                obj.insert(k.clone(), whisper_to_json(v)?);
            }
            Ok(serde_json::Value::Object(obj))
        }
        _ => Ok(serde_json::Value::Null),
    }
}

fn value_to_f64(v: &Value) -> Result<f64, VmError> {
    match v.unwrap_signal_ref() {
        Value::I64(n) => Ok(*n as f64),
        Value::F64(n) => Ok(*n),
        Value::Bool(b) => Ok(if *b { 1.0 } else { 0.0 }),
        other => Err(VmError::TypeMismatch {
            expected: "number".into(),
            actual: other.type_name().into(),
        }),
    }
}

#[cfg(test)]
mod tests {
    fn push_list(items: &[i64]) -> Vec<Opcode> {
        let mut ops: Vec<Opcode> = items.iter().map(|&n| Opcode::PushI64(n)).collect();
        ops.push(Opcode::PushI64(items.len() as i64));
        ops.push(Opcode::PushList);
        ops
    }

    #[test]
    fn list_equality_is_structural() {
        let mut vm = Vm::new();
        let mut prog = push_list(&[1, 2]);
        prog.extend(push_list(&[1, 2]));
        prog.push(Opcode::Eq);
        let r = vm.execute(&prog).unwrap().unwrap();
        assert_eq!(r, Value::Bool(true), "[1 2] = [1 2] should be #t");
    }

    #[test]
    fn list_inequality_is_structural() {
        let mut vm = Vm::new();
        let mut prog = push_list(&[1, 2]);
        prog.extend(push_list(&[1, 3]));
        prog.push(Opcode::Eq);
        let r = vm.execute(&prog).unwrap().unwrap();
        assert_eq!(r, Value::Bool(false), "[1 2] = [1 3] should be #f");
    }

    #[test]
    fn negative_nth_is_an_error() {
        let mut vm = Vm::new();
        let mut prog = push_list(&[1, 2]);
        prog.push(Opcode::PushI64(-1));
        prog.push(Opcode::Nth);
        let err = vm.execute(&prog).unwrap_err();
        assert!(matches!(err, VmError::NegativeValue(-1)));
    }

    #[test]
    fn deep_quote_nesting_errors_instead_of_crashing() {
        // 3000 nested `{ ... N @times }` frames must surface RecursionLimit,
        // not blow the host stack (previously a hard crash). Run on a
        // big-stack thread like the CLI does; the guard, not the OS, stops.
        std::thread::Builder::new()
            .stack_size(64 * 1024 * 1024)
            .spawn(|| {
                let mut code: Vec<Opcode> = vec![Opcode::PushI64(0)];
                for _ in 0..3000 {
                    code = vec![Opcode::PushI64(1), Opcode::PushRef(code), Opcode::Times];
                }
                let mut vm = Vm::new();
                let err = vm.execute(&code).unwrap_err();
                assert!(matches!(err, VmError::RecursionLimit(512)));
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn strslice_binary_semantics() {
        // v1.0.8: slices are byte-semantic; slicing into a multibyte
        // char's bytes yields raw bytes (binary files must survive).
        let mut vm = Vm::new();
        let mut prog = vec![Opcode::PushStr(std::rc::Rc::new("h\u{e9}llo".to_string()))];
        prog.push(Opcode::PushI64(1));
        prog.push(Opcode::PushI64(1));
        prog.push(Opcode::StrSlice);
        let out = vm.execute(&prog).unwrap().unwrap();
        match out {
            Value::Str(s) => assert_eq!(s.len(), 1, "one byte out"),
            other => panic!("{other:?}"),
        }
    }

    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        let mut vm = Vm::new();
        let program = [Opcode::PushI64(3), Opcode::PushI64(4), Opcode::Add];
        let result = vm.execute(&program).unwrap();
        assert_eq!(result, Some(Value::I64(7)));
    }

    #[test]
    fn test_dup_and_swap() {
        let mut vm = Vm::new();
        // dup: 5 → 5 5
        // then add: 5 5 → 10
        let program = [Opcode::PushI64(5), Opcode::Dup, Opcode::Add];
        let result = vm.execute(&program).unwrap();
        assert_eq!(result, Some(Value::I64(10)));
    }

    #[test]
    fn test_swap() {
        let mut vm = Vm::new();
        // 3 4 swap → 4 3 → sub → 1
        let program = [
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Swap,
            Opcode::Sub,
        ];
        let result = vm.execute(&program).unwrap();
        assert_eq!(result, Some(Value::I64(1)));
    }

    #[test]
    fn test_comparison() {
        let mut vm = Vm::new();
        let program = [
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Add,
            Opcode::PushI64(7),
            Opcode::Eq,
        ];
        let result = vm.execute(&program).unwrap();
        assert_eq!(result, Some(Value::Bool(true)));
    }

    #[test]
    fn test_quotation_block() {
        let mut vm = Vm::new();
        // { 2 * } — a block that doubles the value on the stack
        let block: Rc<[Opcode]> =
            Rc::from(vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return].into_boxed_slice());
        vm.data_stack.push(Value::I64(5));
        vm.execute_ref(&block).unwrap();
        let result = vm.data_stack.pop().unwrap();
        assert_eq!(result, Value::I64(10));
    }

    #[test]
    fn test_fold_sum() {
        let mut vm = Vm::new();
        // [1, 2, 3] 0 { + } @fold
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
        ])));
        vm.data_stack.push(Value::I64(0));
        vm.data_stack
            .push(Value::Ref(Rc::from(vec![Opcode::Add].into_boxed_slice())));
        let result = vm.execute(&[Opcode::Fold]).unwrap();
        assert_eq!(result, Some(Value::I64(6)), "Fold sum [1,2,3] should be 6");
    }

    #[test]
    fn test_fibonacci() {
        // Naive fib using word definitions and recursion
        // : fib { _ 1 > ??_ 1 - fib _ 2 - fib +|_]] } ;
        // Then call: 10 fib
        // For now, just test iterative approach on VM
        let mut vm = Vm::new();
        // Push n=10, then manually compute fib(10)=55
        // Simple iterative: a b swap over + swap (Forth-style)
        // [0, 1] start, then 10 times: swap over + swap
        let block: Rc<[Opcode]> = Rc::from(
            vec![
                Opcode::Swap, // a b → b a
                Opcode::Dup,  // b a → b a a
                Opcode::Rot,  // b a a → a a b
                Opcode::Add,  // a a b → a (a+b)
                Opcode::Swap, // a (a+b) → (a+b) a
                Opcode::Return,
            ]
            .into_boxed_slice(),
        );
        vm.data_stack.push(Value::I64(0));
        vm.data_stack.push(Value::I64(1));
        for _ in 0..10 {
            vm.execute_ref(&block).unwrap();
        }
        // Stack: fib(10) fib(11), pop top = fib(11) = 89, we want fib(10) = 55
        let _fib11 = vm.data_stack.pop().unwrap(); // 89
        let fib10 = vm.data_stack.pop().unwrap(); // 55
        assert_eq!(fib10, Value::I64(55));
    }

    #[test]
    fn test_division_by_zero() {
        let mut vm = Vm::new();
        let program = [Opcode::PushI64(10), Opcode::PushI64(0), Opcode::Div];
        let result = vm.execute(&program);
        assert!(result.is_err());
    }

    #[test]
    fn test_eq_simple() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0));
        vm.data_stack.push(Value::I64(0));
        let r = vm.execute(&[Opcode::Eq]).unwrap();
        assert_eq!(r, Some(Value::Bool(true)), "0==0 should be true");
    }

    #[test]
    fn test_nth_direct() {
        let mut vm = Vm::new();
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        vm.data_stack.push(Value::I64(0));
        let r = vm.execute(&[Opcode::Nth]).unwrap();
        assert_eq!(
            r,
            Some(Value::I64(0)),
            "Direct @nth should give 0, got {:?}",
            r
        );
    }

    #[test]
    fn test_call_nth() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        let r = vm.execute(&[Opcode::Call("tt".to_string())]).unwrap();
        assert_eq!(
            r,
            Some(Value::I64(0)),
            "Call tt should return 0, got {:?}",
            r
        );
    }

    #[test]
    fn test_execute_ref_nested_call() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        vm.define_word(
            "check".to_string(),
            vec![
                Opcode::Dup,
                Opcode::Call("tt".to_string()),
                Opcode::PushI64(0),
                Opcode::Eq,
            ],
        );
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        let ref_code = Rc::from(vec![Opcode::Call("check".to_string())].into_boxed_slice());
        vm.execute_ref(&ref_code).unwrap();
        let r = vm.data_stack.pop().unwrap();
        assert_eq!(r, Value::Bool(true), "execute_ref: got {:?}", r);
    }

    #[test]
    fn test_push_eq_in_word() {
        let mut vm = Vm::new();
        vm.define_word(
            "check".to_string(),
            vec![Opcode::PushI64(0), Opcode::PushI64(0), Opcode::Eq],
        );
        let r = vm.execute(&[Opcode::Call("check".to_string())]).unwrap();
        assert_eq!(
            r,
            Some(Value::Bool(true)),
            "PushI64+PushI64+Eq in word: got {:?}",
            r
        );
    }

    #[test]
    fn test_dup_call_push() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        vm.define_word(
            "check".to_string(),
            vec![
                Opcode::Dup,
                Opcode::Call("tt".to_string()),
                Opcode::PushI64(0),
            ],
        );
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        // Stack should be [token, 0, 0] after check → execute pops 0
        let r = vm.execute(&[Opcode::Call("check".to_string())]).unwrap();
        assert_eq!(r, Some(Value::I64(0)), "Dup+Call+Push: got {:?}", r);
    }

    #[test]
    fn test_dup_then_call() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        vm.define_word(
            "check".to_string(),
            vec![Opcode::Dup, Opcode::Call("tt".to_string())],
        );
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        let r = vm.execute(&[Opcode::Call("check".to_string())]).unwrap();
        // After Dup+Call(tt): [token, 0] → pop should be Some(I64(0))
        assert_eq!(r, Some(Value::I64(0)), "Dup+Call(tt): got {:?}", r);
    }

    #[test]
    fn test_direct_nested_call() {
        // Full check body with Eq: this now works correctly
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        vm.define_word(
            "check".to_string(),
            vec![
                Opcode::Dup,
                Opcode::Call("tt".to_string()),
                Opcode::PushI64(0),
                Opcode::Eq,
            ],
        );
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        let r = vm.execute(&[Opcode::Call("check".to_string())]).unwrap();
        assert_eq!(
            r,
            Some(Value::Bool(true)),
            "check([0,42]) should be true, got {:?}",
            r
        );
    }

    // === ProbChoice tests ===

    fn make_ref(opcodes: Vec<Opcode>) -> Value {
        Value::Ref(Rc::from(opcodes.into_boxed_slice()))
    }

    /// Confidence 1.0 always chooses the preferred branch (alt1).
    #[test]
    fn test_prob_choice_full_confidence() {
        let mut vm = Vm::new();
        // Stack: value {alt2} {alt1}
        // alt1 doubles the value, alt2 multiplies by 3
        let alt1 = make_ref(vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return]);
        let alt2 = make_ref(vec![Opcode::PushI64(3), Opcode::Mul, Opcode::Return]);
        vm.data_stack.push(Value::I64(10)); // value with implicit conf 1.0
        vm.data_stack.push(alt2); // alt2 deeper
        vm.data_stack.push(alt1); // alt1 on top
        let r = vm.execute(&[Opcode::ProbChoice]).unwrap();
        // 10 * 2 = 20 (always alt1 when confidence is 1.0)
        assert_eq!(r, Some(Value::I64(20)), "confidence 1.0 → always alt1");
    }

    /// Confidence 0.0 always chooses the fallback branch (alt2).
    #[test]
    fn test_prob_choice_zero_confidence() {
        let mut vm = Vm::new();
        let alt1 = make_ref(vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return]);
        let alt2 = make_ref(vec![Opcode::PushI64(3), Opcode::Mul, Opcode::Return]);
        let conf_val = Value::Signal(Box::new(Value::I64(10)), 0.0);
        vm.data_stack.push(conf_val);
        vm.data_stack.push(alt2);
        vm.data_stack.push(alt1);
        let val = vm.execute(&[Opcode::ProbChoice]).unwrap().unwrap();
        // 10 * 3 = 30, wrapped in Signal since confidence propagates
        assert_eq!(
            val.unwrap_signal(),
            Value::I64(30),
            "confidence 0.0 → always alt2"
        );
    }

    /// Confidence 0.5 should exercise both branches over many trials.
    #[test]
    fn test_prob_choice_even_confidence() {
        let mut saw_alt1 = false;
        let mut saw_alt2 = false;
        for _ in 0..100 {
            let mut vm = Vm::new();
            let alt1 = make_ref(vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return]);
            let alt2 = make_ref(vec![Opcode::PushI64(3), Opcode::Mul, Opcode::Return]);
            let conf_val = Value::Signal(Box::new(Value::I64(10)), 0.5);
            vm.data_stack.push(conf_val);
            vm.data_stack.push(alt2);
            vm.data_stack.push(alt1);
            let r = vm.execute(&[Opcode::ProbChoice]).unwrap();
            match r.map(|v| v.unwrap_signal()) {
                Some(Value::I64(20)) => saw_alt1 = true, // 10 * 2
                Some(Value::I64(30)) => saw_alt2 = true, // 10 * 3
                _ => {}
            }
            if saw_alt1 && saw_alt2 {
                break;
            }
        }
        assert!(
            saw_alt1,
            "ProbChoice with c=0.5 should produce alt1 sometimes"
        );
        assert!(
            saw_alt2,
            "ProbChoice with c=0.5 should produce alt2 sometimes"
        );
    }

    /// Non-Signal values have implicit confidence 1.0 → always alt1.
    #[test]
    fn test_prob_choice_no_signal_value() {
        let mut vm = Vm::new();
        let alt1 = make_ref(vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return]);
        let alt2 = make_ref(vec![Opcode::PushI64(3), Opcode::Mul, Opcode::Return]);
        vm.data_stack.push(Value::I64(7)); // implicit conf 1.0
        vm.data_stack.push(alt2);
        vm.data_stack.push(alt1);
        let r = vm.execute(&[Opcode::ProbChoice]).unwrap();
        assert_eq!(
            r,
            Some(Value::I64(14)),
            "implicit confidence 1.0 → alt1: 7*2=14"
        );
    }

    // === String operation tests ===

    #[test]
    fn test_strlen() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("Hello".into())));
        let r = vm.execute(&[Opcode::StrLen]).unwrap();
        assert_eq!(r, Some(Value::I64(5)));
    }

    #[test]
    fn test_strlen_empty() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("".into())));
        let r = vm.execute(&[Opcode::StrLen]).unwrap();
        assert_eq!(r, Some(Value::I64(0)));
    }

    #[test]
    fn test_strcat() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("Hello, ".into())));
        vm.data_stack.push(Value::Str(Rc::new("World!".into())));
        let r = vm.execute(&[Opcode::StrCat]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("Hello, World!".into()))));
    }

    #[test]
    fn test_strcat_empty() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("".into())));
        vm.data_stack.push(Value::Str(Rc::new("Hi".into())));
        let r = vm.execute(&[Opcode::StrCat]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("Hi".into()))));
    }

    #[test]
    fn test_strslice() {
        let mut vm = Vm::new();
        vm.data_stack
            .push(Value::Str(Rc::new("Hello, World!".into())));
        vm.data_stack.push(Value::I64(0));
        vm.data_stack.push(Value::I64(5));
        let r = vm.execute(&[Opcode::StrSlice]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("Hello".into()))));
    }

    #[test]
    fn test_strslice_middle() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("abcdef".into())));
        vm.data_stack.push(Value::I64(2));
        vm.data_stack.push(Value::I64(3));
        let r = vm.execute(&[Opcode::StrSlice]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("cde".into()))));
    }

    #[test]
    fn test_strslice_clamp_bounds() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("Hi".into())));
        vm.data_stack.push(Value::I64(0));
        vm.data_stack.push(Value::I64(100)); // len exceeds string
        let r = vm.execute(&[Opcode::StrSlice]).unwrap();
        assert_eq!(
            r,
            Some(Value::Str(Rc::new("Hi".into()))),
            "should clamp to string length"
        );
    }

    // === Mod confidence propagation ===

    #[test]
    fn test_mod_confidence() {
        // 10:0.5 3 mod → Signal(1, 0.5)  (confidence propagates)
        let mut vm = Vm::new();
        let val = Value::Signal(Box::new(Value::I64(10)), 0.5);
        vm.data_stack.push(val);
        vm.data_stack.push(Value::I64(3));
        let r = vm.execute(&[Opcode::Mod]).unwrap().unwrap();
        // Check inner value: 10%3=1
        assert_eq!(r.unwrap_signal_ref(), &Value::I64(1));
        // Confidence should be 0.5 (0.5 from signal * 1.0 from literal)
        let conf = r.confidence();
        assert!(
            (conf - 0.5).abs() < 0.001,
            "expected confidence 0.5, got {conf}"
        );
    }

    #[test]
    fn test_mod_division_by_zero_with_confidence() {
        let mut vm = Vm::new();
        let val = Value::Signal(Box::new(Value::I64(10)), 0.8);
        vm.data_stack.push(val);
        vm.data_stack.push(Value::I64(0));
        let result = vm.execute(&[Opcode::Mod]);
        assert!(result.is_err());
    }

    // === New string op tests ===

    #[test]
    fn test_streq_true() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("abc".into())));
        vm.data_stack.push(Value::Str(Rc::new("abc".into())));
        let r = vm.execute(&[Opcode::StrEq]).unwrap();
        assert_eq!(r, Some(Value::Bool(true)));
    }

    #[test]
    fn test_streq_false() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("abc".into())));
        vm.data_stack.push(Value::Str(Rc::new("xyz".into())));
        let r = vm.execute(&[Opcode::StrEq]).unwrap();
        assert_eq!(r, Some(Value::Bool(false)));
    }

    #[test]
    fn test_strlt() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("abc".into())));
        vm.data_stack.push(Value::Str(Rc::new("xyz".into())));
        let r = vm.execute(&[Opcode::StrLt]).unwrap();
        assert_eq!(r, Some(Value::Bool(true)));
    }

    #[test]
    fn test_strfind_found() {
        let mut vm = Vm::new();
        vm.data_stack
            .push(Value::Str(Rc::new("hello world".into())));
        vm.data_stack.push(Value::Str(Rc::new("world".into())));
        let r = vm.execute(&[Opcode::StrFind]).unwrap();
        assert_eq!(r, Some(Value::I64(6)));
    }

    #[test]
    fn test_strfind_not_found() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("hello".into())));
        vm.data_stack.push(Value::Str(Rc::new("xyz".into())));
        let r = vm.execute(&[Opcode::StrFind]).unwrap();
        assert_eq!(r, Some(Value::I64(-1)));
    }

    #[test]
    fn test_strreplace() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("a-b-c".into())));
        vm.data_stack.push(Value::Str(Rc::new("-".into())));
        vm.data_stack.push(Value::Str(Rc::new(":".into())));
        let r = vm.execute(&[Opcode::StrReplace]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("a:b:c".into()))));
    }

    #[test]
    fn test_strtoi64() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("42".into())));
        let r = vm.execute(&[Opcode::StrToI64]).unwrap();
        assert_eq!(r, Some(Value::I64(42)));
    }

    #[test]
    fn test_strtoi64_negative() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("-7".into())));
        let r = vm.execute(&[Opcode::StrToI64]).unwrap();
        assert_eq!(r, Some(Value::I64(-7)));
    }

    #[test]
    fn test_strtoi64_invalid() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("notanumber".into())));
        let r = vm.execute(&[Opcode::StrToI64]);
        assert!(r.is_err());
    }

    #[test]
    fn test_i64tostr() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(99));
        let r = vm.execute(&[Opcode::I64ToStr]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("99".into()))));
    }

    #[test]
    fn test_i64tostr_zero() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0));
        let r = vm.execute(&[Opcode::I64ToStr]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("0".into()))));
    }

    // === Float op tests ===

    #[test]
    fn test_i64tof64() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(42));
        let r = vm.execute(&[Opcode::I64ToF64]).unwrap();
        assert_eq!(r, Some(Value::F64(42.0)));
    }

    #[test]
    fn test_f64toi64() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(3.9));
        let r = vm.execute(&[Opcode::F64ToI64]).unwrap();
        assert_eq!(r, Some(Value::I64(3))); // truncate toward zero
    }

    #[test]
    fn test_fsqrt() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(16.0));
        let r = vm.execute(&[Opcode::FSqrt]).unwrap();
        assert_eq!(r, Some(Value::F64(4.0)));
    }

    #[test]
    fn test_fsin() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(0.0));
        let r = vm.execute(&[Opcode::FSin]).unwrap();
        assert_eq!(r, Some(Value::F64(0.0)));
    }

    #[test]
    fn test_fcos() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(0.0));
        let r = vm.execute(&[Opcode::FCos]).unwrap();
        assert_eq!(r, Some(Value::F64(1.0)));
    }

    #[test]
    fn test_ftan() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(0.0));
        let r = vm.execute(&[Opcode::FTan]).unwrap();
        assert_eq!(r, Some(Value::F64(0.0)));
    }

    // === JSON tests ===

    #[test]
    fn test_json_parse_number() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("42".into())));
        let r = vm.execute(&[Opcode::JsonParse]).unwrap();
        assert_eq!(r, Some(Value::I64(42)));
    }

    #[test]
    fn test_json_parse_string() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("\"hello\"".into())));
        let r = vm.execute(&[Opcode::JsonParse]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("hello".into()))));
    }

    #[test]
    fn test_json_parse_array() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("[1,2,3]".into())));
        let r = vm.execute(&[Opcode::JsonParse]).unwrap();
        assert_eq!(
            r,
            Some(Value::List(Rc::new(vec![
                Value::I64(1),
                Value::I64(2),
                Value::I64(3),
            ])))
        );
    }

    #[test]
    fn test_json_parse_object() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("{\"a\":1}".into())));
        let r = vm.execute(&[Opcode::JsonParse]).unwrap();
        // Object becomes a Map (json-parse round-trips maps as objects)
        match r {
            Some(Value::Map(map)) => {
                assert_eq!(map.len(), 1);
                assert_eq!(map.get("a"), Some(&Value::I64(1)));
            }
            _ => panic!("expected map"),
        }
    }

    #[test]
    fn test_json_parse_bool() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("true".into())));
        let r = vm.execute(&[Opcode::JsonParse]).unwrap();
        assert_eq!(r, Some(Value::Bool(true)));
    }

    #[test]
    fn test_json_stringify_number() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(42));
        let r = vm.execute(&[Opcode::JsonStringify]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("42".into()))));
    }

    #[test]
    fn test_json_stringify_array() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
        ])));
        let r = vm.execute(&[Opcode::JsonStringify]).unwrap();
        assert_eq!(r, Some(Value::Str(Rc::new("[1,2,3]".into()))));
    }

    #[test]
    fn test_json_roundtrip() {
        let original = r#"{"x":1,"y":[2,3]}"#;
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new(original.into())));
        let val = vm.execute(&[Opcode::JsonParse]).unwrap().unwrap();
        vm.data_stack.push(val);
        let json = vm.execute(&[Opcode::JsonStringify]).unwrap().unwrap();
        // Re-parse to verify
        vm.data_stack.push(json);
        let reparsed = vm.execute(&[Opcode::JsonParse]).unwrap();
        assert!(reparsed.is_some());
    }
    // === New opcode tests ===

    #[test]
    fn test_filter_basic() {
        let mut vm = Vm::new();
        // [3 1 4 1 5] { _ 2 > } @filter -> [3 4 5]
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(3),
            Value::I64(1),
            Value::I64(4),
            Value::I64(1),
            Value::I64(5),
        ])));
        let pred: Rc<[Opcode]> = Rc::from(vec![Opcode::PushI64(2), Opcode::Gt].into_boxed_slice());
        vm.data_stack.push(Value::Ref(pred));
        let r = vm.execute(&[Opcode::Filter]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::I64(3));
                assert_eq!(items[1], Value::I64(4));
                assert_eq!(items[2], Value::I64(5));
            }
            _ => panic!("expected list, got {:?}", r),
        }
    }

    #[test]
    fn test_filter_empty_result() {
        let mut vm = Vm::new();
        vm.data_stack
            .push(Value::List(Rc::new(vec![Value::I64(1), Value::I64(2)])));
        let pred: Rc<[Opcode]> = Rc::from(vec![Opcode::PushI64(10), Opcode::Gt].into_boxed_slice());
        vm.data_stack.push(Value::Ref(pred));
        let r = vm.execute(&[Opcode::Filter]).unwrap().unwrap();
        match r {
            Value::List(items) => assert_eq!(items.len(), 0),
            _ => panic!("expected empty list"),
        }
    }

    #[test]
    fn test_strsplit_basic() {
        let mut vm = Vm::new();
        vm.data_stack
            .push(Value::Str(Rc::new("hello world".into())));
        vm.data_stack.push(Value::Str(Rc::new(" ".into())));
        let r = vm.execute(&[Opcode::StrSplit]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::Str(Rc::new("hello".into())));
                assert_eq!(items[1], Value::Str(Rc::new("world".into())));
            }
            _ => panic!("expected list, got {:?}", r),
        }
    }

    #[test]
    fn test_strsplit_newlines() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new(
            "a
b
c"
            .into(),
        )));
        vm.data_stack.push(Value::Str(Rc::new(
            "
"
            .into(),
        )));
        let r = vm.execute(&[Opcode::StrSplit]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_strsplit_no_match() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("hello".into())));
        vm.data_stack.push(Value::Str(Rc::new(",".into())));
        let r = vm.execute(&[Opcode::StrSplit]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 1);
                assert_eq!(items[0], Value::Str(Rc::new("hello".into())));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_ctos_basic() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(65)); // 'A'
        let r = vm.execute(&[Opcode::Ctos]).unwrap().unwrap();
        assert_eq!(r, Value::Str(Rc::new("A".into())));
    }

    #[test]
    fn test_ctos_lowercase() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(97)); // 'a'
        let r = vm.execute(&[Opcode::Ctos]).unwrap().unwrap();
        assert_eq!(r, Value::Str(Rc::new("a".into())));
    }

    #[test]
    fn test_listslice_basic() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
            Value::I64(4),
            Value::I64(5),
        ])));
        vm.data_stack.push(Value::I64(1)); // start
        vm.data_stack.push(Value::I64(3)); // len
        let r = vm.execute(&[Opcode::ListSlice]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::I64(2));
                assert_eq!(items[1], Value::I64(3));
                assert_eq!(items[2], Value::I64(4));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_listslice_tail() {
        let mut vm = Vm::new();
        // [1 2 3 4] 1 3 listslice -> [2 3 4]
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
            Value::I64(4),
        ])));
        vm.data_stack.push(Value::I64(1));
        vm.data_stack.push(Value::I64(3));
        let r = vm.execute(&[Opcode::ListSlice]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::I64(2));
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_listslice_clamp() {
        let mut vm = Vm::new();
        vm.data_stack
            .push(Value::List(Rc::new(vec![Value::I64(1), Value::I64(2)])));
        vm.data_stack.push(Value::I64(0));
        vm.data_stack.push(Value::I64(100)); // exceeds length
        let r = vm.execute(&[Opcode::ListSlice]).unwrap().unwrap();
        match r {
            Value::List(items) => assert_eq!(items.len(), 2),
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn test_shl_basic() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(1));
        vm.data_stack.push(Value::I64(3));
        let r = vm.execute(&[Opcode::Shl]).unwrap().unwrap();
        assert_eq!(r, Value::I64(8));
    }

    #[test]
    fn test_shr_basic() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(8));
        vm.data_stack.push(Value::I64(2));
        let r = vm.execute(&[Opcode::Shr]).unwrap().unwrap();
        assert_eq!(r, Value::I64(2));
    }

    #[test]
    fn test_shr_arithmetic() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(16));
        vm.data_stack.push(Value::I64(4));
        let r = vm.execute(&[Opcode::Shr]).unwrap().unwrap();
        assert_eq!(r, Value::I64(1));
    }

    #[test]
    fn test_shl_zero_shift() {
        // Shifting by 0 should leave the value unchanged
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(42));
        vm.data_stack.push(Value::I64(0));
        let r = vm.execute(&[Opcode::Shl]).unwrap().unwrap();
        assert_eq!(r, Value::I64(42));
    }

    #[test]
    fn test_shr_zero_shift() {
        // Shifting by 0 should leave the value unchanged
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(42));
        vm.data_stack.push(Value::I64(0));
        let r = vm.execute(&[Opcode::Shr]).unwrap().unwrap();
        assert_eq!(r, Value::I64(42));
    }

    #[test]
    fn test_strsplit_dash_separator() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::Str(Rc::new("a-b-c".into())));
        vm.data_stack.push(Value::Str(Rc::new("-".into())));
        let r = vm.execute(&[Opcode::StrSplit]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Value::Str(Rc::new("a".into())));
                assert_eq!(items[1], Value::Str(Rc::new("b".into())));
                assert_eq!(items[2], Value::Str(Rc::new("c".into())));
            }
            _ => panic!("expected list, got {:?}", r),
        }
    }

    #[test]
    fn test_listslice_first_element() {
        // Slice [10 20 30] from index 0, length 2 → [10 20], then nth 1 → 20
        let mut vm = Vm::new();
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(10),
            Value::I64(20),
            Value::I64(30),
        ])));
        vm.data_stack.push(Value::I64(0)); // start
        vm.data_stack.push(Value::I64(2)); // length
        let r = vm.execute(&[Opcode::ListSlice]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::I64(10));
                assert_eq!(items[1], Value::I64(20));
            }
            _ => panic!("expected list, got {:?}", r),
        }
    }

    #[test]
    fn test_ctos_digit_char() {
        // ASCII 48 = '0'
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(48));
        let r = vm.execute(&[Opcode::Ctos]).unwrap().unwrap();
        assert_eq!(r, Value::Str(Rc::new("0".into())));
    }

    #[test]
    fn test_filter_keeps_matching_elements() {
        // Filter [1 2 3 4 5] keeping elements > 3 → [4 5]
        let mut vm = Vm::new();
        vm.data_stack.push(Value::List(Rc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3),
            Value::I64(4),
            Value::I64(5),
        ])));
        let pred: Rc<[Opcode]> = Rc::from(vec![Opcode::PushI64(3), Opcode::Gt].into_boxed_slice());
        vm.data_stack.push(Value::Ref(pred));
        let r = vm.execute(&[Opcode::Filter]).unwrap().unwrap();
        match r {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], Value::I64(4));
                assert_eq!(items[1], Value::I64(5));
            }
            _ => panic!("expected list, got {:?}", r),
        }
    }

    // === Time operation tests ===

    #[test]
    fn test_now_returns_unix_timestamp() {
        let mut vm = Vm::new();
        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        let r = vm.execute(&[Opcode::Now]).unwrap().unwrap();
        let after = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;
        match r {
            Value::I64(ts) => {
                assert!(
                    ts >= before && ts <= after,
                    "timestamp {ts} not in [{before}, {after}]"
                );
            }
            _ => panic!("expected I64, got {:?}", r),
        }
    }

    // === Float math tests ===

    #[test]
    fn test_floor() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(3.7));
        let r = vm.execute(&[Opcode::Floor]).unwrap();
        assert_eq!(r, Some(Value::F64(3.0)));
    }

    #[test]
    fn test_floor_negative() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(-2.3));
        let r = vm.execute(&[Opcode::Floor]).unwrap();
        assert_eq!(r, Some(Value::F64(-3.0)));
    }

    #[test]
    fn test_ceil() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(3.2));
        let r = vm.execute(&[Opcode::Ceil]).unwrap();
        assert_eq!(r, Some(Value::F64(4.0)));
    }

    #[test]
    fn test_round() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(3.5));
        let r = vm.execute(&[Opcode::Round]).unwrap();
        assert_eq!(r, Some(Value::F64(4.0)));
    }

    #[test]
    fn test_log() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(std::f64::consts::E));
        let r = vm.execute(&[Opcode::Log]).unwrap().unwrap();
        match r {
            Value::F64(v) => assert!((v - 1.0).abs() < 1e-10, "ln(e) should be 1.0, got {v}"),
            _ => panic!("expected F64"),
        }
    }

    #[test]
    fn test_exp() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(1.0));
        let r = vm.execute(&[Opcode::Exp]).unwrap().unwrap();
        match r {
            Value::F64(v) => assert!(
                (v - std::f64::consts::E).abs() < 1e-10,
                "exp(1) should be e"
            ),
            _ => panic!("expected F64"),
        }
    }

    #[test]
    fn test_atan2() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::F64(1.0)); // x
        vm.data_stack.push(Value::F64(1.0)); // y
        let r = vm.execute(&[Opcode::Atan2]).unwrap().unwrap();
        match r {
            Value::F64(v) => {
                let expected = std::f64::consts::FRAC_PI_4;
                assert!(
                    (v - expected).abs() < 1e-10,
                    "atan2(1,1) should be pi/4, got {v}"
                );
            }
            _ => panic!("expected F64"),
        }
    }

    // === Bitwise operation tests ===

    #[test]
    fn test_bitand() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0xFF));
        vm.data_stack.push(Value::I64(0x0F));
        let r = vm.execute(&[Opcode::BitAnd]).unwrap();
        assert_eq!(r, Some(Value::I64(0x0F)));
    }

    #[test]
    fn test_bitand_no_overlap() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0xF0));
        vm.data_stack.push(Value::I64(0x0F));
        let r = vm.execute(&[Opcode::BitAnd]).unwrap();
        assert_eq!(r, Some(Value::I64(0x00)));
    }

    #[test]
    fn test_bitor() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0xF0));
        vm.data_stack.push(Value::I64(0x0F));
        let r = vm.execute(&[Opcode::BitOr]).unwrap();
        assert_eq!(r, Some(Value::I64(0xFF)));
    }

    #[test]
    fn test_bitxor() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0xFF));
        vm.data_stack.push(Value::I64(0x0F));
        let r = vm.execute(&[Opcode::BitXor]).unwrap();
        assert_eq!(r, Some(Value::I64(0xF0)));
    }

    #[test]
    fn test_bitnot() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(0));
        let r = vm.execute(&[Opcode::BitNot]).unwrap();
        assert_eq!(r, Some(Value::I64(-1))); // !0 == -1 in two's complement
    }

    #[test]
    fn test_bitnot_roundtrip() {
        let mut vm = Vm::new();
        vm.data_stack.push(Value::I64(42));
        let r = vm.execute(&[Opcode::BitNot, Opcode::BitNot]).unwrap();
        assert_eq!(r, Some(Value::I64(42))); // double NOT is identity
    }
}
