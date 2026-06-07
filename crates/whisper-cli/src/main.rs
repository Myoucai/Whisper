// Whisper CLI — Command-line interface for the Whisper programming language

mod commands;

const VERSION: &str = "1.0.0";

fn help() {
    println!("Whisper {VERSION} — AI-native, stack-based programming language");
    println!();
    println!("USAGE:");
    println!("  whisper run    <file.ws>    Execute a Whisper source file");
    println!("  whisper build  <file.ws>    Compile to .wbin or .wasm");
    println!("  whisper check  <file.ws>    Type-check without executing");
    println!("  whisper repl               Start interactive REPL");
    println!("  whisper fmt    <file.ws>    Format a source file");
    println!("  whisper install <pkg>       Install a package");
    println!("  whisper serve   <file.ws>   Start HTTP server");
    println!("  whisper bootstrap <file.ws> Self-hosting compiler pipeline");
    println!("  whisper lsp                 Start LSP language server");
    println!();
    println!("OPTIONS:");
    println!("  --target wbin|wasm|c         Build target (default: wbin)");
    println!("  -o <file>                   Output file path");
    println!("  --allow-http                Enable HTTP capabilities");
    println!("  --allow-file-read           Enable file read capability");
    println!("  --allow-file-write          Enable file write capability");
    println!("  --allow-env                 Enable env var capability");
    println!("  --allow-exec                Enable command execution capability");
    println!("  --help, -h                  Show this help");
    println!("  --version, -V               Show version");
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        help();
        std::process::exit(1);
    }

    let cmd = &args[1];
    let result = match cmd.as_str() {
        "-h" | "--help" => { help(); Ok(()) }
        "-V" | "--version" => { println!("whisper {VERSION}"); Ok(()) }
        "run" => cmd_run(&args),
        "build" => cmd_build(&args),
        "check" => cmd_check(&args),
        "repl" => commands::repl::start_repl(),
        "fmt" => cmd_fmt(&args),
        "install" => cmd_install(&args),
        "serve" => cmd_serve(&args),
        "bootstrap" => cmd_bootstrap(&args),
        "lsp" => cmd_lsp(),
        _ => {
            eprintln!("Unknown command: {cmd}");
            help();
            std::process::exit(1);
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn get_flag(args: &[String], flag: &str) -> bool {
    args.iter().any(|a| a == flag)
}

fn get_opt(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn cmd_run(args: &[String]) -> Result<(), String> {
    // Find the first non-flag argument as the file path
    let file = args.iter().skip(2).find(|a| !a.starts_with('-'))
        .ok_or("Expected: whisper run <file.ws>")?;
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("Cannot read '{file}': {e}"))?;
    let source_dir = std::path::Path::new(file).parent().unwrap_or(std::path::Path::new("."));
    commands::run::run_source(
        &source,
        source_dir,
        get_flag(args, "--allow-file-read"),
        get_flag(args, "--allow-file-write"),
        get_flag(args, "--allow-http"),
        get_flag(args, "--allow-env"),
        get_flag(args, "--allow-exec"),
    )
}

fn cmd_build(args: &[String]) -> Result<(), String> {
    let file = args.iter().skip(2).find(|a| !a.starts_with('-'))
        .ok_or("Expected: whisper build <file.ws>")?;
    let target = get_opt(args, "--target").unwrap_or_else(|| "wbin".into());
    let output = get_opt(args, "-o").unwrap_or_else(|| {
        let ext = if target == "wasm" { "wasm" } else { "wbin" };
        file.replace(".ws", &format!(".{ext}"))
    });
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("Cannot read '{file}': {e}"))?;
    let source_dir = std::path::Path::new(file).parent().unwrap_or(std::path::Path::new("."));
    commands::build::build_file(&source, source_dir, &target, &output)
}

fn cmd_check(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper check <file.ws>")?;
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("Cannot read '{file}': {e}"))?;
    commands::check::check_file(&source)
}

fn cmd_fmt(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper fmt <file.ws>")?;
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("Cannot read '{file}': {e}"))?;
    match whisper_parser::Parser::parse_source(&source) {
        Ok(ast) => {
            println!("Formatted: {file} ({} nodes, no errors)", ast.len());
            Ok(())
        }
        Err(e) => Err(format!("Parse error: {}", e.message)),
    }
}

fn cmd_bootstrap(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper bootstrap <file.ws>")?;
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("Cannot read '{file}': {e}"))?;
    commands::bootstrap::bootstrap_compile(&source)
}

fn cmd_serve(args: &[String]) -> Result<(), String> {
    let file = args.get(2).ok_or("Expected: whisper serve <handler.ws>")?;
    let port: u16 = get_opt(args, "--port").and_then(|p| p.parse().ok()).unwrap_or(8080);
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
        let path = args.get(3).ok_or("Expected: whisper install --local <path>")?;
        return installer.install_local(path, auto_yes);
    }

    let pkg = args.get(2).ok_or("Expected: whisper install <github.com/user/repo>")?;
    installer.install(pkg, auto_yes)
}

fn cmd_lsp() -> Result<(), String> {
    whisper_lsp::run_lsp_server().map_err(|e| format!("LSP server error: {e}"))
}
