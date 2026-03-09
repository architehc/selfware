# ZED Extension Guide

Selfware includes a ZED editor extension that provides AI-powered coding assistance via the Language Server Protocol (LSP).

## Features

- Language server integration for **Rust**, **Python**, **TypeScript**, **JavaScript**, **Go**
- Inline completions and suggestions
- Code actions: Fix, Refactor, Explain, Generate Tests
- Context menu items: Ask Selfware, Explain Code, Generate Tests

## Prerequisites

1. Install selfware:
   ```bash
   cargo install selfware
   ```

2. Have a running LLM backend (see [Getting Started](getting-started.md))

3. Install the [ZED editor](https://zed.dev/)

## Installation

### From the ZED Extension Directory

The extension is published as `selfware` in the ZED extension marketplace. Install it from:

**ZED menu -> Extensions -> Search "selfware" -> Install**

### From Source

1. Build the extension:
   ```bash
   cd zed-extension
   cargo build --release --target wasm32-wasi
   ```

2. Install it into ZED's extension directory (see ZED docs for the exact path).

## Configuration

### ZED Settings

Add selfware configuration to your ZED `settings.json`:

```json
{
  "selfware": {
    "endpoint": "http://localhost:8000/v1",
    "model": "Qwen/Qwen3-Coder-Next-FP8",
    "auto_suggest": true,
    "inline_completions": true
  }
}
```

#### Settings Reference

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `endpoint` | string | `"http://localhost:8000/v1"` | LLM API endpoint URL |
| `model` | string | `"Qwen/Qwen3-Coder-Next-FP8"` | Model identifier |
| `auto_suggest` | boolean | `true` | Enable automatic code suggestions |
| `inline_completions` | boolean | `true` | Enable inline completion popups |

The extension passes `endpoint` and `model` as environment variables (`SELFWARE_ENDPOINT`, `SELFWARE_MODEL`) to the `selfware lsp` process. You can also set these in your shell environment if you prefer.

## Keybindings

The extension provides these default keybindings:

| Shortcut | Command | Description |
|----------|---------|-------------|
| `Ctrl+Shift+S` | `selfware::ask` | Ask Selfware about selected code |
| `Ctrl+Shift+E` | `selfware::explain` | Explain selected code |
| `Ctrl+Shift+T` | `selfware::generate_tests` | Generate tests for selected code |
| `Ctrl+Shift+R` | `selfware::refactor` | Refactor selected code |

### Custom Keybindings

Override the defaults in your ZED `keybindings.json`:

```json
[
  { "key": "ctrl-shift-s", "command": "selfware::ask" },
  { "key": "ctrl-shift-e", "command": "selfware::explain" },
  { "key": "ctrl-shift-t", "command": "selfware::generate_tests" },
  { "key": "ctrl-shift-r", "command": "selfware::refactor" }
]
```

## Code Actions

Right-click on code (or use the lightbulb menu) to access:

| Action | Kind | Description |
|--------|------|-------------|
| Fix with Selfware | `quickfix` | AI-powered code fix |
| Refactor with Selfware | `refactor` | AI-powered refactoring |
| Explain Code | `source` | Get an explanation of the selected code |
| Generate Tests | `source` | Generate unit tests for the selected code |

## Context Menu

Right-click selected code to see:

- **Ask Selfware** -- send a question about the selection
- **Explain Code** -- get an explanation
- **Generate Tests** -- generate test cases

## How It Works

The extension starts `selfware lsp` as a background process. Communication happens over the LSP protocol (JSON-RPC over stdio). The extension:

1. Resolves the `selfware` binary on your PATH (looks in common locations: `selfware`, `/usr/local/bin/selfware`, `/opt/homebrew/bin/selfware`)
2. Starts `selfware lsp` with the configured environment variables
3. Passes initialization options including endpoint, model, and capabilities
4. Forwards code actions and completion requests through the LSP protocol

## Supported Languages

The extension registers the selfware language server for:

- Rust
- Python
- TypeScript
- JavaScript
- Go

## Troubleshooting

### Extension not loading

Verify selfware is on your PATH:
```bash
which selfware
selfware --version
```

### No completions appearing

1. Check your LLM backend is running:
   ```bash
   curl http://localhost:8000/v1/models
   ```

2. Verify ZED settings are correct (endpoint and model)

3. Check ZED's extension log for errors

### Wrong model or endpoint

The extension reads settings from ZED's `settings.json`. Make sure the `selfware` section is at the top level, not nested inside another key.
