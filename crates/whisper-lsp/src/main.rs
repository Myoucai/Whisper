//! Standalone binary entry point for the Whisper LSP server.
//! Delegates to the library's `run_lsp_server()`.

fn main() -> anyhow::Result<()> {
    whisper_lsp::run_lsp_server()
}
