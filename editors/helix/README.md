# Selfware Code Map -- Helix Integration

Helix does not have a plugin API yet. Integration works through shell commands
and the built-in `:sh` command.

## Prerequisites

```sh
# Verify rust-analyzer is detected
hx --health rust
```

## Setup

Add custom key bindings in `~/.config/helix/config.toml`:

```toml
[keys.normal.space.c]
m = ":sh cargo run --bin codegraph -- --format summary"
f = ":sh cargo run --bin codegraph -- --focus %{filename} --format summary"
a = ":sh cargo run --bin codegraph -- --focus agent --format summary"
d = ":sh cargo run --bin codegraph -- --focus agent --deps --format list"
```

## Usage

| Key | Action |
|-----|--------|
| `<space>cm` | Show full code map summary |
| `<space>cf` | Show graph focused on current file |
| `<space>ca` | Show agent module summary |
| `<space>cd` | List agent module dependencies |

## How it works

Each binding shells out to the `codegraph` binary which reads `codegraph.json`
(or regenerates it) and prints a text summary to the Helix command output area.

For a richer experience, run the codegraph CLI in a separate terminal pane
(e.g. via tmux/zellij) and use Helix's `:sh` to trigger refreshes.

## Generating the graph

```sh
cargo run --bin codegraph
# produces codegraph.json in the project root
```
