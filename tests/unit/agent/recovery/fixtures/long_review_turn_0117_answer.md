## Final Audit Report: Selfware Core Workflow — Six Stages

---

### Stage 1 — API Streaming and Tool-Call Parsing

**Files inspected:** `src/api/streaming.rs` (lines 1–300), `src/tool_parser.rs` (lines 700–1255), `src/agent/recovery.rs` (lines 1–200), `tests/unit/tool_parser/tool_parser_tests_test.rs`, `tests/unit/api/streaming/streaming_test.rs`

**Concrete findings:**

**Bug 1.1 — Inconsistent unclosed-tag handling between `strip_think_blocks` and `strip_gemma_thinking`.**
`src/agent/recovery.rs:186–193` — `strip_think_blocks` preserves content after an unclosed `` by advancing `rest` past the 7-char opening tag. `src/agent/recovery.rs:210–215` — `strip_gemma_thinking` sets `rest = ""` on an unclosed `<|channel>`, discarding all subsequent content. **Trigger:** A Gemma model emits `<|channel>thought<channel|>` then a second `<|channel>` with no closing tag, followed by the final answer. **Consequence:** The final answer after the second unclosed marker is silently dropped; the agent sees only the first block's content. Severity: medium — affects any session using a Gemma-family model with multi-block thinking.

**Bug 1.2 — `ParseMethod::Json` reused for plain function-call syntax.**
`src/tool_parser.rs:1088` — `try_parse_plain_function_calls` assigns `ParseMethod::Json` to its results, the same variant used by `try_parse_json_blocks` (line 960). **Trigger:** Model emits `file_read("src/main.rs")` (positional syntax). **Consequence:** Telemetry and debug logs cannot distinguish JSON-block parsing from positional-function-call parsing. Functional correctness is unaffected; only diagnostic granularity is lost. Severity: low.

**No further bugs confirmed in this stage.** The `extract_json_balanced` brace-counting logic (lines 830–870) correctly handles nested objects, escaped quotes, and truncated input. The `qwen_hybrid_header` function (lines 720–750) correctly limits structural tags to the leading position. The `classify_stream_timeout` function (lines 62–82) correctly prioritizes run-deadline over per-call cap.

---

### Stage 2 — Sequential and Parallel Dispatch

**Files inspected:** `src/agent/tool_dispatch/mod.rs` (lines 1–2600), `src/agent/tool_dispatch/helpers.rs` (lines 2730–2800), `src/agent/execution.rs` (lines 1–400), `tests/unit/agent/tool_dispatch/tool_dispatch_test.rs` (lines 1–300)

**Concrete findings:**

**Bug 2.1 — `detect_oscillating_batch_pair` skips parallel batches entirely.**
`src/agent/recovery.rs:148–152` — The function returns `None` if any of the last 4 batches has `batch.len() != 1`. **Trigger:** Two tools dispatched in parallel (e.g., `file_read` + `grep_search` in one batch), repeated 4 times. **Consequence:** The oscillation detector never fires for parallel-dispatch patterns, so the agent may loop indefinitely on a 2-tool cycle without triggering the `FORCE_FALLBACK_AFTER=3` escalation (which only counts no-action prompts, not oscillating batches). Severity: medium — affects any workflow using parallel tool dispatch.

**Bug 2.2 — `inject_runtime_tool_defaults` silently passes through when vision profile is `None`.**
`src/agent/tool_dispatch/helpers.rs:2773` — When `configured_vision_profile` returns `None`, the original `args_str` is returned unchanged. The `VisionAnalyze::execute` then fails with `"endpoint is required"` (`src/tools/vision.rs:100`). **Trigger:** Calling `vision_analyze` with only `{"prompt": "..."}` on a config with no `[models.vision]` section. **Consequence:** The error names the missing field but not the config key to set. The operator must know to add `models.vision` to the TOML. Severity: low — documented in the schema description.

**No further bugs confirmed.** The read-only task detection (line 447) correctly suppresses mutation gates. The escalated-edit deduplication via `args_hash` (line 1503) correctly applies edits once.

---

### Stage 3 — Completion and Verification

**Files inspected:** `src/agent/verification.rs` (lines 1–400), `tests/unit/agent/checkpointing/checkpointing_resume_verification_test.rs` (188 lines)

**Concrete findings:**

**Bug 3.1 — Trailing-colon heuristic false-positive on single-word labels.**
`src/agent/verification.rs:155` — `is_incomplete_action_response` flags text ending with `:` and under 80 chars as incomplete. **Trigger:** A task whose expected final answer is exactly `"Summary:"` or `"Steps:"`. **Consequence:** The gate rejects the correct answer and re-prompts, consuming one of the `MAX_NO_ACTION_PROMPTS=20` slots. Severity: low — most real answers are multi-part.

**No further bugs confirmed.** The `forward_cue && tool_markers` pairing (lines 196–210) correctly identifies past-tense summaries as complete. The `NonCodeArtifactReadback` ordering logic (lines 230–260) correctly credits verification passes that ran after the most recent write.

---

### Stage 4 — Checkpoint Save, Delta Replay, and Resume

**Files inspected:** `src/session/checkpoint.rs` (lines 1–2577), `src/agent/checkpointing.rs` (lines 1–1508), `tests/unit/session/checkpoint/checkpoint_test.rs` (lines 1–1100)

**Concrete findings:**

**Bug 4.1 — Workspace-refresh note caps at 30 paths, truncating file lists for large sessions.**
`src/agent/checkpointing.rs` — The resume directive lists at most 30 written file paths. **Trigger:** A 40-file refactoring task interrupted at step 35. **Consequence:** On resume, the model sees only 30 paths and may re-issue `file_edit` calls for files 31–40 with stale `old_str` values, causing edit failures. The edit-failure escalation path handles this, but at the cost of extra round-trips. Severity: low.

**No further bugs confirmed.** The `.bak` fallback for corrupted primary checkpoints is correct and tested. Verification credit on resume correctly checks write-order vs. test-order.

---

### Stage 5 — Recovery Snapshots and Hooks

**Files inspected:** `src/agent/recovery.rs` (lines 1–400), `src/self_healing/recovery_tree.rs` (lines 1–736), `tests/unit/self_healing/recovery_tree/recovery_tree_test.rs` (lines 1–692), `tests/unit/agent/recovery/recovery_test.rs` (lines 1–300)

**Concrete findings:**

**No confirmed bugs in this stage.** The three-layer no-action counter (20/3/500) is intentional and tested. The `LocalEndpointFallback` defaulting to Ollama `:11434` is documented and handled by the recovery tree's exhaustion logic. The `empty_response_loop_message` correctly distinguishes reasoning-only from truly-empty responses.

---

### Stage 6 — Screenshot-to-Vision Workflow

**Files inspected:** `src/tools/vision.rs` (lines 1–120), `tests/unit/tools/vision/vision_test.rs` (lines 1–400), `src/agent/tool_dispatch/helpers.rs` (lines 2730–2800)

**Concrete findings:**

**Bug 6.1 — `guess_mime` defaults to `"image/png"` for files with no extension.**
`tests/unit/tools/vision/vision_test.rs` — A file named `screenshot` (no dot) gets MIME `"image/png"`. If the actual content is JPEG or WebP, the VLM endpoint may misinterpret the encoding. **Trigger:** `screen_capture` writes a file without extension (some capture tools do). **Consequence:** The VLM receives a JPEG-encoded payload labeled `image/png`; most OpenAI-compatible endpoints handle this via magic-byte sniffing, but strict parsers may reject it. Severity: low.

**No further bugs confirmed.** The `detail` parameter defaults to `"auto"`, `max_tokens` to 4096, `temperature` to 0.2 — all reasonable. The `extra_body` merge via `merge_request_extra_body` correctly overlays provider-specific fields.

---

### Summary Table

| # | File:Line | Bug | Severity |
|---|-----------|-----|----------|
| 1.1 | `src/agent/recovery.rs:186–215` | `strip_gemma_thinking` discards content after unclosed `<|channel>`; `strip_think_blocks` preserves it | Medium |
| 1.2 | `src/tool_parser.rs:1088` | `ParseMethod::Json` reused for plain function calls, losing parser-strategy granularity | Low |
| 2.1 | `src/agent/recovery.rs:148–152` | `detect_oscillating_batch_pair` skips all parallel batches (len≠1) | Medium |
| 2.2 | `src/agent/tool_dispatch/helpers.rs:2773` | Vision profile `None` produces bare `"endpoint is required"` error | Low |
| 3.1 | `src/agent/verification.rs:155` | Trailing-colon heuristic false-positives on single-word labels | Low |
| 4.1 | `src/agent/checkpointing.rs` | 30-path cap truncates file list on resume for large sessions | Low |
| 6.1 | `tests/unit/tools/vision/vision_test.rs` | No-extension MIME defaults to `image/png` regardless of actual encoding | Low |

**Source-confirmed vs. runtime-tested:** All findings above are source-confirmed from direct reads of implementation bodies. No runtime test execution was performed (per task instructions). The `cargo test` suite was not invoked.

**Remaining limits:** (1) `src/agent/tool_dispatch/mod.rs` was read to line 2600 but the file may extend further; parallel-dispatch scheduling internals beyond the dedup logic were not fully traced. (2) The `src/self_healing/recovery_tree.rs` fallback chain was read to line 736; deeper provider-specific retry logic (e.g., vLLM vs. Ollama response shapes) was not exhaustively compared. (3) The `src/api/streaming.rs` UTF-8 partial-chunk buffering (`append_utf8_chunk`) was seen in the call site but its implementation body was not independently verified against multi-byte boundary splits.