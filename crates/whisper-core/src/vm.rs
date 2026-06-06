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
}

impl Vm {
    /// Create a new VM with empty stacks and default settings.
    pub fn new() -> Self {
        Vm {
            data_stack: Vec::new(),
            call_stack: Vec::new(),
            word_dict: HashMap::new(),
            capability_table: CapabilityTable::new(),
            memory: Vec::new(),
            trace: false,
        }
    }

    /// Create a VM with a pre-bound capability table.
    pub fn with_capabilities(capability_table: CapabilityTable) -> Self {
        Vm {
            data_stack: Vec::new(),
            call_stack: Vec::new(),
            word_dict: HashMap::new(),
            capability_table,
            memory: Vec::new(),
            trace: false,
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
            Opcode::Mod => self.binary_int_op(|a, b| {
                if b == 0 {
                    Err(VmError::DivisionByZero)
                } else {
                    Ok(Value::I64(a % b))
                }
            })?,

            // === Comparison ===
            Opcode::Eq => {
                let a = self.pop()?;
                let b = self.pop()?;
                self.data_stack
                    .push(Value::Bool(a.unwrap_signal().equals(&b.unwrap_signal())));
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
                return Err(VmError::ProgramError(
                    "PushList must be handled by compiler".into(),
                ));
            }
            Opcode::PushRef => {
                return Err(VmError::ProgramError(
                    "PushRef must be handled by compiler".into(),
                ));
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
            Opcode::Call(_index) => {
                // For now, Call uses word_dict lookup
                // In .wbin, this would be pre-resolved
                return Err(VmError::ProgramError(
                    "Call must be resolved by compiler to direct bytecode".into(),
                ));
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
                // {alt2} {alt1} ?|  — choose alt1 or alt2 randomly by confidence
                let _alt2 = self.pop_ref()?;
                let alt1 = self.pop_ref()?;
                // Simple: use alt1 (deterministic fallback)
                // In probability mode, would use confidence-weighted random choice
                self.execute_ref(&alt1)?;
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
    fn execute_ref(&mut self, code: &Rc<[Opcode]>) -> Result<(), VmError> {
        let frame = CallFrame {
            word_name: Some("<block>".into()),
            code: Rc::clone(code),
            ip: 0,
            base: self.data_stack.len(),
        };
        self.call_stack.push(frame);

        // Execute until this frame completes
        while let Some(current_frame) = self.call_stack.last() {
            let ip = current_frame.ip;
            let code_len = current_frame.code.len();

            if ip >= code_len {
                self.call_stack.pop();
                break;
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

    /// Binary operation on integer-only stack values.
    fn binary_int_op(
        &mut self,
        op: impl FnOnce(i64, i64) -> Result<Value, VmError>,
    ) -> Result<(), VmError> {
        let b = self.pop_i64()?;
        let a = self.pop_i64()?;
        let result = op(a, b)?;
        self.data_stack.push(result);
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
}
