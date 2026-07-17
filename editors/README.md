# Selfware Code Map -- Editor Integrations

All editors share the same data source: `codegraph.json`

## Generate the graph

```sh
cargo run --bin codegraph
```

## Editors

| Editor | Status | Install |
|--------|--------|---------|
| VS Code | Full | See ../vscode-selfware/ |
| Zed | Scaffold | See ../zed-extension/ |
| Neovim | Functional | See neovim/ |
| JetBrains | Scaffold | See jetbrains/ |
| Helix | CLI only | See helix/ |
| Sublime | Scaffold | See sublime/ |

## Architecture

```
codegraph.rs --> codegraph.json --> editor plugin --> webview/canvas
```

The `codegraph` binary parses the Rust source tree and emits a JSON graph
(`codegraph.json`) containing nodes (modules, structs, functions) with
token counts and dependency edges.  Each editor plugin reads this file and
presents it through the editor's native UI -- telescope for Neovim, JCEF
for JetBrains, WASM for Zed, quick panel for Sublime, and shell commands
for Helix.

The JSON format is the universal data contract.  Adding a new editor
integration means writing a thin adapter that reads `codegraph.json` and
maps its structure to the editor's extension API.
