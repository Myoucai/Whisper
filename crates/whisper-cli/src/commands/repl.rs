//! whisper repl — Interactive Read-Eval-Print Loop

use std::io::{self, Write};

use whisper_core::vm::Vm;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_parser::Parser;

/// Start the interactive REPL.
pub fn start_repl() -> Result<(), String> {
    println!("╔══════════════════════════════════════╗");
    println!("║   Whisper REPL v0.1.0                ║");
    println!("║   Stack-based, AI-native language    ║");
    println!("╠══════════════════════════════════════╣");
    println!("║ Commands:                            ║");
    println!("║   ..        — show full stack        ║");
    println!("║   .words    — list defined words     ║");
    println!("║   .clear    — clear stack            ║");
    println!("║   .exit     — quit REPL              ║");
    println!("╚══════════════════════════════════════╝");

    let mut vm = Vm::new();

    loop {
        print!("> ");
        io::stdout().flush().map_err(|e| e.to_string())?;

        let mut line = String::new();
        match io::stdin().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error reading input: {e}");
                continue;
            }
        }

        let line = line.trim();

        if line.is_empty() {
            continue;
        }

        // REPL meta-commands
        match line {
            ".exit" | ".quit" => break,
            ".." => {
                println!("Stack ({} items):", vm.data_stack.len());
                for (i, val) in vm.data_stack.iter().rev().enumerate() {
                    println!("  [{i}] {val}");
                }
                continue;
            }
            ".words" => {
                println!("Defined words:");
                for name in vm.word_dict.keys() {
                    println!("  {name}");
                }
                continue;
            }
            ".clear" => {
                vm.data_stack.clear();
                println!("Stack cleared.");
                continue;
            }
            _ => {}
        }

        // Word definition: : name { body } ;
        if line.starts_with(": ") {
            match parse_definition(line) {
                Ok((name, body)) => {
                    vm.define_word(name.clone(), body);
                    println!("Defined: {name}");
                }
                Err(e) => eprintln!("Error: {e}"),
            }
            continue;
        }

        // Regular expression: parse and execute
        match Parser::parse_source(line) {
            Ok(ast) => {
                let mut gen = BytecodeGenerator::new();
                let (bytecode, defs) = gen.compile(&ast);
                for (name, code) in defs {
                    vm.define_word(name, code);
                }
                match vm.execute(&bytecode) {
                    Ok(Some(result)) => {
                        println!("→ {result}");
                    }
                    Ok(None) => {
                        // No result on stack — show current stack top
                        if let Some(top) = vm.data_stack.last() {
                            println!("→ {top}");
                        }
                    }
                    Err(e) => eprintln!("Runtime error: {e}"),
                }
            }
            Err(e) => {
                eprintln!(
                    "Parse error at line {}: {}",
                    e.token.span.line, e.message
                );
            }
        }
    }

    println!("Goodbye!");
    Ok(())
}

/// Parse a word definition from REPL input.
fn parse_definition(line: &str) -> Result<(String, Vec<whisper_core::opcode::Opcode>), String> {
    let line = line.strip_prefix(": ").ok_or("Expected ': name { body } ;'")?;
    let (name, rest) = line.split_once(char::is_whitespace)
        .ok_or("Expected word name")?;
    let name = name.trim().to_string();
    let rest = rest.trim();

    // Extract body between { and }
    let body_str = rest
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix("} ;"))
        .or_else(|| rest.strip_prefix('{').and_then(|s| s.strip_suffix('}')));

    let body_str = body_str.ok_or("Expected { body }")?.trim();

    // Parse body and compile to bytecode
    let ast = Parser::parse_source(body_str)
        .map_err(|e| format!("Parse error in body: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (bytecode, _defs) = gen.compile(&ast);

    Ok((name, bytecode))
}
