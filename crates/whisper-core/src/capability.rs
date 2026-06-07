/// Capability-based security model.
///
use std::rc::Rc;
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

/// HTTP GET capability with host whitelist.
pub struct HttpGetCap {
    pub id: u16,
    pub allowed_hosts: Vec<String>,
}

impl Capability for HttpGetCap {
    fn id(&self) -> u16 { self.id }
    fn name(&self) -> &str { "http_get" }
    fn description(&self) -> &str { "HTTP GET requests to allowed hosts" }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        if args.is_empty() {
            return Err(VmError::ProgramError("http_get: expected URL argument".into()));
        }
        let url_str = match &args[0] {
            Value::Str(s) => s.as_ref().clone(),
            other => return Err(VmError::TypeMismatch {
                expected: "str".into(), actual: other.type_name().into(),
            }),
        };

        let host = extract_host(&url_str).unwrap_or(&url_str);
        let allowed = self.allowed_hosts.iter().any(|h| host.contains(h.as_str()));
        if !allowed {
            return Err(VmError::CapabilityDenied(format!(
                "Host '{}' not in allowed hosts: {:?}", host, self.allowed_hosts
            )));
        }

        match http_get(&url_str) {
            Ok(body) => {
                data_stack.push(Value::Str(Rc::new(body)));
                Ok(())
            }
            Err(e) => Err(VmError::IoError(e)),
        }
    }
}

/// HTTP POST capability with host whitelist.
pub struct HttpPostCap {
    pub id: u16,
    pub allowed_hosts: Vec<String>,
}

impl Capability for HttpPostCap {
    fn id(&self) -> u16 { self.id }
    fn name(&self) -> &str { "http_post" }
    fn description(&self) -> &str { "HTTP POST requests to allowed hosts" }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        if args.len() < 2 {
            return Err(VmError::ProgramError("http_post: expected URL and body".into()));
        }
        let url_str = match &args[0] { Value::Str(s) => s.as_ref().clone(), other => return Err(VmError::TypeMismatch { expected: "str".into(), actual: other.type_name().into() }) };
        let body_str = match &args[1] { Value::Str(s) => s.as_ref().clone(), other => return Err(VmError::TypeMismatch { expected: "str".into(), actual: other.type_name().into() }) };

        let host = extract_host(&url_str).unwrap_or(&url_str);
        let allowed = self.allowed_hosts.iter().any(|h| host.contains(h.as_str()));
        if !allowed {
            return Err(VmError::CapabilityDenied(format!("Host '{}' not allowed", host)));
        }

        match http_post(&url_str, &body_str) {
            Ok(response) => { data_stack.push(Value::Str(Rc::new(response))); Ok(()) }
            Err(e) => Err(VmError::IoError(e)),
        }
    }
}

fn extract_host(url: &str) -> Option<&str> {
    let s = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
    s.split('/').next()
}

fn http_get(url: &str) -> Result<String, String> {
    let host = extract_host(url).ok_or_else(|| "Invalid URL".to_string())?;
    let path = url.find(host).map(|i| &url[i + host.len()..]).unwrap_or("/");

    let mut stream = std::net::TcpStream::connect((host, 80))
        .or_else(|_| std::net::TcpStream::connect((host, 443)))
        .map_err(|e| format!("Connect failed: {e}"))?;

    use std::io::{Read, Write};
    let request = format!("GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|e| e.to_string())?;

    let body = response.split("\r\n\r\n").nth(1).unwrap_or(&response);
    Ok(body.to_string())
}

fn http_post(url: &str, body: &str) -> Result<String, String> {
    let host = extract_host(url).ok_or_else(|| "Invalid URL".to_string())?;
    let path = url.find(host).map(|i| &url[i + host.len()..]).unwrap_or("/");

    let mut stream = std::net::TcpStream::connect((host, 80))
        .map_err(|e| format!("Connect failed: {e}"))?;

    use std::io::{Read, Write};
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).map_err(|e| e.to_string())?;

    let mut response = String::new();
    stream.read_to_string(&mut response).map_err(|e| e.to_string())?;

    let body = response.split("\r\n\r\n").nth(1).unwrap_or(&response);
    Ok(body.to_string())
}

/// Environment variable capability.
pub struct EnvCap {
    pub id: u16,
}

impl Capability for EnvCap {
    fn id(&self) -> u16 { self.id }
    fn name(&self) -> &str { "env" }
    fn description(&self) -> &str { "Read environment variables" }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        if args.is_empty() {
            return Err(VmError::ProgramError("env: expected variable name".into()));
        }
        let name = match &args[0] {
            Value::Str(s) => s.as_ref().clone(),
            other => return Err(VmError::TypeMismatch {
                expected: "str".into(), actual: other.type_name().into(),
            }),
        };
        let value = std::env::var(&name).unwrap_or_default();
        data_stack.push(Value::Str(std::rc::Rc::new(value)));
        Ok(())
    }
}

/// Shell command execution capability.
pub struct ExecCap {
    pub id: u16,
}

impl Capability for ExecCap {
    fn id(&self) -> u16 { self.id }
    fn name(&self) -> &str { "exec" }
    fn description(&self) -> &str { "Execute shell commands" }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        if args.is_empty() {
            return Err(VmError::ProgramError("exec: expected command string".into()));
        }
        let cmd = match &args[0] {
            Value::Str(s) => s.as_ref().clone(),
            other => return Err(VmError::TypeMismatch {
                expected: "str".into(), actual: other.type_name().into(),
            }),
        };

        #[cfg(windows)]
        let output = std::process::Command::new("cmd")
            .args(["/C", &cmd])
            .output();
        #[cfg(not(windows))]
        let output = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
                let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
                let status = out.status.code().unwrap_or(-1) as i64;
                // Push [status, stdout, stderr]
                use std::rc::Rc;
                data_stack.push(Value::List(Rc::new(vec![
                    Value::I64(status),
                    Value::Str(Rc::new(stdout)),
                    Value::Str(Rc::new(stderr)),
                ])));
                Ok(())
            }
            Err(e) => Err(VmError::IoError(e.to_string())),
        }
    }
}

