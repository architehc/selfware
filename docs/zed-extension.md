# ZED Extension Guide

Selfware includes a Zed editor extension that launches `selfware lsp` for editor navigation and exposes `/selfware-graph` in the Assistant for workspace graph exploration.

## Features

- Language server integration for **Rust**, **Python**, **TypeScript**, **JavaScript**, **Go**
- Workspace graph slash command: `/selfware-graph`
- Context server wiring for MCP-style integrations

## Prerequisites

1. Install selfware:
   ```bash
   cargo install --git https://github.com/architehc/selfware
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

Add selfware configuration to your Zed `settings.json`:

```json
{
  "selfware": {
    "endpoint": "http://localhost:8000/v1",
    "model": "Qwen/Qwen3-Coder-Next-FP8"
  }
}
```

#### Settings Reference

| Setting | Type | Default | Description |
|---------|------|---------|-------------|
| `endpoint` | string | `"http://localhost:8000/v1"` | LLM API endpoint URL |
| `model` | string | `"Qwen/Qwen3-Coder-Next-FP8"` | Model identifier |
| `binary_path` | string | none | Optional explicit path to the `selfware` binary |

The extension passes `endpoint` and `model` as environment variables (`SELFWARE_ENDPOINT`, `SELFWARE_MODEL`) to the `selfware lsp` process. You can also set `binary_path` or override the context-server command path if you need to point at a custom build.

## Slash Commands

Use `/selfware-graph` in the Assistant to render a workspace graph for the current worktree. The command accepts an optional format plus an optional focus path.

## How It Works

The extension starts `selfware lsp` as a background process. Communication happens over the LSP protocol (JSON-RPC over stdio). The extension:

1. Resolves the `selfware` binary on your PATH (looks in common locations: `selfware`, `/usr/local/bin/selfware`, `/opt/homebrew/bin/selfware`)
2. Starts `selfware lsp` with the configured environment variables
3. Passes initialization options including endpoint, model, and capabilities
4. Routes `/selfware-graph` through the editor command path for workspace graph generation

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

### LSP features not appearing

1. Check your LLM backend is running:
   ```bash
   curl http://localhost:8000/v1/models
   ```

2. Verify ZED settings are correct (endpoint and model)

3. Check ZED's extension log for errors

### Wrong model or endpoint

The extension reads settings from ZED's `settings.json`. Make sure the `selfware` section is at the top level, not nested inside another key.
