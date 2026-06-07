//! whisper repl — Interactive Read-Eval-Print Loop
//!
//! Features: multi-line input, history file, completion helper.

use std::io::{self, BufRead, Write};
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_core::vm::Vm;
use whisper_parser::Parser;

// ── Helpers ──────────────────────────────────────────────────────────

/// Count unmatched braces and brackets in input.
fn unmatched_delimiters(input: &str) -> (usize, usize) {
    let mut braces: i64 = 0;
    let mut brackets: i64 = 0;
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '{' => braces += 1,
            '}' => braces = (braces - 1).max(0),
            '[' => brackets += 1,
            ']' => brackets = (brackets - 1).max(0),
            '"' => {
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    (braces as usize, brackets as usize)
}

fn needs_more(input: &str) -> bool {
    let (b, r) = unmatched_delimiters(input);
    b > 0 || r > 0
}

/// Get the REPL history file path.
fn history_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("WHISPER_HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .or_else(|_| std::env::var("HOME"))
        .ok()?;
    Some(std::path::PathBuf::from(home).join(".whisper_history"))
}

/// Load REPL history from file.
fn load_history() -> Vec<String> {
    if let Some(path) = history_path() {
        if let Ok(file) = std::fs::File::open(&path) {
            io::BufReader::new(file)
                .lines()
                .map_while(Result::ok)
                .collect()
        } else {
            vec![]
        }
    } else {
        vec![]
    }
}

/// Append a line to the history file.
fn append_history(line: &str) {
    if line.trim().is_empty() {
        return;
    }
    if let Some(path) = history_path() {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            let _ = writeln!(file, "{}", line.trim());
        }
    }
}

/// Builtin completions.
fn builtin_completions() -> Vec<&'static str> {
    vec![
        // Stack
        "_",
        "`",
        "drop",
        "rot",
        // Arithmetic
        "+",
        "-",
        "*",
        "/",
        "mod",
        // Comparison
        "=",
        "<",
        ">",
        "!=",
        "<=",
        ">=",
        // Logic
        "&",
        "|",
        "!",
        // List
        "@nth",
        "append",
        "len",
        "@map",
        "@each",
        "@fold",
        "@times",
        // String
        "strlen",
        "strcat",
        "strslice",
        "streq",
        "strlt",
        "strfind",
        "strreplace",
        "strtoi64",
        "i64tostr",
        // Float
        "i64tof64",
        "f64toi64",
        "fsqrt",
        "fsin",
        "fcos",
        "ftan",
        // JSON
        "json-parse",
        "json-stringify",
        // IO
        ".",
        "..",
        ",",
        // Control
        "??",
        "?->",
        "#",
        // Definition
        "import",
        "export",
        // Boolean
        "#t",
        "#f",
        // Confidence
        "?|",
    ]
}

/// Show completions matching a prefix.
fn show_completions(prefix: &str, vm: &Vm) {
    let matches: Vec<String> = builtin_completions()
        .iter()
        .filter(|w| w.starts_with(prefix))
        .map(|s| s.to_string())
        .chain(
            vm.word_dict
                .keys()
                .filter(|k| k.starts_with(prefix))
                .cloned(),
        )
        .collect();

    if matches.is_empty() {
        println!("(no matches for '{prefix}')");
    } else {
        // Show in columns
        let max_len = matches.iter().map(|m| m.len()).max().unwrap_or(0) + 2;
        let term_width = 80;
        let cols = (term_width / max_len).max(1);
        for (i, m) in matches.iter().enumerate() {
            print!("{:<width$}", m, width = max_len);
            if (i + 1) % cols == 0 {
                println!();
            }
        }
        if !matches.len().is_multiple_of(cols) {
            println!();
        }
    }
}

/// Parse a word definition from REPL input.
fn parse_definition(line: &str) -> Result<(String, Vec<whisper_core::opcode::Opcode>), String> {
    let line = line
        .strip_prefix(": ")
        .ok_or("Expected ': name { body } ;'")?;
    let (name, rest) = line
        .split_once(char::is_whitespace)
        .ok_or("Expected word name")?;
    let name = name.trim().to_string();
    let rest = rest.trim();

    let body_str = rest
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix("} ;"))
        .or_else(|| rest.strip_prefix('{').and_then(|s| s.strip_suffix('}')));

    let body_str = body_str.ok_or("Expected { body }")?.trim();

    let ast = Parser::parse_source(body_str)
        .map_err(|e| format!("Parse error in body: {}", e.message))?;
    let mut gen = BytecodeGenerator::new();
    let (bytecode, _defs) = gen.compile(&ast);

    Ok((name, bytecode))
}

// ── REPL ──────────────────────────────────────────────────────────────

pub fn start_repl() -> Result<(), String> {
    println!("╔══════════════════════════════════════╗");
    println!("║   Whisper REPL v1.1.0                ║");
    println!("║   Stack-based, AI-native language    ║");
    println!("╠══════════════════════════════════════╣");
    println!("║ Commands:                            ║");
    println!("║   ..          — show full stack      ║");
    println!("║   .words      — list defined words   ║");
    println!("║   .complete X — show completions     ║");
    println!("║   .history    — show recent history  ║");
    println!("║   .clear      — clear stack          ║");
    println!("║   .exit       — quit REPL            ║");
    println!("║   Unclosed {{}}/[] → continue on next  ║");
    println!("║   Empty line on continuation → cancel║");
    println!("╚══════════════════════════════════════╝");

    let mut history = load_history();
    let mut vm = Vm::new();

    loop {
        let prompt = format!("[{}] > ", vm.data_stack.len());
        print!("{prompt}");
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

        let line = line.trim().to_string();

        if line.is_empty() {
            continue;
        }

        // Multi-line continuation
        let full_line = if needs_more(&line) {
            let mut buf = line;
            let (open_braces, open_brackets) = unmatched_delimiters(&buf);
            let cont_prompt = format!(
                "… {{…{}}} […{}] ",
                if open_braces > 0 {
                    format!(" x{open_braces}")
                } else {
                    String::new()
                },
                if open_brackets > 0 {
                    format!(" x{open_brackets}")
                } else {
                    String::new()
                },
            );
            loop {
                print!("{cont_prompt}");
                io::stdout().flush().map_err(|e| e.to_string())?;

                let mut more = String::new();
                match io::stdin().read_line(&mut more) {
                    Ok(0) => break,
                    Ok(_) => {}
                    Err(_) => break,
                }
                let more = more.trim().to_string();
                if more.is_empty() {
                    // Empty line on continuation → cancel
                    buf.clear();
                    break;
                }
                buf.push(' ');
                buf.push_str(&more);
                if !needs_more(&buf) {
                    break;
                }
            }
            if buf.is_empty() {
                continue; // cancelled
            }
            buf
        } else {
            line
        };

        let full_line = full_line.trim().to_string();
        if full_line.is_empty() {
            continue;
        }

        // Save to history
        append_history(&full_line);
        history.push(full_line.clone());
        if history.len() > 1000 {
            history.remove(0);
        }

        // REPL meta-commands
        let parts: Vec<&str> = full_line.split_whitespace().collect();
        let cmd = parts.first().copied().unwrap_or("");

        match cmd {
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
                if vm.word_dict.is_empty() {
                    println!("  (none)");
                }
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
            ".history" => {
                let start = history.len().saturating_sub(20);
                for (i, h) in history.iter().enumerate().skip(start) {
                    println!("  {i}: {h}");
                }
                continue;
            }
            _ => {}
        }

        // .complete <prefix>
        if cmd == ".complete" {
            let prefix = parts.get(1).copied().unwrap_or("");
            show_completions(prefix, &vm);
            continue;
        }

        // Word definition: : name { body } ;
        if full_line.starts_with(": ") {
            match parse_definition(&full_line) {
                Ok((name, body)) => {
                    vm.define_word(name.clone(), body);
                    println!("Defined: {name}");
                }
                Err(e) => eprintln!("Error: {e}"),
            }
            continue;
        }

        // Regular expression: parse and execute
        match Parser::parse_source(&full_line) {
            Ok(ast) => {
                let mut gen = BytecodeGenerator::new();
                let (bytecode, defs) = gen.compile(&ast);
                for (name, code) in defs {
                    vm.define_word(name, code);
                }
                match vm.execute(&bytecode) {
                    Ok(Some(result)) => println!("→ {result}"),
                    Ok(None) => {
                        if let Some(top) = vm.data_stack.last() {
                            println!("→ {top}");
                        }
                    }
                    Err(e) => eprintln!("Runtime error: {e}"),
                }
            }
            Err(e) => {
                eprintln!("Parse error at line {}: {}", e.token.span.line, e.message);
            }
        }
    }

    println!("Goodbye!");
    Ok(())
}
