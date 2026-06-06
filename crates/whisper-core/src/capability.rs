/// Capability-based security model.
///
/// Whisper programs have zero IO by default. All side effects require
/// explicit capability tokens that are bound at load time by the host.
///
/// Three-layer defense:
/// 1. Compile time: Cap(u16) is a distinct type, cannot mix with data
/// 2. Load time: Capability table is initialized once, immutable at runtime
/// 3. Runtime: Capabilities run in restricted context (path/host whitelists)

use crate::value::Value;
use crate::VmError;

/// Trait for capabilities that can be called by Whisper programs.
///
/// Each capability has a unique numeric ID and a human-readable description
/// for the capability authorization prompt.
pub trait Capability: Send + Sync {
    /// Numeric capability ID (bound to @n in Whisper source).
    fn id(&self) -> u16;

    /// Human-readable name (e.g., "file_read", "http_post").
    fn name(&self) -> &str;

    /// Description shown to users during authorization.
    fn description(&self) -> &str;

    /// Execute the capability with the given arguments from the VM stack.
    /// The capability can push results back onto the VM's data stack.
    fn call(
        &self,
        data_stack: &mut Vec<Value>,
        args: &[Value],
    ) -> Result<(), VmError>;
}

/// A table of capabilities indexed by their numeric ID.
///
/// This is the single point of control for all IO in a Whisper program.
/// Once created, it is immutable — dynamic capability creation is impossible.
pub struct CapabilityTable {
    capabilities: Vec<Option<Box<dyn Capability>>>,
}

impl CapabilityTable {
    /// Create an empty capability table.
    pub fn new() -> Self {
        CapabilityTable {
            capabilities: Vec::new(),
        }
    }

    /// Bind a capability at the given slot.
    pub fn bind(&mut self, cap: Box<dyn Capability>) {
        let id = cap.id() as usize;
        while id >= self.capabilities.len() {
            self.capabilities.push(None);
        }
        self.capabilities[id] = Some(cap);
    }

    /// Look up a capability by ID. Returns None if not bound.
    pub fn get(&self, id: u16) -> Option<&dyn Capability> {
        self.capabilities
            .get(id as usize)
            .and_then(|opt| opt.as_deref())
    }

    /// Check if a capability is bound.
    pub fn is_bound(&self, id: u16) -> bool {
        self.get(id).is_some()
    }

    /// Call a capability by ID, forwarding arguments.
    pub fn call(
        &self,
        id: u16,
        data_stack: &mut Vec<Value>,
        args: &[Value],
    ) -> Result<(), VmError> {
        match self.get(id) {
            Some(cap) => cap.call(data_stack, args),
            None => Err(VmError::CapabilityNotBound(id)),
        }
    }
}

impl Default for CapabilityTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Standard capability: file reading with path whitelist.
pub struct FileReadCap {
    pub id: u16,
    pub allowed_paths: Vec<std::path::PathBuf>,
}

impl Capability for FileReadCap {
    fn id(&self) -> u16 {
        self.id
    }
    fn name(&self) -> &str {
        "file_read"
    }
    fn description(&self) -> &str {
        "Read files from allowed paths"
    }
    fn call(
        &self,
        data_stack: &mut Vec<Value>,
        args: &[Value],
    ) -> Result<(), VmError> {
        if args.is_empty() {
            return Err(VmError::ProgramError(
                "file_read: expected path argument".into(),
            ));
        }
        let path_str = match &args[0] {
            Value::Str(s) => s.as_ref().clone(),
            other => {
                return Err(VmError::TypeMismatch {
                    expected: "str".into(),
                    actual: other.type_name().into(),
                })
            }
        };
        let path = std::path::Path::new(&path_str);

        // Check path whitelist
        let allowed = self.allowed_paths.iter().any(|allowed| {
            path.starts_with(allowed)
        });
        if !allowed {
            return Err(VmError::CapabilityDenied(format!(
                "Path '{}' is not in allowed paths",
                path.display()
            )));
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                data_stack
                    .push(Value::Str(std::rc::Rc::new(content)));
                Ok(())
            }
            Err(e) => Err(VmError::IoError(e.to_string())),
        }
    }
}

/// Standard capability: file writing with path whitelist.
pub struct FileWriteCap {
    pub id: u16,
    pub allowed_paths: Vec<std::path::PathBuf>,
}

impl Capability for FileWriteCap {
    fn id(&self) -> u16 {
        self.id
    }
    fn name(&self) -> &str {
        "file_write"
    }
    fn description(&self) -> &str {
        "Write files to allowed paths"
    }
    fn call(
        &self,
        _data_stack: &mut Vec<Value>,
        args: &[Value],
    ) -> Result<(), VmError> {
        if args.len() < 2 {
            return Err(VmError::ProgramError(
                "file_write: expected path and content arguments".into(),
            ));
        }
        let path_str = match &args[0] {
            Value::Str(s) => s.as_ref().clone(),
            other => {
                return Err(VmError::TypeMismatch {
                    expected: "str".into(),
                    actual: other.type_name().into(),
                })
            }
        };
        let content = match &args[1] {
            Value::Str(s) => s.as_ref().clone(),
            other => {
                return Err(VmError::TypeMismatch {
                    expected: "str".into(),
                    actual: other.type_name().into(),
                })
            }
        };
        let path = std::path::Path::new(&path_str);

        let allowed = self.allowed_paths.iter().any(|allowed| {
            path.starts_with(allowed)
        });
        if !allowed {
            return Err(VmError::CapabilityDenied(format!(
                "Path '{}' is not in allowed write paths",
                path.display()
            )));
        }

        std::fs::write(path, content)
            .map_err(|e| VmError::IoError(e.to_string()))
    }
}
