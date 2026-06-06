/// whisper serve — HTTP server with Whisper request handlers
///
/// Usage: whisper serve <handler.ws> [--port 8080]
///
/// The handler.ws file defines request handlers as Whisper words.
/// On each HTTP request, the server calls the handler word with the
/// request info on the stack and expects a response to be pushed back.
///
/// Handler word signature: request -> response
///   request: [method, path, body] (list of 3 strings)
///   response: [status, content_type, body] (list of 3 strings)

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
    let mut buf = [0u8; 4096];
    let n = stream.read(&mut buf).map_err(|e| e.to_string())?;
    if n == 0 { return Ok(()); }

    let request = String::from_utf8_lossy(&buf[..n]);
    let (method, path, body) = parse_http(&request);

    // Create request value: [method, path, body]
    let req = Value::List(Rc::new(vec![
        Value::Str(Rc::new(method)),
        Value::Str(Rc::new(path.to_string())),
        Value::Str(Rc::new(body)),
    ]));

    // Execute handler
    let mut vm = Vm::new();
    for (name, code) in defs {
        vm.define_word(name.clone(), code.clone());
    }
    // Run main bytecode to register definitions
    vm.execute(bytecode).map_err(|e| format!("Handler init: {e}"))?;
    // Call handler word with request
    vm.data_stack.push(req);
    let call_handler = [whisper_core::opcode::Opcode::Call("handler".to_string())];
    vm.execute(&call_handler).map_err(|e| format!("Handler error: {e}"))?;

    // Get response
    let response = match vm.data_stack.pop() {
        Some(Value::List(items)) if items.len() >= 3 => {
            let status = match &items[0] { Value::Str(s) => s.as_ref().clone(), _ => "200 OK".into() };
            let ct = match &items[1] { Value::Str(s) => s.as_ref().clone(), _ => "text/plain".into() };
            let body = match &items[2] { Value::Str(s) => s.as_ref().clone(), _ => String::new() };
            (status, ct, body)
        }
        _ => ("200 OK".into(), "text/plain".into(), "".into()),
    };

    // Write HTTP response
    let response_text = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.0, response.1, response.2.len(), response.2
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
