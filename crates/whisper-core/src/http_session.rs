//! L-06 HTTP 会话能力：cookie 罐 + 状态码/响应头 + 零出域审计。
//!
//! 栈型（http-req / @6）：`method url headers-json body -- status headers-json body`
//! - cookie 罐自动携带（Set-Cookie 收集为 name=value）
//! - 不自动跟随重定向（302 原样返回 Location）
//! - 每次请求记审计行（METHOD URL STATUS）；http-audit / @7 导出全量

use crate::capability::{host_allowed, parse_url, Capability};
use crate::value::Value;
use crate::VmError;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// 会话 HTTP 能力（id 6）。
pub struct HttpSessionCap {
    pub id: u16,
    pub allowed_hosts: Vec<String>,
    pub jar: Arc<Mutex<std::collections::HashMap<String, String>>>,
    pub audit: Arc<Mutex<Vec<String>>>,
}

impl Capability for HttpSessionCap {
    fn id(&self) -> u16 {
        self.id
    }
    fn name(&self) -> &str {
        "http_session"
    }
    fn description(&self) -> &str {
        "HTTP with cookie jar, status/headers, audit log"
    }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        let pop_str = |ds: &mut Vec<Value>, what: &str| -> Result<String, VmError> {
            let v = ds
                .pop()
                .ok_or_else(|| VmError::StackUnderflow { expected: 1, actual: 0 })?;
            match v {
                Value::Str(s) => Ok(s.as_ref().clone()),
                other => Err(VmError::TypeMismatch {
                    expected: what.into(),
                    actual: other.type_name().into(),
                }),
            }
        };
        // VM CapCall 约定先弹 1 个实参（栈顶 = body）；其余三个自行弹
        let body = match args.first() {
            Some(Value::Str(s)) => s.as_ref().clone(),
            Some(other) => {
                return Err(VmError::TypeMismatch {
                    expected: "body".into(),
                    actual: other.type_name().into(),
                })
            }
            None => String::new(),
        };
        let headers_json = pop_str(data_stack, "headers-json")?;
        let url = pop_str(data_stack, "url")?;
        let method = pop_str(data_stack, "method")?;

        let parsed = parse_url(&url)
            .ok_or_else(|| VmError::ProgramError(format!("Invalid URL: {url}")))?;
        if !host_allowed(&parsed.host, &self.allowed_hosts) {
            return Err(VmError::CapabilityDenied(format!(
                "Host '{}' not in allowed hosts: {:?}",
                parsed.host, self.allowed_hosts
            )));
        }

        // 用户头（会话层管理 Cookie/Content-Length/Host）
        let mut req_headers = String::new();
        for (k, v) in parse_simple_json_object(&headers_json) {
            let kl = k.to_ascii_lowercase();
            if kl == "cookie" || kl == "content-length" || kl == "host" {
                continue;
            }
            req_headers.push_str(&format!("{k}: {v}\r\n"));
        }
        {
            let jar = self.jar.lock().unwrap();
            if !jar.is_empty() {
                let cookies: Vec<String> =
                    jar.iter().map(|(k, v)| format!("{k}={v}")).collect();
                req_headers.push_str(&format!("Cookie: {}\r\n", cookies.join("; ")));
            }
        }
        let (ct, extra) = if body.is_empty() {
            ("", String::new())
        } else {
            let has_ct = headers_json.to_ascii_lowercase().contains("content-type");
            (
                if has_ct {
                    ""
                } else {
                    "Content-Type: application/x-www-form-urlencoded\r\n"
                },
                format!("Content-Length: {}\r\n", body.len()),
            )
        };

        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{req_headers}{ct}{extra}\r\n{body}",
            path = parsed.path,
            host = parsed.host,
        );

        use std::io::{ErrorKind, Read, Write};
        use std::net::{TcpStream, ToSocketAddrs};
        use std::time::Duration;

        // L-06 真靶加固：连接 10s / 读写 15s 超时。挂死端点（如 Windows
        // nslookup 对畸形参数进入交互态、永不回包）不得卡死 VM——超时/
        // 连接后 IO 失败一律化为 status 0 空响应（无证据单位），扫描继续。
        const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
        const IO_TIMEOUT: Duration = Duration::from_secs(15);

        // ( -- ) 无响应吞吐：status 0 / 空 headers / 空 body + 审计标记
        macro_rules! no_response {
            ($why:expr) => {{
                self.audit
                    .lock()
                    .unwrap()
                    .push(format!("{method} {url} {}", $why));
                data_stack.push(Value::I64(0));
                data_stack.push(Value::Str(Rc::new("{}".to_string())));
                data_stack.push(Value::Str(Rc::new(String::new())));
                return Ok(());
            }};
        }

        let addr = (parsed.host.as_str(), parsed.port)
            .to_socket_addrs()
            .map_err(|e| VmError::IoError(e.to_string()))?
            .next()
            .ok_or_else(|| VmError::IoError("no address resolved".into()))?;
        let mut stream = match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
            Ok(s) => s,
            Err(_) => no_response!("TIMEOUT-CONNECT"),
        };
        let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
        let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
        if stream.write_all(request.as_bytes()).is_err() {
            no_response!("IOERR-WRITE");
        }
        let mut response = String::new();
        match stream.read_to_string(&mut response) {
            Ok(_) => {}
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                no_response!("TIMEOUT-READ");
            }
            Err(_) => no_response!("IOERR-READ"),
        }

        let (head, resp_body) = match response.split_once("\r\n\r\n") {
            Some((h, b)) => (h, b.to_string()),
            None => (response.as_str(), String::new()),
        };
        let mut lines = head.split("\r\n");
        let status: i64 = lines
            .next()
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        let mut out_headers: Vec<(String, String)> = Vec::new();
        for line in lines {
            if let Some((k, v)) = line.split_once(':') {
                let k = k.trim().to_string();
                let v = v.trim().to_string();
                if k.eq_ignore_ascii_case("set-cookie") {
                    if let Some(nv) = v.split(';').next() {
                        if let Some((n, val)) = nv.split_once('=') {
                            self.jar
                                .lock()
                                .unwrap()
                                .insert(n.trim().to_string(), val.trim().to_string());
                        }
                    }
                } else if k.eq_ignore_ascii_case("location")
                    || k.eq_ignore_ascii_case("content-type")
                {
                    out_headers.push((k, v));
                }
            }
        }
        self.audit
            .lock()
            .unwrap()
            .push(format!("{method} {url} {status}"));

        data_stack.push(Value::I64(status));
        let hj = if out_headers.is_empty() {
            "{}".to_string()
        } else {
            let items: Vec<String> = out_headers
                .iter()
                .map(|(k, v)| format!("{:?}: {:?}", k, v))
                .collect();
            format!("{{{}}}", items.join(","))
        };
        data_stack.push(Value::Str(Rc::new(hj)));
        data_stack.push(Value::Str(Rc::new(resp_body)));
        Ok(())
    }
}

/// 审计导出能力（id 7）：-- 审计行（\n 连接）
pub struct HttpAuditCap {
    pub id: u16,
    pub audit: Arc<Mutex<Vec<String>>>,
}

impl Capability for HttpAuditCap {
    fn id(&self) -> u16 {
        self.id
    }
    fn name(&self) -> &str {
        "http_audit"
    }
    fn description(&self) -> &str {
        "Export HTTP session audit log"
    }
    fn call(&self, data_stack: &mut Vec<Value>, _args: &[Value]) -> Result<(), VmError> {
        let log = self.audit.lock().unwrap().join("\n");
        data_stack.push(Value::Str(Rc::new(log)));
        Ok(())
    }
}

/// 极简 JSON 对象解析（平 str:str，会话头足够）
fn parse_simple_json_object(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let inner = match s.trim().strip_prefix('{').and_then(|r| r.strip_suffix('}')) {
        Some(i) => i,
        None => return out,
    };
    let mut rest = inner;
    while let Some((kv, tail)) = split_top_level(rest) {
        if let Some((k, v)) = parse_kv(kv) {
            out.push((k, v));
        }
        rest = tail;
    }
    if let Some((k, v)) = parse_kv(rest) {
        out.push((k, v));
    }
    out
}

fn split_top_level(s: &str) -> Option<(&str, &str)> {
    let mut depth = 0i32;
    let mut in_str = false;
    for (i, c) in s.char_indices() {
        match c {
            '"' => in_str = !in_str,
            '[' | '{' if !in_str => depth += 1,
            ']' | '}' if !in_str => depth -= 1,
            ',' if !in_str && depth == 0 => return Some((&s[..i], &s[i + 1..])),
            _ => {}
        }
    }
    None
}

fn parse_kv(s: &str) -> Option<(String, String)> {
    let (k, v) = s.split_once(':')?;
    let k = k.trim().trim_matches('"').to_string();
    let v = v.trim().trim_matches('"').to_string();
    if k.is_empty() {
        None
    } else {
        Some((k, v))
    }
}
