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
    /// Test seam: custom TLS config/roots (None = webpki system roots).
    #[allow(dead_code)]
    pub tls_override: Option<Arc<rustls::ClientConfig>>,
    /// L-12 raw 模式（id 8）：body 按 hex 串解码为原始字节发送——
    /// multipart/二进制载荷通道（zip-slip 探针等）。响应仍为文本。
    pub raw_body: bool,
}

impl Capability for HttpSessionCap {
    fn id(&self) -> u16 {
        self.id
    }
    fn name(&self) -> &str {
        if self.raw_body {
            "http_session_raw"
        } else {
            "http_session"
        }
    }
    fn description(&self) -> &str {
        "HTTP with cookie jar, status/headers, audit log"
    }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        let pop_str = |ds: &mut Vec<Value>, what: &str| -> Result<String, VmError> {
            let v = ds.pop().ok_or(VmError::StackUnderflow {
                expected: 1,
                actual: 0,
            })?;
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
        let (status, hj, resp_body) = self.request_once(method, url, headers_json, body)?;
        data_stack.push(Value::I64(status));
        data_stack.push(Value::Str(Rc::new(hj)));
        data_stack.push(Value::Str(Rc::new(resp_body)));
        Ok(())
    }
}

impl HttpSessionCap {
    /// 单次会话请求核心（call 与并发 cap 共用）。返回
    /// (status, headers-json, body)；无响应路径返回 (0, "{}", "")。
    pub fn request_once(
        &self,
        method: String,
        url: String,
        headers_json: String,
        body: String,
    ) -> Result<(i64, String, String), VmError> {
        let parsed =
            parse_url(&url).ok_or_else(|| VmError::ProgramError(format!("Invalid URL: {url}")))?;
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
                let cookies: Vec<String> = jar.iter().map(|(k, v)| format!("{k}={v}")).collect();
                req_headers.push_str(&format!("Cookie: {}\r\n", cookies.join("; ")));
            }
        }
        // L-12 raw 模式：hex body → 原始字节（multipart/二进制）。
        let raw_bytes: Option<Vec<u8>> = if self.raw_body {
            if body.is_empty() {
                None
            } else {
                Some(decode_hex(&body).map_err(VmError::ProgramError)?)
            }
        } else {
            None
        };
        let body_len = raw_bytes.as_ref().map_or(body.len(), |b| b.len());
        let (ct, extra) = if body_len == 0 {
            ("", String::new())
        } else {
            // raw 载荷（multipart 等）必带用户 Content-Type，不注入默认值
            let has_ct =
                self.raw_body || headers_json.to_ascii_lowercase().contains("content-type");
            (
                if has_ct {
                    ""
                } else {
                    "Content-Type: application/x-www-form-urlencoded\r\n"
                },
                format!("Content-Length: {body_len}\r\n"),
            )
        };

        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n{req_headers}{ct}{extra}\r\n{body_tail}",
            path = parsed.path,
            host = parsed.host,
            // raw 模式 body 走二进制写入（hex 不入流）；文本模式内联
            body_tail = if raw_bytes.is_some() { String::new() } else { body.clone() },
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
        macro_rules! no_resp {
            ($why:expr) => {{
                self.audit
                    .lock()
                    .unwrap()
                    .push(format!("{method} {url} {}", $why));
                return Ok((0, "{}".to_string(), String::new()));
            }};
        }

        let addr = (parsed.host.as_str(), parsed.port)
            .to_socket_addrs()
            .map_err(|e| VmError::IoError(e.to_string()))?
            .next()
            .ok_or_else(|| VmError::IoError("no address resolved".into()))?;

        trait ReadWrite: Read + Write {}
        impl<T: Read + Write> ReadWrite for T {}

        // TLS（https://）：rustls 升级同一 TcpStream；超时在明文 socket 上
        // 设置，读写经由 trait 对象对 Plain/Tls 统一。
        let mut stream: Box<dyn ReadWrite> =
            match TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT) {
                Ok(s) => {
                    let _ = s.set_read_timeout(Some(IO_TIMEOUT));
                    let _ = s.set_write_timeout(Some(IO_TIMEOUT));
                    if parsed.use_tls {
                        let tls = match &self.tls_override {
                            Some(cfg) => {
                                crate::tls::tls_connect_with_config(s, &parsed.host, cfg.clone())
                            }
                            None => crate::tls::tls_connect(s, &parsed.host),
                        };
                        match tls {
                            Ok(t) => Box::new(t),
                            Err(_) => no_resp!("IOERR-TLS"),
                        }
                    } else {
                        Box::new(s)
                    }
                }
                Err(_) => no_resp!("TIMEOUT-CONNECT"),
            };
        let write_res = stream
            .write_all(request.as_bytes())
            .and_then(|_| match &raw_bytes {
                Some(bytes) => stream.write_all(bytes),
                None => Ok(()),
            });
        if write_res.is_err() {
            no_resp!("IOERR-WRITE");
        }
        // TLS 路径：Write 实现只进 rustls 缓冲，必须显式 flush 落 socket
        let _ = stream.flush();
        // Assigned unconditionally below; a `= String::new()` initialiser
        // would just be dead.
        let response;
        {
            // 手写读循环：read_to_string 不容忍 (a) 对端缺 close_notify 的
            // UnexpectedEof（Connection: close 下常见）与 (b) 中途分块的
            // UTF-8 边界。两者都按"已读到此为止"处理。
            let mut chunk = [0u8; 8192];
            let mut raw = Vec::new();
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => raw.extend_from_slice(&chunk[..n]),
                    Err(e)
                        if matches!(
                            e.kind(),
                            ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::UnexpectedEof
                        ) =>
                    {
                        if e.kind() == ErrorKind::UnexpectedEof {
                            break;
                        }
                        no_resp!("TIMEOUT-READ");
                    }
                    Err(_) => no_resp!("IOERR-READ"),
                }
            }
            response = String::from_utf8_lossy(&raw).into_owned();
        }

        let (head, resp_body) = match response.split_once("\r\n\r\n") {
            Some((h, b)) => (h, b.to_string()),
            None => (response.as_str(), String::new()),
        };
        // Transfer-Encoding: chunked — decode before the unit sees the body
        let resp_body = if head
            .to_ascii_lowercase()
            .contains("transfer-encoding: chunked")
        {
            decode_chunked(&resp_body)
        } else {
            resp_body
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

        let hj = if out_headers.is_empty() {
            "{}".to_string()
        } else {
            let items: Vec<String> = out_headers
                .iter()
                .map(|(k, v)| format!("{:?}: {:?}", k, v))
                .collect();
            format!("{{{}}}", items.join(","))
        };
        Ok((status, hj, resp_body))
    }
}

/// L-11 并发会话能力（id 9）。
/// 栈型（http-req-parallel / @9）：
///   `count method url headers-json body -- results-json`
/// 内部对同一请求并发发出 count 份（服务端 latch/竞态窗口探测），
/// **按请求序聚合**（确定性输出），共用 cookie 罐与零出域审计。
/// VM 侧仍是单次调用——不引入语言级并发语义。
pub struct HttpSessionParallelCap {
    pub id: u16,
    pub inner: HttpSessionCap,
}

impl Capability for HttpSessionParallelCap {
    fn id(&self) -> u16 {
        self.id
    }
    fn name(&self) -> &str {
        "http_session_parallel"
    }
    fn description(&self) -> &str {
        "Concurrent fan-out of one request (race/latch probing), indexed aggregation"
    }

    fn call(&self, data_stack: &mut Vec<Value>, args: &[Value]) -> Result<(), VmError> {
        let pop_str = |ds: &mut Vec<Value>, what: &str| -> Result<String, VmError> {
            let v = ds.pop().ok_or(VmError::StackUnderflow {
                expected: 1,
                actual: 0,
            })?;
            match v {
                Value::Str(s) => Ok(s.as_ref().clone()),
                other => Err(VmError::TypeMismatch {
                    expected: what.into(),
                    actual: other.type_name().into(),
                }),
            }
        };
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
        let count = match data_stack.pop() {
            Some(Value::I64(n)) if (1..=16).contains(&n) => n as usize,
            Some(other) => {
                return Err(VmError::TypeMismatch {
                    expected: "count (1..=16)".into(),
                    actual: other.type_name().into(),
                })
            }
            None => {
                return Err(VmError::StackUnderflow {
                    expected: 1,
                    actual: 0,
                })
            }
        };

        // 并发扇出；scoped threads 借用 &self.inner（字段全 Sync）。
        // 结果按线程序（= 请求序）聚合——确定性；线程 panic 视为无响应。
        let joined: Vec<Result<(i64, String, String), VmError>> = std::thread::scope(|scope| {
            let mut handles = Vec::with_capacity(count);
            for _ in 0..count {
                handles.push(scope.spawn(|| {
                    self.inner.request_once(
                        method.clone(),
                        url.clone(),
                        headers_json.clone(),
                        body.clone(),
                    )
                }));
            }
            handles
                .into_iter()
                .map(|h| match h.join() {
                    Ok(r) => r,
                    Err(_) => Ok((0, "{}".to_string(), String::new())),
                })
                .collect()
        });
        // 零出域 denial 等错误必须传播（不被并发聚合吞掉）
        let results: Vec<(i64, String, String)> =
            joined.into_iter().collect::<Result<Vec<_>, _>>()?;

        let items: Vec<String> = results
            .iter()
            .map(|(s, h, b)| format!("{{\"status\":{s},\"headers\":{h},\"body\":{b:?}}}"))
            .collect();
        data_stack.push(Value::Str(Rc::new(format!("[{}]", items.join(",")))));
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

/// hex 串 → 原始字节（L-12 raw body 通道；空白容忍，长度须偶数）
fn decode_hex(s: &str) -> Result<Vec<u8>, String> {
    let t: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    if !t.len().is_multiple_of(2) {
        return Err(format!("http-req-raw: odd hex length {}", t.len()));
    }
    (0..t.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&t[i..i + 2], 16).map_err(|e| format!("http-req-raw: bad hex '{e}'"))
        })
        .collect()
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

/// Decode an HTTP/1.1 chunked body (best-effort: malformed or truncated
/// input yields whatever was decoded so far).
pub fn decode_chunked(body: &str) -> String {
    let mut out: Vec<u8> = Vec::new();
    let mut rest = body.as_bytes();
    while let Some(pos) = find_subslice(rest, b"\r\n") {
        let size_line = String::from_utf8_lossy(&rest[..pos]).to_string();
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_str, 16) else {
            break;
        };
        rest = &rest[pos + 2..];
        if size == 0 {
            break; // terminal chunk (trailers, if any, are dropped)
        }
        if rest.len() < size {
            out.extend_from_slice(rest);
            break; // truncated body — keep what arrived
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size..];
        if rest.starts_with(b"\r\n") {
            rest = &rest[2..];
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[test]
fn test_decode_hex() {
    assert_eq!(
        decode_hex("504b0304").unwrap(),
        vec![0x50, 0x4b, 0x03, 0x04]
    );
    assert_eq!(
        decode_hex("P").unwrap_err(),
        "http-req-raw: odd hex length 1"
    );
    assert!(decode_hex("zz").is_err());
    // 空白容忍
    assert_eq!(decode_hex("50 4b").unwrap(), vec![0x50, 0x4b]);
}

#[cfg(test)]
mod chunked_tests {
    use super::decode_chunked;

    #[test]
    fn single_chunk() {
        let body = "4\r\nWiki\r\n5\r\npedia\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(body), "Wikipedia");
    }

    #[test]
    fn chunk_extension_ignored() {
        let body = "4;ext=1\r\nWiki\r\n0\r\n\r\n";
        assert_eq!(decode_chunked(body), "Wiki");
    }

    #[test]
    fn truncated_keeps_prefix() {
        let body = "A\r\nshort";
        assert_eq!(decode_chunked(body), "short");
    }
}
