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
    // c-native target compiles all whisperc/ files directly
    if target == "c-native" {
        return build_native_compiler(output);
    }

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
        "c" => {
            let defs_vec: Vec<(String, Vec<whisper_core::opcode::Opcode>)> =
                defs.into_iter().collect();
            let c_code = whisper_codegen::transpile_to_c(&bytecode, &defs_vec);
            std::fs::write(output, &c_code).map_err(|e| format!("Failed to write C: {e}"))?;
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("Transpiled {} bytes → {}", size, output);
            println!("Compile with: gcc -O2 -lm -o program {output}");
        }
        "c-whisper" => {
            let defs_vec: Vec<(String, Vec<whisper_core::opcode::Opcode>)> =
                defs.into_iter().collect();
            let c_code = whisper_codegen::transpile_via_whisper(&bytecode, &defs_vec)
                .map_err(|e| format!("Whisper cgen: {e}"))?;
            std::fs::write(output, &c_code).map_err(|e| format!("Failed to write C: {e}"))?;
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("Transpiled (Whisper) {} bytes → {}", size, output);
            println!("Compile with: gcc -O2 -lm -o program {output}");
        }
        "c-native" => {
            // Generate a self-contained native compiler
            // Compile all whisperc/ files and combine into one C program
            let compiler_files = [
                include_str!("../../../../whisperc/lexer.ws"),
                include_str!("../../../../whisperc/classify.ws"),
                include_str!("../../../../whisperc/parser.ws"),
                include_str!("../../../../whisperc/main.ws"),
                include_str!("../../../../whisperc/native/cgen.ws"),
            ];

            let mut all_defs: Vec<(String, Vec<whisper_core::opcode::Opcode>)> = Vec::new();
            let mut all_main_bc: Vec<whisper_core::opcode::Opcode> = Vec::new();

            for src in &compiler_files {
                let ast = whisper_parser::Parser::parse_source(src)
                    .map_err(|e| format!("Parse error: {}", e.message))?;
                let mut gen = whisper_codegen::BytecodeGenerator::new();
                let (bc, defs) = gen.compile(&ast);
                all_main_bc.extend(bc);
                for (name, code) in defs {
                    all_defs.push((name, code));
                }
            }

            let c_code = whisper_codegen::transpile_via_whisper(&all_main_bc, &all_defs)
                .map_err(|e| format!("Whisper cgen: {e}"))?;

            // Build native compiler with driver main()
            let mut native_code = String::new();
            native_code.push_str("// Whisper Native Compiler — Self-Hosting\n");
            native_code.push_str("// Generated by: whisper build --target c-native\n\n");
            native_code.push_str(&c_code);
            native_code.push_str("\n// Compiler driver\n");
            native_code.push_str("static void w_compile_source(void) {\n");
            native_code.push_str("  // Read source from stdin\n");
            native_code.push_str("  w_push((wval)w_read_line());\n");
            native_code.push_str("  // Classify: tokenize → structify → classify\n");
            native_code.push_str("  w_classify_full();\n");
            native_code.push_str("  // Split definitions\n");
            native_code.push_str("  w_split_defs();\n");
            native_code.push_str("  // Compile main tokens\n");
            native_code.push_str("  w_compile();\n");
            native_code.push_str("  // Output bytecodes (for now)\n");
            native_code.push_str("  w_print_val(w_pop());\n");
            native_code.push_str("}\n\n");
            native_code.push_str("int main(int argc, char**argv) {\n");
            native_code.push_str("  w_init(argc, argv);\n");
            native_code.push_str("  w_compile_source();\n");
            native_code.push_str("  return 0;\n");
            native_code.push_str("}\n");

            std::fs::write(output, &native_code).map_err(|e| format!("Failed to write C: {e}"))?;
            let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
            println!("Native compiler {} bytes → {}", size, output);
            println!("Compile with: gcc -O2 -lm -o whisper {output}");
            println!("Usage: echo '42 .' | ./whisper");
        }
        other => {
            return Err(format!("Unknown target: {other}. Supported: wbin, native, c"));
        }
    }

    Ok(())
}

/// Build a self-contained native Whisper compiler.
/// Architecture: Rust generates C code for the full pipeline,
/// Whisper compiler handles bytecode generation.
fn build_native_compiler(output: &str) -> Result<(), String> {
    // Load the Whisper compiler (main.ws — bytecode compilation)
    let compiler_src = include_str!("../../../../whisperc/main.ws");
    let compiler_ast = Parser::parse_source(compiler_src)
        .map_err(|e| format!("whisperc parse: {}", e.message))?;
    let mut cgen = BytecodeGenerator::new();
    let (compiler_bc, compiler_defs) = cgen.compile(&compiler_ast);

    // Transpile the Whisper compiler to C
    let compiler_defs_vec: Vec<(String, Vec<whisper_core::opcode::Opcode>)> =
        compiler_defs.into_iter().collect();
    let mut c_code = whisper_codegen::transpile_to_c(&compiler_bc, &compiler_defs_vec);

    // Remove the generated main() (we add our own driver)
    if let Some(pos) = c_code.rfind("\nint main(") {
        c_code.truncate(pos);
    }

    // Build the complete native compiler
    let mut native_code = String::new();
    native_code.push_str("// Whisper Native Compiler — Self-Hosting\n\n");
    native_code.push_str(&c_code);
    native_code.push_str("\n");
    // Include the native driver (C tokenizer + structifier + classifier)
    native_code.push_str(include_str!("../native_driver.c"));

    std::fs::write(output, &native_code).map_err(|e| format!("Failed to write C: {e}"))?;
    let size = std::fs::metadata(output).map(|m| m.len()).unwrap_or(0);
    println!("Native compiler {} bytes → {}", size, output);
    println!("Compile with: gcc -O2 -lm -o whisper {output}");
    println!("Usage: echo '42 .' | ./whisper");
    Ok(())
}
