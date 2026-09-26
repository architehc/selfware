

**Stage 1 findings (API streaming and tool-call parsing):**

From `src/api/streaming.rs` and `tests/unit/api/streaming/streaming_test.rs`:

- **`classify_stream_timeout`** (line 57–85): Correctly prioritizes run deadline > per-call cap > plain timeout. Tests at lines 514–602 confirm all three branches.
- **`collect()`** (line 382+): Handles truncated streams with typed `ApiError::Parse` (lines 481–512), retaining usage in the message. Tests at lines 303–401 confirm.
- **`into_channel()`** (line 177+): Flushes buffered tool calls on deadline (line 244) and on chunk timeout (line 275). CRLF normalization at line 298. Multiline SSE data joining tested at line 71–83.
- **`append_utf8_chunk`**: Handles split multibyte codepoints (test line 5) and invalid bytes with U+FFFD (test line 21).
- **`null_usage_chunks_are_skipped_silently_without_warning`** (test line 451): Confirms `"usage": null` produces no extra chunk and no warning.

No confirmed bugs in Stage 1. The implementation and tests are consistent.

---

Now moving to **Stage 2: Sequential and parallel dispatch**.