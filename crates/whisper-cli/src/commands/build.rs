//! whisper build — Compile .ws to .wbin or native ELF

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
    let (bytecode, defs) = gen.compile(&ast);

    // Phase 2b: Optimize bytecode
    let bytecode = whisper_codegen::optimize(&bytecode);
    let defs: Vec<_> = defs
        .into_iter()
        .map(|(k, v)| (k, whisper_codegen::optimize(&v)))
        .collect();

    // Phase 3: Output in target format
    match target {
        "wbin" => {
            let output_path = std::path::Path::new(output);
            WbinWriter::write_to_file(&bytecode, output_path)
                .map_err(|e| format!("Failed to write .wbin: {e}"))?;
            let size = std::fs::metadata(output_path).map(|m| m.len()).unwrap_or(0);
            println!("Compiled {} bytes → {}", size, output);
        }
        "native" | "elf" => {
            let elf = whisper_codegen::compile_to_native(&bytecode, &defs);
            std::fs::write(output, &elf).map_err(|e| format!("Failed to write ELF: {e}"))?;
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("Compiled {} bytes → {}", size, output);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(output,
                    std::fs::Permissions::from_mode(0o755))
                    .map_err(|e| format!("chmod: {e}"))?;
            }
            println!("Run: ./{output}");
        }
        other => {
            return Err(format!("Unknown target: {other}. Supported: wbin, native"));
        }
    }

    Ok(())
}
