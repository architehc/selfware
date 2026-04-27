# Qwen3.6 quant benchmark harness

Scripts and a Rust example for comparing quantizations of
`HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive` end-to-end through the
selfware agent loop, served via `llama.cpp`'s `llama-server`.

We currently target the smallest and biggest non-FP16 quants:

- **smallest** — `Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf` (~10 GB)
- **biggest**  — `Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf` (~32 GB)
- **mmproj**   — `mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf` (~928 MB)

## Prerequisites

- `llama.cpp` built with `LLAMA_CURL=1` (the recipe needs `--mmproj`).
  ```bash
  cmake -B build -DGGML_CUDA=ON -DLLAMA_CURL=ON
  cmake --build build --config Release -j
  export PATH="$(pwd)/build/bin:$PATH"
  ```
- `huggingface-cli` (or `wget`/`curl` as a fallback). Install with
  `pip install -U "huggingface_hub[cli]"` and authenticate via
  `huggingface-cli login`.
- Free disk: at least **~50 GB** for both quants + mmproj. Plus enough VRAM
  to fit each (Q8_K_P needs ~32 GB; IQ2_M comfortably runs on 12-16 GB).
- Rust 1.95.0 toolchain (to match the rest of the repo).
- `python3` for the report collator.

## Files

| File | Role |
|------|------|
| `download_quants.sh` | Idempotent fetcher for the two GGUFs + the mmproj |
| `run_bench.sh`       | Boots `llama-server` per quant and runs the example |
| `../../examples/quant_benchmark.rs` | Test suite executed against each endpoint |

## 1. Download the GGUFs

```bash
bash scripts/quant_bench/download_quants.sh
# or, override location:
bash scripts/quant_bench/download_quants.sh --dir /data/models/qwen36
```

Downloads land in `~/models/qwen36-quants/` by default. The script:

- Prefers `huggingface-cli download`; falls back to `wget`/`curl`.
- Skips a file if it is already present and matches HF's reported size.
- Verifies sha256 (LFS oid) when HF exposes it; warns on mismatch.

## 2. Run the benchmark

```bash
bash scripts/quant_bench/run_bench.sh
# typical override:
bash scripts/quant_bench/run_bench.sh \
    --models-dir /data/models/qwen36 \
    --reports-dir reports/qwen36-bench \
    --port-base 9000
```

For each quant the orchestrator:

1. Launches
   ```
   llama-server --model <gguf> --port <p> --host 127.0.0.1 \
                --jinja -c 131072 -ngl 99 \
                --chat-template-kwargs '{"enable_thinking": false}' \
                --mmproj <mmproj-f16.gguf>
   ```
2. Waits up to `--wait-secs` (default 300) for `/v1/models` to respond.
3. Runs `cargo run --release --example quant_benchmark`, writing the JSON
   report to `<reports-dir>/<quant>.json`.
4. Cleanly stops `llama-server`.

Once both quants are done, `run_bench.sh` collates every
`<reports-dir>/*.json` into `<reports-dir>/comparison.md` (summary table,
per-test breakdown, per-quant detail).

## 3. Test categories

The example (`examples/quant_benchmark.rs`) runs five tests in order:

| ID | What it measures |
|----|------------------|
| `speed`      | Median tok/s across 3 cold 200-token completions |
| `tool_use`   | Whether the model issues tool calls in the right shape (`file_read` → `grep_search` → `file_edit`) using selfware-style schemas |
| `codegen`    | Generates a Rust `fizzbuzz` and confirms it builds via `cargo check` in a tempdir |
| `reasoning`  | Solves a 3-step word problem and writes `ANSWER: <number>` |
| `multimodal` | Sends a synthetic 64×64 PNG (red square on green background) and checks for visual vocabulary in the reply |

Output:

- **JSON** to stdout (consumed by the collator).
- **Markdown** table to stderr (per-test pass/fail + timing).
- A copy of the JSON to `<output-dir>/<quant>.json`.

## Knobs (env / flags)

| Flag / env | Default | Notes |
|---|---|---|
| `--models-dir`      | `~/models/qwen36-quants` | Where the GGUFs live |
| `--reports-dir`     | `reports/quant_bench` (under the repo root) | Where JSON + `comparison.md` are written |
| `--llama-server`    | `llama-server` on PATH   | Path to the binary |
| `--port-base`       | `8080`                   | First port; subsequent quants increment |
| `--ngl`             | `99`                     | Layers to offload to GPU |
| `--ctx`             | `131072`                 | `-c` flag for llama-server |
| `--wait-secs`       | `300`                    | Endpoint readiness timeout |
| `--timeout-secs`    | `180`                    | Per-request timeout in the example |
| `--skip-multimodal` | off                      | Skip test 5 (e.g. when no mmproj) |

## Sanity checks before committing

```bash
cargo +1.95.0 build --example quant_benchmark
cargo +1.95.0 clippy --example quant_benchmark --features extras -- -D warnings
cargo +1.95.0 fmt --check
bash -n scripts/quant_bench/*.sh
```
