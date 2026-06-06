//! whisper serve — HTTP server with Whisper request handlers
//!
//! Usage: whisper serve <handler.ws> [--port 8080]
//!
//! The handler.ws file defines request handlers as Whisper words.
//! On each HTTP request, the server calls the handler word with the
//! request info on the stack and expects a response to be pushed back.
//!
//! Handler word signature: request -> response
//!   request: [method, path, body] (list of 3 strings)
//!   response: [status, content_type, body] (list of 3 strings)

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::rc::Rc;
use whisper_core::value::Value;
use whisper_core::vm::Vm;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_parser::Parser;

pub fn serve(handler_file: &str, port: u16) -> Result<(), String> {
    let handler_src = std::fs::read_to_string(handler_file)
        .map_err(|e| format!("Cannot read '{handler_file}': {e}"))?;
    let ast = Parser::parse_source(&handler_src)
        .map_err(|e| format!("Parse error: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (bytecode, defs) = gen.compile(&ast);

    let addr = format!("127.0.0.1:{port}");
    let listener = TcpListener::bind(&addr)
        .map_err(|e| format!("Cannot bind to {addr}: {e}"))?;
    println!("Whisper server listening on http://{addr}");
    println!("Handler: {handler_file}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(e) = handle_request(&mut stream, &bytecode, &defs) {
                    eprintln!("Request error: {e}");
                }
            }
            Err(e) => eprintln!("Connection error: {e}"),
        }
    }
    Ok(())
}

fn handle_request(
    stream: &mut TcpStream,
    bytecode: &[whisper_core::opcode::Opcode],
    defs: &std::collections::HashMap<String, Vec<whisper_core::opcode::Opcode>>,
) -> Result<(), String> {
    stream.set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .map_err(|e| e.to_string())?;

    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).map_err(|e| format!("Read error: {e}"))?;
    if n == 0 { return Ok(()); }

    let request = String::from_utf8_lossy(&buf[..n]);
    let (method, path, _body) = parse_http(&request);
    println!("{} {}", method, path);

    let req = Value::List(Rc::new(vec![
        Value::Str(Rc::new(method)),
        Value::Str(Rc::new(path.to_string())),
        Value::Str(Rc::new(String::new())),
    ]));

    let mut vm = Vm::new();
    for (name, code) in defs {
        vm.define_word(name.clone(), code.clone());
    }
    vm.execute(bytecode).map_err(|e| format!("Init: {e}"))?;
    vm.data_stack.push(req);
    let call_handler = [whisper_core::opcode::Opcode::Call("handler".to_string())];
    let handler_result = vm.execute(&call_handler).map_err(|e| format!("Handler: {e}"))?;

    let (status, ct, body) = match handler_result {
        Some(Value::List(items)) if items.len() >= 3 => (
            items[0].to_string().trim_matches('"').to_string(),
            items[1].to_string().trim_matches('"').to_string(),
            items[2].to_string().trim_matches('"').to_string(),
        ),
        other => {
            eprintln!("Handler returned: {:?}", other);
            ("200 OK".into(), "text/plain".into(), "".into())
        }
    };

    let response_text = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        status, ct, body.len(), body
    );
    stream.write_all(response_text.as_bytes()).map_err(|e| e.to_string())?;
    Ok(())
}

fn parse_http(request: &str) -> (String, String, String) {
    let mut lines = request.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let path = parts.next().unwrap_or("/").to_string();

    let body = request.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
    (method, path, body)
}
