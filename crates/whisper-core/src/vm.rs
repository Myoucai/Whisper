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
    pub word_dict: HashMap<String, Vec<Opcode>>,
    /// Capability table (IO capabilities bound at init)
    pub capability_table: CapabilityTable,
    /// Linear memory for data structures (maps, arrays, etc.)
    pub memory: Vec<Value>,
    /// Whether to trace execution (debug mode)
    pub trace: bool,
    /// PRNG state for probabilistic choice (`?|`)
    rng_state: u64,
}

impl Vm {
    /// Create a new VM with empty stacks and default settings.
    pub fn new() -> Self {
        // Seed PRNG with system time for probabilistic choice
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEAD_BEEF)
            .wrapping_mul(0x2545F4914F6CDD1D);
        Vm {
            data_stack: Vec::new(),
            call_stack: Vec::new(),
            word_dict: HashMap::new(),
            capability_table: CapabilityTable::new(),
            memory: Vec::new(),
            trace: false,
            rng_state: seed,
        }
    }

    /// Create a VM with a pre-bound capability table.
    pub fn with_capabilities(capability_table: CapabilityTable) -> Self {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0xDEAD_BEEF)
            .wrapping_mul(0x2545F4914F6CDD1D);
        Vm {
            data_stack: Vec::new(),
            call_stack: Vec::new(),
            word_dict: HashMap::new(),
            capability_table,
            memory: Vec::new(),
            trace: false,
            rng_state: seed,
        }
    }

    /// Define a word in the dictionary.
    pub fn define_word(&mut self, name: String, code: Vec<Opcode>) {
        self.word_dict.insert(name, code);
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

            let op = frame.code[ip].clone();
            self.call_stack.last_mut().unwrap().ip = ip + 1;

            if self.trace {
                eprintln!(
                    "[trace] {} | op={:?} | stack={:?}",
                    self.call_stack
                        .last()
                        .and_then(|f| f.word_name.as_deref())
                        .unwrap_or("?"),
                    op.name(),
                    self.data_stack
                );
            }

            self.step(&op)?;
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

            // === Arithmetic ===
            Opcode::Add => self.binary_num_op(|a, b| Ok(Value::I64(a + b)), |a, b| {
                Ok(Value::F64(a + b))
            })?,
            Opcode::Sub => self.binary_num_op(|a, b| Ok(Value::I64(a - b)), |a, b| {
                Ok(Value::F64(a - b))
            })?,
            Opcode::Mul => self.binary_num_op(|a, b| Ok(Value::I64(a * b)), |a, b| {
                Ok(Value::F64(a * b))
            })?,
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
                |_a, _b| Err(VmError::TypeMismatch {
                    expected: "i64".into(),
                    actual: "f64".into(),
                }),
            )?,

            // === Comparison ===
            Opcode::Eq => {
                let a = self.pop()?;
                let b = self.pop()?;
                let result = a.clone().unwrap_signal().equals(&b.clone().unwrap_signal());
                self.data_stack.push(Value::Bool(result));
            }
            Opcode::Lt => self.compare_op(|a, b| a < b)?,
            Opcode::Gt => self.compare_op(|a, b| a > b)?,
            Opcode::Neq => {
                let a = self.pop()?;
                let b = self.pop()?;
                self.data_stack.push(Value::Bool(
                    !a.unwrap_signal().equals(&b.unwrap_signal()),
                ));
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
                self.data_stack
                    .push(Value::Str(Rc::new(s.clone())));
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
                self.data_stack
                    .push(Value::List(Rc::new(items)));
            }
            Opcode::PushRef(ref code) => {
                self.data_stack.push(Value::Ref(Rc::from(
                    code.clone().into_boxed_slice(),
                )));
            }

            // === List operations ===
            Opcode::Nth => {
                let n = self.pop_i64()? as usize;
                let list = self.pop_list()?;
                if n >= list.len() {
                    return Err(VmError::ProgramError(format!(
                        "Index {n} out of bounds for list of length {}",
                        list.len()
                    )));
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
                for item in list.iter() {
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                    if let Some(result) = self.data_stack.pop() {
                        results.push(result);
                    }
                }
                self.data_stack
                    .push(Value::List(Rc::new(results)));
            }
            Opcode::Each => {
                let quot = self.pop_ref()?;
                let list = self.pop_list()?;
                for item in list.iter() {
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                }
            }
            Opcode::Fold => {
                let quot = self.pop_ref()?;
                let init = self.pop()?;
                let list = self.pop_list()?;
                let mut acc = init;
                for item in list.iter() {
                    self.data_stack.push(acc.clone());
                    self.data_stack.push(item.clone());
                    self.execute_ref(&quot)?;
                    acc = self.pop().unwrap_or(acc);
                }
                self.data_stack.push(acc);
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
                let start = start.max(0) as usize;
                let len = len.max(0) as usize;
                let end = (start + len).min(s.len());
                let start = start.min(s.len());
                let substr: String = s[start..end].to_string();
                self.data_stack.push(Value::Str(Rc::new(substr)));
            }
            Opcode::StrEq => {
                let s2 = self.pop_str()?;
                let s1 = self.pop_str()?;
                self.data_stack.push(Value::Bool(s1.as_ref() == s2.as_ref()));
            }
            Opcode::StrLt => {
                let s2 = self.pop_str()?;
                let s1 = self.pop_str()?;
                self.data_stack.push(Value::Bool(s1.as_ref() < s2.as_ref()));
            }
            Opcode::StrFind => {
                let pat = self.pop_str()?;
                let s = self.pop_str()?;
                let idx = s.find(pat.as_ref())
                    .map(|i| i as i64)
                    .unwrap_or(-1);
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
                let n = s.trim().parse::<i64>()
                    .map_err(|_| VmError::ProgramError(format!("Cannot parse '{}' as i64", s)))?;
                self.data_stack.push(Value::I64(n));
            }
            Opcode::I64ToStr => {
                let n = self.pop_i64()?;
                self.data_stack.push(Value::Str(Rc::new(n.to_string())));
            }

            // === Control flow ===
            Opcode::Cond(offset) => {
                let cond = self.pop_bool()?;
                if !cond {
                    // Jump forward by offset (skip then-branch)
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = (frame.ip as i32 + offset) as usize;
                }
            }
            Opcode::Jump(offset) => {
                let frame = self.call_stack.last_mut().unwrap();
                frame.ip = (frame.ip as i32 + offset) as usize;
            }
            Opcode::Loop(offset) => {
                // Check condition at top of stack; if true, jump back
                let cond = self.pop_bool()?;
                if cond {
                    let frame = self.call_stack.last_mut().unwrap();
                    frame.ip = (frame.ip as i32 + offset) as usize;
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
                let word_code = self
                    .word_dict
                    .get(name)
                    .cloned()
                    .ok_or_else(|| VmError::UndefinedWord(name.clone()))?;
                let frame = CallFrame {
                    word_name: Some(name.clone()),
                    code: Rc::from(word_code.into_boxed_slice()),
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
                self.data_stack
                    .push(Value::Signal(Box::new(v), confidence));
            }
            Opcode::ProbChoice => {
                // Stack: ... value {alt2} {alt1}
                // Pop both alternatives (alt1 on top, pushed second by codegen)
                let branch_true = self.pop_ref()?;  // alt1 (preferred branch)
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

    #[allow(dead_code)]
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
        match self.pop()?.unwrap_signal() {
            Value::Bool(b) => Ok(b),
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

    /// Execute a quotation (ref block) by pushing a new call frame.
    pub fn execute_ref(&mut self, code: &Rc<[Opcode]>) -> Result<(), VmError> {
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

            let op = current_frame.code[ip].clone();
            self.call_stack.last_mut().unwrap().ip = ip + 1;

            if self.trace {
                eprintln!("[trace] <block> op={:?} stack={:?}", op.name(), self.data_stack);
            }

            self.step(&op)?;
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
            self.data_stack
                .push(Value::Signal(Box::new(result), conf));
        } else {
            self.data_stack.push(result);
        }
        Ok(())
    }

    /// Comparison operation.
    fn compare_op(
        &mut self,
        cmp: impl FnOnce(f64, f64) -> bool,
    ) -> Result<(), VmError> {
        let b_val = self.pop()?;
        let a_val = self.pop()?;

        let a_num = value_to_f64(&a_val)?;
        let b_num = value_to_f64(&b_val)?;
        let result = cmp(a_num, b_num);

        let conf = (a_val.confidence() * b_val.confidence()).clamp(0.0, 1.0);
        if conf < 1.0 {
            self.data_stack.push(Value::Signal(
                Box::new(Value::Bool(result)),
                conf,
            ));
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
    use super::*;

    #[test]
    fn test_simple_arithmetic() {
        let mut vm = Vm::new();
        let program = [
            Opcode::PushI64(3),
            Opcode::PushI64(4),
            Opcode::Add,
        ];
        let result = vm.execute(&program).unwrap();
        assert_eq!(result, Some(Value::I64(7)));
    }

    #[test]
    fn test_dup_and_swap() {
        let mut vm = Vm::new();
        // dup: 5 → 5 5
        // then add: 5 5 → 10
        let program = [
            Opcode::PushI64(5),
            Opcode::Dup,
            Opcode::Add,
        ];
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
        let block: Rc<[Opcode]> = Rc::from(
            vec![Opcode::PushI64(2), Opcode::Mul, Opcode::Return].into_boxed_slice(),
        );
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
            Value::I64(1), Value::I64(2), Value::I64(3),
        ])));
        vm.data_stack.push(Value::I64(0));
        vm.data_stack.push(Value::Ref(Rc::from(
            vec![Opcode::Add].into_boxed_slice()
        )));
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
                Opcode::Swap,   // a b → b a
                Opcode::Dup,    // b a → b a a
                Opcode::Rot,    // b a a → a a b
                Opcode::Add,    // a a b → a (a+b)
                Opcode::Swap,   // a (a+b) → (a+b) a
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
        let program = [
            Opcode::PushI64(10),
            Opcode::PushI64(0),
            Opcode::Div,
        ];
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
        assert_eq!(r, Some(Value::I64(0)), "Direct @nth should give 0, got {:?}", r);
    }

    #[test]
    fn test_call_nth() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        let r = vm.execute(&[Opcode::Call("tt".to_string())]).unwrap();
        assert_eq!(r, Some(Value::I64(0)), "Call tt should return 0, got {:?}", r);
    }

    #[test]
    fn test_execute_ref_nested_call() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        vm.define_word("check".to_string(), vec![
            Opcode::Dup, Opcode::Call("tt".to_string()),
            Opcode::PushI64(0), Opcode::Eq,
        ]);
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
        vm.define_word("check".to_string(), vec![Opcode::PushI64(0), Opcode::PushI64(0), Opcode::Eq]);
        let r = vm.execute(&[Opcode::Call("check".to_string())]).unwrap();
        assert_eq!(r, Some(Value::Bool(true)), "PushI64+PushI64+Eq in word: got {:?}", r);
    }

    #[test]
    fn test_dup_call_push() {
        let mut vm = Vm::new();
        vm.define_word("tt".to_string(), vec![Opcode::PushI64(0), Opcode::Nth]);
        vm.define_word("check".to_string(), vec![
            Opcode::Dup, Opcode::Call("tt".to_string()), Opcode::PushI64(0),
        ]);
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
        vm.define_word("check".to_string(), vec![Opcode::Dup, Opcode::Call("tt".to_string())]);
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
        vm.define_word("check".to_string(), vec![
            Opcode::Dup, Opcode::Call("tt".to_string()),
            Opcode::PushI64(0), Opcode::Eq,
        ]);
        let token = Value::List(Rc::new(vec![Value::I64(0), Value::I64(42)]));
        vm.data_stack.push(token);
        let r = vm.execute(&[Opcode::Call("check".to_string())]).unwrap();
        assert_eq!(r, Some(Value::Bool(true)), "check([0,42]) should be true, got {:?}", r);
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
        vm.data_stack.push(Value::I64(10));       // value with implicit conf 1.0
        vm.data_stack.push(alt2);                   // alt2 deeper
        vm.data_stack.push(alt1);                   // alt1 on top
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
        assert_eq!(val.unwrap_signal(), Value::I64(30), "confidence 0.0 → always alt2");
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
        assert!(saw_alt1, "ProbChoice with c=0.5 should produce alt1 sometimes");
        assert!(saw_alt2, "ProbChoice with c=0.5 should produce alt2 sometimes");
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
        assert_eq!(r, Some(Value::I64(14)), "implicit confidence 1.0 → alt1: 7*2=14");
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
        vm.data_stack.push(Value::Str(Rc::new("Hello, World!".into())));
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
        assert_eq!(r, Some(Value::Str(Rc::new("Hi".into()))), "should clamp to string length");
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
        assert!((conf - 0.5).abs() < 0.001, "expected confidence 0.5, got {conf}");
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
        vm.data_stack.push(Value::Str(Rc::new("hello world".into())));
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
}
