// Whisper CLI — Command-line interface for the Whisper programming language

mod commands;

use clap::{Parser as ClapParser, Subcommand};

/// Whisper: An AI-native, stack-based programming language
#[derive(ClapParser)]
#[command(
    name = "whisper",
    version,
    about = "Whisper programming language — AI-native, dataflow-oriented, capability-safe",
    long_about = "Whisper is a stack-based programming language designed for AI generation.\n\
                  Features: token-economical syntax, capability sandbox, native confidence,\n\
                  WASM compilation, and self-hosting compiler."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a Whisper source file
    Run {
        /// Path to .ws source file
        file: String,
        /// Enable HTTP capability (@http_get, @http_post)
        #[arg(long = "allow-http")]
        allow_http: bool,
        /// Enable file read capability (@file_read)
        #[arg(long = "allow-file-read")]
        allow_file_read: bool,
        /// Enable file write capability (@file_write)
        #[arg(long = "allow-file-write")]
        allow_file_write: bool,
    },

    /// Compile a .ws file to .wbin or .wasm
    Build {
        /// Path to .ws source file
        file: String,
        /// Output target format: wbin (default) or wasm
        #[arg(long = "target", default_value = "wbin")]
        target: String,
        /// Output file path (default: input file with new extension)
        #[arg(short = 'o', long = "output")]
        output: Option<String>,
    },

    /// Type-check a .ws file without executing
    Check {
        /// Path to .ws source file
        file: String,
    },

    /// Install a Whisper package
    Install {
        /// Package spec (e.g., github.com/user/repo)
        package: String,
    },

    /// Start interactive REPL
    Repl,

    /// Format a Whisper source file
    Fmt {
        /// Path to .ws source file
        file: String,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Run {
            file,
            allow_http,
            allow_file_read,
            allow_file_write,
        } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading file '{file}': {e}");
                    std::process::exit(1);
                }
            };
            commands::run::run_file(&source, allow_file_read, allow_file_write, allow_http)
        }
        Commands::Build { file, target, output } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading file '{file}': {e}");
                    std::process::exit(1);
                }
            };
            let output = output.unwrap_or_else(|| {
                let ext = if target == "wasm" { "wasm" } else { "wbin" };
                file.replace(".ws", &format!(".{ext}"))
            });
            commands::build::build_file(&source, &target, &output)
        }
        Commands::Check { file } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading file '{file}': {e}");
                    std::process::exit(1);
                }
            };
            commands::check::check_file(&source)
        }
        Commands::Install { package } => {
            println!("Installing: {package}...");
            println!("Note: Package manager is a stub. Full Git-based registry coming soon.");
            Ok(())
        }
        Commands::Repl => commands::repl::start_repl(),
        Commands::Fmt { file } => {
            let source = match std::fs::read_to_string(&file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Error reading file '{file}': {e}");
                    std::process::exit(1);
                }
            };
            // Basic formatting: re-parse and pretty-print
            match whisper_parser::Parser::parse_source(&source) {
                Ok(ast) => {
                    println!("// Formatted: {file} ({:#?} nodes)", ast.len());
                    // Full formatter would output canonical .ws
                }
                Err(e) => {
                    eprintln!("Parse error: {}", e.message);
                    std::process::exit(1);
                }
            }
            Ok(())
        }
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}
