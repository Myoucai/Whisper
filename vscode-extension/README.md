# Whisper Language Support for VS Code

Syntax highlighting for the [Whisper](https://github.com/Myoucai/Whisper) programming language.

## Features

- Syntax highlighting for `.ws` files
- Bracket matching and auto-closing
- Code folding for word definitions
- String, number, boolean, operator highlighting
- Special highlighting for `@map`, `@each`, `@fold`, `@nth`, `@times`
- Word definition highlighting (`: name { ... } ;`)

## Install

Copy this folder to `~/.vscode/extensions/whisper-language/`:

```bash
cp -r vscode-extension ~/.vscode/extensions/whisper-language/
```

Or from the Whisper project root:

```bash
cd vscode-extension
code --install-extension whisper-language-0.1.0.vsix
```

## Screenshot

With this extension, a `.ws` file looks like:

![syntax highlighting example](https://raw.githubusercontent.com/Myoucai/Whisper/main/docs/screenshot.png)
