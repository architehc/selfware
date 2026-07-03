# Tool verification against GLM-5.2

Verifies that **every registered selfware tool works with the configured model**
(GLM-5.2 via OpenRouter). For each tool in the `ToolRegistry`, it forces the model
to call that exact tool (named `tool_choice`) and validates the arguments the model
returns against the tool's own JSON schema — required fields present, declared types
matched. It's a per-tool accuracy check: *can the model produce a valid call for
every tool selfware exposes?*

It reuses selfware's own registry (`ToolRegistry::new()` → `list_critical()` +
`list_deferred()`) and config (endpoint / model / key / OpenRouter provider routing),
so it exercises the real, live tool schemas.

It does **not** execute the tools (that would run `file_write`/`shell` with
model-chosen arguments); it verifies the model emits schema-valid calls.

## Run

```bash
cargo build --release --features tool-verify --bin tool_verify
source ~/.openrouter_env
./target/release/tool_verify --config ~/.config/selfware/config.toml \
    --json experiments/tool_verify/results/verify.json

# Cheaper subsets:
./target/release/tool_verify --limit 10
./target/release/tool_verify --tool file_edit
```

## What "PASS" means

For a tool `T`: the model was forced to call `T`, returned a `tool_calls` entry
named `T`, whose `arguments` parse as JSON and satisfy `T`'s schema `required`
fields and declared property types. Any of: no tool call, non-JSON arguments,
missing required field, or a type mismatch → `FAIL` with the reason.

## Latest result

`93/93` tools produced schema-valid GLM-5.2 calls (100%), served by Fireworks
(a full-1M-context, tool-capable OpenRouter provider). See `results/verify.json`.

Run this after adding or changing a tool's schema to confirm the model can still
call it correctly.
