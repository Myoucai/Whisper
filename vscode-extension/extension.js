// Whisper VS Code Extension — LSP Client
// Launches the whisper-lsp server and connects via stdio.

const vscode = require('vscode');
const { spawn } = require('child_process');

let clientProcess = null;

function startLsp() {
    const config = vscode.workspace.getConfiguration('whisper');
    const lspPath = config.get('lsp.path', 'whisper-lsp');

    console.log(`[Whisper] Starting LSP server: ${lspPath}`);

    clientProcess = spawn(lspPath, [], {
        stdio: ['pipe', 'pipe', 'pipe'],
    });

    clientProcess.stderr.on('data', (data) => {
        console.log(`[Whisper LSP] ${data}`);
    });

    clientProcess.on('error', (err) => {
        vscode.window.showWarningMessage(
            `Whisper LSP server not found at "${lspPath}". ` +
            'Build with: cargo build -p whisper-lsp'
        );
    });

    clientProcess.on('exit', (code) => {
        console.log(`[Whisper] LSP server exited: ${code}`);
        clientProcess = null;
    });

    return clientProcess;
}

function stopLsp() {
    if (clientProcess) {
        clientProcess.kill();
        clientProcess = null;
    }
}

function activate(context) {
    console.log('[Whisper] Extension activated');

    // Try to start LSP server
    const proc = startLsp();
    if (!proc) {
        vscode.window.showInformationMessage(
            'Whisper LSP: Install whisper-lsp for diagnostics and completions'
        );
    }

    context.subscriptions.push({
        dispose: () => stopLsp(),
    });
}

function deactivate() {
    stopLsp();
}

module.exports = { activate, deactivate };
