/// whisper build — Compile .ws to .wbin or .wasm

use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_codegen::wbin::WbinWriter;
use whisper_parser::Parser;

/// Build a Whisper source file to the specified target format.
pub fn build_file(source: &str, target: &str, output: &str) -> Result<(), String> {
    // Phase 1: Parse
    let ast = Parser::parse_source(source).map_err(|e| {
        format!("Parse error at {}:{}: {}", e.token.span.line, e.token.span.column, e.message)
    })?;

    // Phase 2: Compile to bytecode
    let mut gen = BytecodeGenerator::new();
    let (bytecode, _defs) = gen.compile(&ast);

    // Phase 3: Output in target format
    match target {
        "wbin" => {
            let output_path = std::path::Path::new(output);
            WbinWriter::write_to_file(&bytecode, output_path)
                .map_err(|e| format!("Failed to write .wbin: {e}"))?;
            let size = std::fs::metadata(output_path)
                .map(|m| m.len())
                .unwrap_or(0);
            println!("Compiled {} bytes → {}", size, output);
        }
        "wasm" => {
            let wasm = whisper_codegen::compile_direct(&bytecode);
            std::fs::write(output, wasm)
                .map_err(|e| format!("Failed to write WASM: {e}"))?;
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("Compiled {} bytes → {}", size, output);
        }
        other => {
            return Err(format!(
                "Unknown target: {other}. Supported: wbin, wasm"
            ));
        }
    }

    Ok(())
}
