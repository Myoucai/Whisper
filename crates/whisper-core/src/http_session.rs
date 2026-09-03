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
                            Some(cfg) => crate::tls::tls_connect_with_config(
                                s,
                                &parsed.host,
                                cfg.clone(),
                            ),
                            None => crate::tls::tls_connect(s, &parsed.host),
                        };
                        match tls {
                            Ok(t) => Box::new(t),
                            Err(_) => no_response!("IOERR-TLS"),
                        }
                    } else {
                        Box::new(s)
                    }
                }
                Err(_) => no_response!("TIMEOUT-CONNECT"),
            };
        if stream.write_all(request.as_bytes()).is_err() {
            no_response!("IOERR-WRITE");
        }
        // TLS 路径：Write 实现只进 rustls 缓冲，必须显式 flush 落 socket
        let _ = stream.flush();
        let mut response = String::new();
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
                            ErrorKind::WouldBlock
                                | ErrorKind::TimedOut
                                | ErrorKind::UnexpectedEof
                        ) =>
                    {
                        if e.kind() == ErrorKind::UnexpectedEof {
                            break;
                        }
                        no_response!("TIMEOUT-READ");
                    }
                    Err(_) => no_response!("IOERR-READ"),
                }
            }
            response = String::from_utf8_lossy(&raw).into_owned();
        }

        let (head, resp_body) = match response.split_once("\r\n\r\n") {
            Some((h, b)) => (h, b.to_string()),
            None => (response.as_str(), String::new()),
        };
        // Transfer-Encoding: chunked — decode before the unit sees the body
        let resp_body = if head.to_ascii_lowercase().contains("transfer-encoding: chunked") {
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


/// Decode an HTTP/1.1 chunked body (best-effort: malformed or truncated
/// input yields whatever was decoded so far).
pub fn decode_chunked(body: &str) -> String {
    let mut out: Vec<u8> = Vec::new();
    let mut rest = body.as_bytes();
    loop {
        let Some(pos) = find_subslice(rest, b"\r\n") else { break };
        let size_line = String::from_utf8_lossy(&rest[..pos]).to_string();
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_str, 16) else { break };
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

#[cfg(test)]
mod tls_tests {
    use super::*;
    use rustls::ServerConfig;
    use std::io::{Read, Write};

    /// Hermetic https round-trip: self-signed rcgen server on localhost,
    /// client config injected via `tls_override`, chunked response body.
    #[test]
    fn https_chunked_round_trip() {
        let key_pair = rcgen::KeyPair::generate().expect("key");
        let mut params = rcgen::CertificateParams::new(vec![]).expect("params");
        params.subject_alt_names = vec![rcgen::SanType::IpAddress(
            std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
        )];
        let server_cert = params.self_signed(&key_pair).expect("self-signed cert");

        // Server: rustls with the self-signed cert, one chunked response.
        let mut server_roots = rustls::RootCertStore::empty();
        server_roots
            .add(server_cert.der().clone())
            .expect("trust anchor");
        let server_cfg = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![server_cert.der().clone()],
                rustls::pki_types::PrivateKeyDer::Pkcs8(
                    rustls::pki_types::PrivatePkcs8KeyDer::from(
                        key_pair.serialize_der(),
                    ),
                ),
            )
            .expect("server cert");

        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (server_done_tx, server_done_rx) = std::sync::mpsc::channel::<()>();
        let server_done_tx = std::sync::Mutex::new(server_done_tx);
        let server_done_tx2 = server_done_tx.lock().unwrap().clone();
        drop(server_done_tx);
        let server = std::thread::spawn(move || {
            let (mut tcp, _) = listener.accept().unwrap();
            let mut conn = rustls::ServerConnection::new(Arc::new(server_cfg)).unwrap();
            // Drive the handshake explicitly (Stream::read alone does not
            // flush server handshake records on this rustls version).
            while conn.is_handshaking() {
                conn.complete_io(&mut tcp).unwrap();
            }
            let mut buf = [0u8; 4096];
            // rustls reader() returns WouldBlock when no plaintext is
            // buffered yet — pull more TLS records until the request lands.
            let n = loop {
                match conn.reader().read(&mut buf) {
                    Ok(k) => break k,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        conn.complete_io(&mut tcp).unwrap();
                    }
                    Err(e) => panic!("server read: {e}"),
                }
            };
            assert!(String::from_utf8_lossy(&buf[..n]).starts_with("GET /x "));
            use std::io::Write as _;
            conn.writer().write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                  Transfer-Encoding: chunked\r\n\r\n\
                  5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
            )
            .unwrap();
            while conn.wants_write() {
                conn.complete_io(&mut tcp).unwrap();
            }
            conn.send_close_notify();
            while conn.wants_write() {
                conn.complete_io(&mut tcp).unwrap();
            }
            let _ = server_done_tx2.send(());
        });

        // Client config trusting only the self-signed cert.
        let mut roots = rustls::RootCertStore::empty();
        roots.add(server_cert.der().clone()).unwrap();
        let mut client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        client_cfg.alpn_protocols = vec![b"http/1.1".to_vec()];

        let cap = HttpSessionCap {
            id: 6,
            allowed_hosts: vec!["127.0.0.1".to_string()],
            jar: Arc::new(Mutex::new(std::collections::HashMap::new())),
            audit: Arc::new(Mutex::new(Vec::new())),
            tls_override: Some(Arc::new(client_cfg)),
        };

        eprintln!("[t] client starting");
        let mut stack: Vec<Value> = Vec::new();
        stack.push(Value::Str(Rc::new("GET".to_string())));
        stack.push(Value::Str(Rc::new(format!("https://127.0.0.1:{port}/x"))));
        stack.push(Value::Str(Rc::new("{}".to_string())));
        cap.call(&mut stack, &[Value::Str(Rc::new(String::new()))])
            .unwrap();
        assert!(
            server_done_rx.recv_timeout(std::time::Duration::from_secs(10)).is_ok(),
            "TLS server thread stuck (handshake/read deadlock)"
        );

        // call pushes (status, headers-json, body)
        let body = match stack.pop() {
            Some(Value::Str(s)) => s.to_string(),
            other => panic!("body: {other:?}"),
        };
        let headers = match stack.pop() {
            Some(Value::Str(s)) => s.to_string(),
            other => panic!("headers: {other:?}"),
        };
        let status = match stack.pop() {
            Some(Value::I64(n)) => n,
            other => panic!("status: {other:?}"),
        };
        if status == 0 {
            panic!(
                "no_response fired: {:?}",
                cap.audit.lock().unwrap().last()
            );
        }
        assert_eq!(status, 200);
        assert_eq!(body, "hello world", "chunked body must be decoded");
        assert!(headers.to_ascii_lowercase().contains("content-type"));
    }
}
