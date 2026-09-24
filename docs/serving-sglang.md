# Serving qwen38-flash-next with SGLang

This page is for operators running their own SGLang server for Selfware, like
the one behind `https://llm.selfware.design/v1`. The numbers come from our
endpoint validation on 2026-09-24 with `qwen38-flash-next`, a hybrid
Mamba/GDN (linear-attention) model. Other models may behave differently, so
measure before copying.

For the client side (what to put in `selfware.toml`), see
[Running against llm.selfware.design / SGLang](../README.md#running-against-llmselfwaredesign--sglang)
in the README.

## Recommended flags

| Flag | Why |
|------|-----|
| `--tool-call-parser qwen3_coder` | The `qwen` parser **silently drops streamed tool calls** for this model. With `qwen3_coder`, clients can use native tool calling (`[agent] native_function_calling = true`). With any other parser, keep `native_function_calling = false` on the client (the Selfware default for this model). |
| `--enable-metrics` | Exposes Prometheus metrics (throughput, queue depth, TTFT) so you can measure the rest of this table on your own hardware. |
| `--max-running-requests 8` | Total throughput plateaus at ~117–122 tok/s between 8 and 16 concurrent requests, while per-request speed halves at 16. Past 8, clients wait longer and total throughput doesn't go up. |
| `--chunked-prefill-size 8192` | About 20% lower latency on prefill-bound (long-prompt) requests. |
| `--mamba-scheduler-strategy extra_buffer --page-size 64` | Required for prefix caching on hybrid Mamba/GDN models. With the default `no_buffer` strategy there are **zero** prefix-cache hits, so every agent turn re-prefills the whole conversation. Also needs the FLA linear-attention backend. Newer SGLang renames this flag to `--mamba-radix-cache-strategy`. |

## Example launch

```bash
python -m sglang.launch_server \
  --model-path <path-or-repo-of-qwen38-flash-next> \
  --served-model-name qwen38-flash-next \
  --tool-call-parser qwen3_coder \
  --enable-metrics \
  --max-running-requests 8 \
  --chunked-prefill-size 8192 \
  --mamba-scheduler-strategy extra_buffer \
  --page-size 64 \
  --host 0.0.0.0 --port 8000
```

Add your usual model-specific flags (tensor parallelism, context length,
memory fraction, reasoning parser, and the FLA linear-attention backend
option for your SGLang version). Check `python -m sglang.launch_server --help`
for the exact flag names in your version.

## Context length and timeouts

- **Context.** The largest real prompt we saw in agent runs was 127k tokens.
  Long prompts are expensive: time to first token was 13 s at 99k tokens vs
  40 s at 257k, and decode dropped to 17 tok/s. A served window of 128k–160k
  covers real use. Clients should set `context_length` to match.
- **Gateways.** An ngrok-style gateway in front of the server cut
  non-streaming requests at 300 s. The longest real call we saw took 358 s, so
  clients must stream (Selfware does by default) and should cap calls with
  `[agent] max_call_secs = 600`.
- **Reasoning length.** `reasoning_effort` had no measurable effect on this
  model. Reasoning is bounded only by the client's `max_tokens`; at 65536 a
  runaway reasoning stream ran ~27 minutes at ~40 tok/s, so we recommend
  clients use `max_tokens = 24576`.

## Checking the server from Selfware

Point a config at the server and run:

```bash
selfware llm-doctor
```

It checks connectivity and latency and probes streaming and streamed tool
calls, which is where a wrong `--tool-call-parser` shows up.
