# API / Token / Streaming / Prompt Bug Audit

Analyzed: `src/api/{mod,client,types,tool_calling,streaming,tests}.rs`, `src/tokens.rs`, `src/token_count.rs`, `src/llm_doctor.rs`, `src/cognitive/{token_budget,intelligence}.rs`, `src/agent/{prompt_builder,assistant_response,interactive,session_log}.rs`

---

## 1. API Client Bugs

### 1.1 Retry Logic

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/api/client.rs` | **535–538 + 467** | `Retry-After` header value is assigned to `delay_ms`, but the next loop iteration **doubles it again** via `(delay_ms * 2).min(max_delay_ms)`. A backend asking for 60 s becomes 120 s (or capped). | **High** |
| `src/api/client.rs` | **396–430** | `chat_stream_send()` has **zero retry logic**. Network timeouts, 429/5xx, and connect errors on streaming requests are returned immediately. The circuit breaker only prevents cascading failures; it does not retry. | **High** |
| `src/api/client.rs` | **169–209** | `completion_inner()` (legacy `/completions` endpoint) has **no retry**, no circuit breaker, and no `send_with_retry` wrapper. Transient failures are fatal. | **High** |
| `src/api/client.rs` | **517–522** | `Retry-After` parsing only accepts numeric seconds. It does **not** support the HTTP-date format (`Retry-After: <http-date>`) allowed by RFC 7231. Proxies/CDNs that send dates cause the value to be ignored. | Medium |
| `src/api/client.rs` | **460–471** | Log message says "Retry attempt {attempt}/{max_retries}" where `attempt` is the loop counter. With `max_retries = 3`, attempt 3 is logged as `3/3`, but it is actually the **4th** request (initial + 3 retries). | Low |

### 1.2 Timeout Handling

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/api/client.rs` | **108–113** | `request_timeout = step_timeout_secs.max(60)`. A user who sets `step_timeout_secs = 10` to get fast local-LLM feedback still gets a 60 s HTTP timeout, masking hung backends. | Medium |
| `src/api/client.rs` | **425** | `stream_chunk_timeout_secs = step_timeout_secs.max(30)`. For local backends with slow cold-start TTFT (first token), 30 s can be too short. There is no special case for local endpoints. | Medium |
| `src/api/client.rs` | **110** | `connect_timeout` is hard-coded to 30 s. Local backends that are still loading a large model (e.g. Qwen 3.6 122B) may need more. | Low |

---

## 2. Token Counting Inaccuracies

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/token_count.rs` | **41** | Loads tokenizer `"Qwen/Qwen2.5-Coder-32B"` from HuggingFace Hub. **Qwen 3.6 uses a different vocabulary**; counts will drift. If HF Hub is offline, fallback is OpenAI `cl100k_base`, which is completely wrong for Qwen. | **High** |
| `src/token_count.rs` | **114–122** | `heuristic_estimate` uses `len() / 3` for code and `len() / 4` for text. For CJK text (Qwen is a Chinese-English model), characters are often 1–2 tokens each. The heuristic **systematically under-counts** CJK prompts. | **High** |
| `src/tokens.rs` | **389–411** | `estimate_messages_tokens` adds a fixed `4` tokens per message for "role overhead". This is a crude heuristic; actual overhead varies by tokenizer and message format (OpenAI chat format vs raw Qwen template). | Medium |
| `src/tokens.rs` | **413–427** | `estimate_tool_definitions_tokens` adds a flat `10` tokens per tool for "structural overhead" and then counts the JSON schema. Complex schemas are under-counted; simple schemas may be over-counted. | Medium |
| `src/tokens.rs` | **330–376** | `estimate_image_tokens` implements the **OpenAI vision tiling formula**. Qwen 3.6 VL (and other non-OpenAI vision models) use different image tokenization. Budget calculations for vision tasks will be wrong. | Medium |
| `src/tokens.rs` | **186–203** | `get_model_pricing` only matches Claude model names (`haiku`, `sonnet`, `opus`). Qwen models fall back to Claude Sonnet pricing, making cost estimates meaningless. | Low |

---

## 3. Prompt Construction Flaws

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/api/mod.rs` | **29–60** | `canonicalize_message_order` merges **all** system messages into one at index 0. If `maybe_prepend_disabled_thinking_instruction` (called earlier) inserted a critical "DO NOT think" instruction, it gets buried inside a large merged system prompt, reducing its salience. | Medium |
| `src/api/mod.rs` | **51–59** | Injects a dummy user message (`"Continue with the task."`) when no user message exists. This happens after system-message merging but before tool-message validation. The resulting order can place this dummy message **after** tool-result messages, which some strict backends reject. | Medium |
| `src/agent/assistant_response.rs` | **431–439** | Stores assistant history with `reasoning_content: None` (per Qwen 3.5 "no thinking in history" advice). For Qwen 3.6 multi-step reasoning workflows, stripping reasoning from history may **reduce coherence** on subsequent turns because the model loses its own chain-of-thought context. | Medium |
| `src/agent/assistant_response.rs` | **198–211** | System hints (learning hints, failure hints, context map, RAG results) are merged into the **first** system message or prepended as a new system message. If the original system prompt was carefully tuned with a specific prefix/suffix, this append/prepend can break template boundaries. | Low |
| `src/agent/assistant_response.rs` | **213–221** | RoPE boundary marker is inserted as a `user` message 6 messages from the end. If the conversation contains tool-result messages near the end, this inserts a user message between a tool result and the next assistant turn, which violates "user/assistant alternating" rules on some backends. | Low |

---

## 4. Streaming Parser Bugs

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/api/streaming.rs` | **90** | `String::from_utf8_lossy(&bytes)` is applied to raw HTTP chunks. If a **multi-byte UTF-8 character is split across chunks**, it produces replacement characters (`�`) instead of valid text. The SSE parser then operates on corrupted text. | **High** |
| `src/api/streaming.rs` | **93** | SSE event delimiter is hard-coded as `\n\n`. The SSE spec allows `\r\n\r\n` (CRLF). Proxies or local servers that use CRLF will never have events parsed; data accumulates in `buffer` until timeout. | **High** |
| `src/api/streaming.rs` | **129–137** | Trailing buffer after stream end is trimmed and parsed. If the trailing data is a **partial JSON chunk** (e.g. `data: {"choices":[{`), `serde_json::from_str` fails silently and the partial content is **permanently lost**. | **High** |
| `src/api/streaming.rs` | **266–306 + 364–373** | `ToolCallAccumulator::process_delta` **always returns `None`**. The code at line 369 `if let Some(completed) = accumulator.process_delta(tc_delta)` is **dead code**. Tool calls are **never emitted mid-stream**; they are only flushed on `[DONE]`, `finish_reason`, timeout, or error. Downstream consumers must wait until the stream ends. | **High** |
| `src/api/streaming.rs` | **308–322** | `flush()` creates `ToolCall` objects with `unwrap_or("")` defaults. If the backend never sends `id`, `type`, or `name` in deltas (common with vLLM/SGLang), `flush()` produces **invalid ToolCalls** with empty IDs and names. | **High** |
| `src/api/streaming.rs` | **342** | `parse_sse_event` silently swallows all JSON parse errors (`if let Ok(json) = ...`). A single malformed chunk causes **silent content loss** with no log warning. | Medium |
| `src/api/streaming.rs` | **375–385** | On `finish_reason`, the parser flushes tool calls **and then** emits `FinishReason`. But if the `finish_reason` chunk also contains a delta with content, that content is ignored because the parser handles `finish_reason` after `content`. | Low |

---

## 5. Rate Limiting Gaps

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/api/client.rs` | *entire file* | **No client-side rate limiter** (token bucket, leaky bucket, or request-per-second cap). Can overwhelm a local vLLM/llama.cpp backend, especially in swarm mode or rapid tool loops. | **High** |
| `src/api/client.rs` | **512–540** | On 429 responses, only the `Retry-After` header is inspected. Many APIs embed rate-limit metadata in the JSON body (`error.retry_after`, `error.type = "rate_limit_error"`). These are ignored. | Medium |
| `src/api/client.rs` | **396–430** | Streaming requests have **no retry path** for 429/5xx. A rate-limited streaming request fails immediately. | **High** |
| `src/api/client.rs` | **169–209** | `completion()` has **no retry** for 429/5xx. | Medium |

---

## 6. Issues Specific to Local vLLM / llama-server Endpoints

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/api/client.rs` | **115–123** | HTTP warning only checks `starts_with("http://") && !is_local_endpoint`. Local endpoints using HTTP (the common case) are whitelisted with no warning, but there is **no additional resilience** added for them (e.g. longer connect timeouts, health-check probes). | Medium |
| `src/api/streaming.rs` | **14** | `STREAM_SEMAPHORE` is capped at 100 concurrent streams. Local backends (especially single-GPU vLLM or llama.cpp) can be brought down by far fewer concurrent connections. No per-backend concurrency limit. | Medium |
| `src/api/types.rs` | **4–11** | `deserialize_nullable_content` handles vLLM `"content": null`. Good. But some local backends also emit `"content": []` (empty array) which the `untagged` enum deserializer may fail to parse as either `Text` or `Blocks`. | Medium |
| `src/api/tool_calling.rs` | **38–48** | `attach_tools` **always** sets `body["tools"]`, even when `native_function_calling = false`. Local backends that do not support the `tools` field at all (some llama.cpp configs) will return **400 Bad Request** with no way to opt out. | **High** |
| `src/llm_doctor.rs` | **430–432** | Streaming probe uses `resp.text().await` on a streaming request. Local proxies may buffer or truncate the response; the probe does not consume SSE chunks properly and can falsely report streaming as broken. | Medium |
| `src/llm_doctor.rs` | **446–449** | `chat_template_kwargs` probe only checks HTTP status. A local backend that **ignores** the parameter (e.g. older llama.cpp) still returns 200, so the probe falsely reports thinking control as working. | Medium |

---

## 7. Issues That Will Cause Failures with Qwen 3.6 Models

| File | Line | Issue | Severity |
|------|------|-------|----------|
| `src/llm_doctor.rs` | **785–788** | `is_qwen35_model()` only matches `qwen3.5` and `qwen3-5`. **Qwen 3.6 is not matched**, so all Qwen-specific handling (context length hints, chat template advice, thinking recommendations) is skipped. | **High** |
| `src/llm_doctor.rs` | **996–1061** | `assess_capabilities` has no branch for Qwen 3.6. Falls through to generic `"Qwen model — generally good"`, missing model-size-specific guidance. | Medium |
| `src/token_count.rs` | **41** | Hard-coded `"Qwen/Qwen2.5-Coder-32B"` tokenizer. Qwen 3.6 has a different vocabulary. Token estimates will be **systematically wrong**, causing context-overflow errors or premature truncation. | **High** |
| `src/tokens.rs` | **330–376** | Image token estimate uses OpenAI tiling. Qwen 3.6 VL uses its own vision encoder with different patch sizes and token counts. Vision budget calculations will be wrong. | Medium |
| `src/agent/assistant_response.rs` | **422–436** | Strips all reasoning from history per Qwen 3.5 best practice. Qwen 3.6 may expect reasoning to be preserved in history for coherent multi-turn reasoning. Stripping it could cause the model to "forget" its plan between tool calls. | Medium |
| `src/api/mod.rs` | **101–124** | `ALLOWED_EXTRA_BODY_KEYS` includes `chat_template_kwargs` but not Qwen 3.6-specific top-level parameters. If a backend expects `enable_thinking` at the top level (not nested), it cannot be sent via `extra_body`. | Low |
| `src/api/streaming.rs` | **355–362** | `reasoning_content` extraction checks `reasoning_content` and `reasoning`. Qwen 3.6 may emit thinking under a different field name (e.g. `think`, `thought`) depending on the chat template. If the template changes, reasoning is silently dropped. | Medium |

---

## Quick-Fix Priority List

1. **Add retry to `chat_stream_send`** (`client.rs:396–430`) — copy the `send_with_retry` pattern or create a shared retry wrapper.
2. **Fix Retry-After doubling** (`client.rs:535–538`) — skip the backoff-doubling on the next iteration when `Retry-After` was honored.
3. **Handle CRLF in SSE parser** (`streaming.rs:93`) — normalize `\r\n` to `\n` before scanning for `\n\n`.
4. **Fix UTF-8 split across chunks** (`streaming.rs:90`) — buffer raw bytes and only convert to `String` at event boundaries, or use a lossless UTF-8 accumulator.
5. **Emit tool calls mid-stream** (`streaming.rs:266–306`) — return `Some(tool_call)` from `process_delta` when a complete tool call is detected (e.g. when all required fields are present).
6. **Detect Qwen 3.6** (`llm_doctor.rs:785–788`) — add `qwen3.6` / `qwen3-6` matching.
7. **Load correct tokenizer for Qwen 3.6** (`token_count.rs:41`) — add `"Qwen/Qwen3-6B"` (or the correct HF repo) as a higher-priority match before falling back to 2.5.
8. **Add `tools` opt-out** (`tool_calling.rs:38–48`) — allow callers to suppress `body["tools"]` entirely when the backend does not support it.
9. **Add client-side rate limiter** (`client.rs`) — a simple token bucket (e.g. 10 req/s default) to protect local backends.
10. **Preserve reasoning for Qwen 3.6** (`assistant_response.rs:431–439`) — make reasoning stripping conditional on a model-family flag rather than unconditional.
