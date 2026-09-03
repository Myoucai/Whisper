// Whisper CLI — Command-line interface for the Whisper programming language

mod commands;

const VERSION: &str = "1.0.0";

fn help() {
    println!("Whisper {VERSION} — AI-native, stack-based programming language");
    println!();
    println!("USAGE:");
    println!("  whisper run    <file.ws>    Execute a Whisper source file");
    println!("  whisper build  <file.ws>    Compile to .wbin bytecode or C source");
    println!("  whisper check  <file.ws>    Type-check without executing");
    println!("  whisper repl               Start interactive REPL");
    println!("  whisper fmt    <file.ws>    Format a source file");
    println!("  whisper install <pkg>       Install a package");
    println!("  whisper uninstall <name>    Remove a package");
    println!("  whisper search [query]      Search installed packages");
    println!("  whisper serve   <file.ws>   Start HTTP server");
    println!("  whisper bootstrap <file.ws> Self-hosting compiler pipeline");
    println!("  whisper lsp                 Start LSP language server");
    println!("  whisper test   [path]       Run test files (*_test.ws)");
    println!();
    println!("OPTIONS:");
    println!(
        "  --target wbin|c|c2|native|exe Build target (default: wbin; c2 = self-hosted chain + gcc;
                                native = c2 + -O3 -march=native -flto, content-cached;
                                exe = self-contained single-file executable)"
    );
    println!("  -o <file>                   Output file path");
    println!("  --allow-http                Enable HTTP capabilities");
    println!("  --allow-file-read           Enable file read capability");
    println!("  --allow-file-write          Enable file write capability");
    println!("  --allow-env                 Enable env var capability");
    println!("  --allow-exec                Enable command execution capability");
    println!("  --trace                     Enable VM execution tracing");
    println!("  --help, -h                  Show this help");
    println!("  --version, -V               Show version");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // Deeply nested sources (long elif chains, nested quotes) recurse through
    // the lexer, parser, type checker and code generator. Run all command
    // work on a large-stack thread so compile-time recursion is bounded by
    // the source, not the default 8 MB main-thread stack.
    let child = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || run_cli(args))
        .expect("failed to spawn CLI worker thread");
    let result = child
        .join()
        .unwrap_or_else(|_| Err("internal panic".into()));
    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run_cli(args: Vec<String>) -> Result<(), String> {
    // Self-contained app mode (`whisper build -t exe`): if this binary has a
    // wbin payload appended after its own image (trailer: payload_len u64 LE +
    // "WSHPEXE1"), it IS the app — run the embedded payload, ignore CLI args
    // except capability flags. The original whisper.exe carries no trailer.
    if let Some(payload) = embedded_app_payload() {
        let config = commands::run::RunConfig {
            allow_file_read: get_flag(&args, "--allow-file-read"),
            allow_file_write: get_flag(&args, "--allow-file-write"),
            allow_http: get_flag(&args, "--allow-http"),
            allow_env: get_flag(&args, "--allow-env"),
            allow_exec: get_flag(&args, "--allow-exec"),
            trace: get_flag(&args, "--trace"),
        };
        return commands::run::run_wbin(&payload, &config);
    }

    if args.len() < 2 {
        help();
        std::process::exit(1);
    }

    let cmd = &args[1];
    match cmd.as_str() {
        "-h" | "--help" => {
            help();
            Ok(())
        }
        "-V" | "--version" => {
            println!("whisper {VERSION}");
            Ok(())
        }
        "run" => cmd_run(&args),
        "build" => cmd_build(&args),
        "check" => cmd_check(&args),
        "repl" => commands::repl::start_repl(),
        "fmt" => cmd_fmt(&args),
        "install" => cmd_install(&args),
        "uninstall" => cmd_uninstall(&args),
        "search" => cmd_search(&args),
        "serve" => cmd_serve(&args),
        "bootstrap" => cmd_bootstrap(&args),
        "lsp" => cmd_lsp(),
        "test" => cmd_test(&args),
        _ => {
            eprintln!("Unknown command: {cmd}");
            help();
            std::process::exit(1);
        }
    }
}

fn get_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

/// Trailer layout appended by `whisper build -t exe`:
///   [ wbin payload bytes ][ payload_len: u64 LE ][ b"WSHPEXE1" ]
/// Returns the payload slice when this executable carries one.
fn embedded_app_payload() -> Option<Vec<u8>> {
    const MAGIC: &[u8; 8] = b"WSHPEXE1";
    let data = std::fs::read(std::env::current_exe().ok()?).ok()?;
    if data.len() < 8 + 8 {
        return None;
    }
    let n = data.len();
    if &data[n - 8..] != MAGIC {
        return None;
    }
    let mut len_bytes = [0u8; 8];
    len_bytes.copy_from_slice(&data[n - 16..n - 8]);
    let payload_len = u64::from_le_bytes(len_bytes) as usize;
    if payload_len + 16 > n {
        return None;
    }
    Some(data[n - 16 - payload_len..n - 16].to_vec())
}

fn get_opt(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn cmd_run(args: &[String]) -> Result<(), String> {
    let file = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with('-'))
        .ok_or("Expected: whisper run <file.ws>")?;
    let config = commands::run::RunConfig {
        allow_file_read: get_flag(args, "--allow-file-read"),
        allow_file_write: get_flag(args, "--allow-file-write"),
        allow_http: get_flag(args, "--allow-http"),
        allow_env: get_flag(args, "--allow-env"),
        allow_exec: get_flag(args, "--allow-exec"),
        trace: get_flag(args, "--trace"),
    };
    // .wbin: decode + linearize (v2 blocks) or run flat (v1 legacy)
    if file.ends_with(".wbin") {
        let data = std::fs::read(file).map_err(|e| format!("Cannot read '{file}': {e}"))?;
        return commands::run::run_wbin(&data, &config);
    }
    let source = std::fs::read_to_string(file).map_err(|e| format!("Cannot read '{file}': {e}"))?;
    let source_dir = std::path::Path::new(file)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    commands::run::run_source(&source, source_dir, &config)
}

fn cmd_build(args: &[String]) -> Result<(), String> {
    // Accept both the long and short spelling of the target flag.
    let target = get_opt(args, "--target")
        .or_else(|| get_opt(args, "-t"))
        .unwrap_or_else(|| "wbin".into());
    let output = get_opt(args, "-o");
    // Find the file argument: skip flag values (--target X, -o X)
    let output_val = output.as_deref();
    let file = args
        .iter()
        .skip(2)
        .filter(|a| !a.starts_with('-'))
        .filter(|a| Some(a.as_str()) != output_val) // exclude -o value
        .find(|a| a.ends_with(".ws") || a.ends_with(".whisper"))
        .ok_or("Expected: whisper build <file.ws>")?;
    let ext = match target.as_str() {
        "c" | "c2" => "c",
        _ => "wbin",
    };
    let output = output.unwrap_or_else(|| file.replace(".ws", &format!(".{ext}")));
    let source = std::fs::read_to_string(file).map_err(|e| format!("Cannot read '{file}': {e}"))?;
    let source_dir = std::path::Path::new(file)
        .parent()
        .unwrap_or(std::path::Path::new("."));
    commands::build::build_file(&source, source_dir, &target, &output)
}

fn cmd_check(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper check <file.ws>")?;
    let source = std::fs::read_to_string(file).map_err(|e| format!("Cannot read '{file}': {e}"))?;
    commands::check::check_file(&source)
}

fn cmd_fmt(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper fmt <file.ws>")?;
    let source = std::fs::read_to_string(file).map_err(|e| format!("Cannot read '{file}': {e}"))?;
    let ast = whisper_parser::Parser::parse_source(&source)
        .map_err(|e| format!("Parse error: {}", e.message))?;
    let formatted = whisper_codegen::formatter::format_ast(&ast);
    std::fs::write(file, &formatted).map_err(|e| format!("Cannot write '{file}': {e}"))?;
    println!("Formatted: {file} ({} nodes)", ast.len());
    Ok(())
}

fn cmd_bootstrap(args: &[String]) -> Result<(), String> {
    let file = args
        .iter()
        .skip(2)
        .find(|a| !a.starts_with('-'))
        .ok_or("Expected: whisper bootstrap <file.ws>")?;
    let source = std::fs::read_to_string(file).map_err(|e| format!("Cannot read '{file}': {e}"))?;
    if get_flag(args, "--hard") {
        commands::bootstrap::hard_bootstrap_compile(&source)
    } else {
        commands::bootstrap::bootstrap_compile(&source)
    }
}

fn cmd_serve(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper serve <handler.ws>")?;
    let port: u16 = get_opt(args, "--port")
        .and_then(|p| p.parse().ok())
        .unwrap_or(8080);
    commands::serve::serve(file, port)
}

fn cmd_install(args: &[String]) -> Result<(), String> {
    if get_flag(args, "--list") || get_flag(args, "-l") {
        let installer = whisper_package::install::Installer::new();
        let packages = installer.list()?;
        if packages.is_empty() {
            println!("No packages installed.");
        } else {
            println!("Installed packages:");
            for pkg in &packages {
                println!("  {} v{}", pkg.name, pkg.version);
            }
        }
        return Ok(());
    }

    let auto_yes = get_flag(args, "-y") || get_flag(args, "--yes");
    let mut installer = whisper_package::install::Installer::new();

    if get_flag(args, "--local") {
        let path = args
            .get(3)
            .ok_or("Expected: whisper install --local <path>")?;
        return installer.install_local(path, auto_yes);
    }

    let pkg = args
        .get(2)
        .ok_or("Expected: whisper install <github.com/user/repo>")?;
    installer.install(pkg, auto_yes)
}

fn cmd_uninstall(args: &[String]) -> Result<(), String> {
    let name = args.get(2).ok_or("Expected: whisper uninstall <name>")?;
    let installer = whisper_package::install::Installer::new();
    installer.uninstall(name)
}

fn cmd_search(args: &[String]) -> Result<(), String> {
    let query = args.get(2).map(|s| s.as_str()).unwrap_or("");
    let installer = whisper_package::install::Installer::new();
    let packages = installer.list()?;
    let filtered: Vec<_> = packages
        .iter()
        .filter(|p| query.is_empty() || p.name.contains(query))
        .collect();
    if filtered.is_empty() {
        let suffix = if query.is_empty() {
            String::new()
        } else {
            format!(" matching '{query}'")
        };
        println!("No packages found{suffix}");
    } else {
        println!("Installed packages:");
        for pkg in &filtered {
            println!("  {} v{}", pkg.name, pkg.version);
            if !pkg.manifest.exports.is_empty() {
                println!("    exports: {}", pkg.manifest.exports.join(", "));
            }
        }
    }
    Ok(())
}

fn cmd_lsp() -> Result<(), String> {
    whisper_lsp::run_lsp_server().map_err(|e| format!("LSP server error: {e}"))
}

fn cmd_test(args: &[String]) -> Result<(), String> {
    use std::path::Path;

    let path = args.get(2).map(|s| s.as_str()).unwrap_or(".");
    let test_files: Vec<std::path::PathBuf> = if path.ends_with(".ws") {
        vec![std::path::PathBuf::from(path)]
    } else {
        // Find all *_test.ws files in the directory
        let dir = Path::new(path);
        if !dir.is_dir() {
            return Err(format!("Not a directory: {path}"));
        }
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .map_err(|e| format!("Cannot read directory '{path}': {e}"))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension().map(|e| e == "ws").unwrap_or(false)
                    && p.file_name()
                        .map(|n| n.to_string_lossy().ends_with("_test.ws"))
                        .unwrap_or(false)
            })
            .collect();
        files.sort();
        files
    };

    if test_files.is_empty() {
        println!("No test files found in '{path}'");
        return Ok(());
    }

    let config = commands::run::RunConfig {
        allow_file_read: get_flag(args, "--allow-file-read"),
        allow_file_write: get_flag(args, "--allow-file-write"),
        allow_http: get_flag(args, "--allow-http"),
        allow_env: get_flag(args, "--allow-env"),
        allow_exec: get_flag(args, "--allow-exec"),
        trace: false,
    };

    let mut passed = 0;
    let mut failed = 0;

    for file in &test_files {
        let name = file.file_name().unwrap().to_string_lossy();
        let source = std::fs::read_to_string(file)
            .map_err(|e| format!("Cannot read '{}': {e}", file.display()))?;
        let source_dir = file.parent().unwrap_or(Path::new("."));

        print!("  {name} ... ");
        match commands::run::run_source(&source, source_dir, &config) {
            Ok(()) => {
                println!("ok");
                passed += 1;
            }
            Err(e) => {
                println!("FAILED");
                eprintln!("    {e}");
                failed += 1;
            }
        }
    }

    println!();
    println!("test result: {passed} passed, {failed} failed");
    if failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
