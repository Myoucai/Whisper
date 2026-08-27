//! whisper build — Compile .ws to .wbin bytecode or C source

use std::path::Path;
use whisper_codegen::bytecode_gen::BytecodeGenerator;
use whisper_codegen::wbin::WbinWriter;
use whisper_parser::Parser;

/// Build a Whisper source file to the specified target format.
pub fn build_file(
    source: &str,
    source_dir: &Path,
    target: &str,
    output: &str,
) -> Result<(), String> {
    // Phase 1: Parse
    let ast = Parser::parse_source(source).map_err(|e| {
        format!(
            "Parse error at {}:{}: {}",
            e.token.span.line, e.token.span.column, e.message
        )
    })?;

    // Phase 1a: Resolve imports
    let resolved = whisper_parser::resolve_imports(ast, source_dir)
        .map_err(|e| format!("Import error: {e}"))?;
    let ast = resolved.ast;

    // Phase 2: Compile to bytecode
    let mut gen = BytecodeGenerator::new();
    let (bytecode, defs) = gen
        .compile(&ast)
        .map_err(|e| format!("Compile error: {}", whisper_codegen::block_ir::contract_errors_to_string(&e)))?;

    // Phase 3: Output in target format
    match target {
        "wbin" => {
            // v2 block format: main + defs as block programs, no jump
            // offsets on disk — the loader (whisper run *.wbin)
            // linearizes. (Flat-code optimization is skipped here: the
            // blocks are the source of truth and B4 optimizes them at
            // the graph level before linearization.)
            let unit = gen
                .compile_to_unit(&ast)
                .map_err(|e| format!("Compile error: {}", whisper_codegen::block_ir::contract_errors_to_string(&e)))?;
            let output_path = std::path::Path::new(output);
            WbinWriter::write_unit_to_file(&unit.main, &unit.defs, output_path)
                .map_err(|e| format!("Failed to write .wbin: {e}"))?;
            let size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
            println!("Compiled {size} bytes (v2 blocks, {} defs) → {output}", unit.defs.len());
        }
        "c" => {
            // Phase 2b: Optimize bytecode (flat-code target)
            let bytecode = whisper_codegen::optimize(&bytecode);
            let defs: Vec<_> = defs
                .into_iter()
                .map(|(k, v)| (k, whisper_codegen::optimize(&v)))
                .collect();
            let c_code = whisper_codegen::transpile_to_c(&bytecode, &defs);
            std::fs::write(output, &c_code).map_err(|e| format!("Failed to write C: {e}"))?;
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("Transpiled {} bytes → {}", size, output);
            println!("Compile with: gcc -O2 -lm -o program {output}");
        }
        "c2" => {
            // Self-hosted chain: the whisperc compiler runs on the Rust VM
            // and emits C; the system compiler produces the executable.
            super::c2::build_c2(source, source_dir, output, true)?;
        }
        other => {
            return Err(format!(
                "Unknown target: {other}. Supported: wbin, c, c2 (self-hosted chain)"
            ));
        }
    }

    Ok(())
}
