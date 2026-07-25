# Visual Embeddings (Qwen3-VL-Embedding) Integration Plan

> **Status:** Blocked on provider availability — as of 2026-07-25 the model is
> NOT on OpenRouter (`qwen/qwen3-vl-embedding-8b` → "Model does not exist";
> no embedding models are listed in `/v1/models` at all, though
> `qwen/qwen3-embedding-8b` works via `/v1/embeddings`). Execute when it
> lands on OpenRouter or another OpenAI-compatible endpoint is configured.
> User decision 2026-07-25: no local runtimes (Ollama etc.) — hosted only.

**Goal:** Index and retrieve visual artifacts (UI screenshots, diagrams,
document images) alongside code text via Qwen3-VL-Embedding, and evaluate
retrieval quality ("feedback") with a repeatable harness.

**Availability probe (re-run any time):**

```bash
curl -s https://openrouter.ai/api/v1/embeddings \
  -H "Authorization: Bearer $OPENROUTER_API_KEY" \
  -H "content-type: application/json" \
  -d '{"model":"qwen/qwen3-vl-embedding-8b","input":["test"]}'
# "does not exist"  → still blocked
# embedding vector  → unblocked, start Task 1
```

## Background (verified 2026-07-25)

- Models: Qwen3-VL-Embedding-2B (2048-dim) / -8B (4096-dim), 32k context,
  Matryoshka (MRL) truncation 64..full, instruction-aware (custom instruction
  = +1-5% retrieval per Qwen evals), 30+ languages, MMEB-V2 77.8 (8B).
- Inputs: text, images (≤1,280 visual tokens), video frames, mixed blocks.
- Already in place here: `EmbeddingProvider` trait, `HttpEmbeddingProvider`
  (bearer auth fixed 2026-07-25, commit cc8ec3ba), `[models.embedding]`
  profile wiring into RagEngine, vector store with configurable dimension.

## Task 1: Multimodal input support in `HttpEmbeddingProvider`

**Files:** `src/analysis/vector_store.rs`, tests in
`tests/unit/analysis/vector_store/vector_store_test.rs`

- Add `embed_blocks(&self, blocks: Vec<EmbeddingInput>) -> Result<Vec<f32>>`
  where `EmbeddingInput` is `{ Text(String) | ImageDataUrl { mime, base64 } }`,
  serialized as OpenAI content blocks
  (`{"type":"text","text":..}` / `{"type":"image_url","image_url":{"url":"data:.."}}`).
- The exact wire shape for VL embeddings on OpenRouter is UNVERIFIED (model
  not live); validate against the provider error messages and Qwen's API
  docs at execution time. Fall back to DashScope's documented
  `input: [{"image": "..."}, {"text": "..."}]` shape if the OpenAI block
  shape is rejected.
- Trait: add `embed_multimodal` to `EmbeddingProvider` with a default
  text-only error impl (TfIdf/Mock keep working); `EmbeddingBackend` gets a
  matching enum dispatch.
- Tests: fake-server test asserting the block JSON shape; live ignored test
  gated on `OPENROUTER_API_KEY` (mirrors `live_openrouter_qwen3_embedding`).

## Task 2: Image indexing in RagEngine

**Files:** `src/cognitive/rag.rs`, `src/agent/interactive.rs`

- Extend the RAG index scan: for `.png/.jpg/.jpeg/.webp` files, produce
  `ChunkType::Image` chunks whose embedding comes from `embed_blocks` (image
  bytes base64 + optional caption text) instead of text.
- Config: `[models.embedding]` gains optional `dimensions` (MRL truncation,
  default = model full) and `instruction` (retrieval instruction prefix).
  Stop overloading `context_length` as the dimension hint when `dimensions`
  is set.
- Cap image size before base64 (resize/compress if > provider visual-token
  budget, ~1.3M px for this model).

## Task 3: Retrieval evaluation harness ("feedback")

**Files:** `scripts/embedding_quality_bench.sh` (new),
`docs/quant_bench/YYYY-MM-DD-embedding-quality.md` (results)

- Corpus (this repo): 20 component-card texts (from `evolve::map`), 5 UI
  screenshots (headless Chrome, we already do this), 5 diagram images.
- Queries (10, mixed): e.g. "context tier picker UI" (should hit the
  screenshot), "comment stripping module" (should hit context_reduce card),
  "architecture overview" (diagram).
- Score: top-1 and top-3 hit rates per query, cosine margin between correct
  and best-wrong hit, per modality. Compare qwen3-embedding-8b (text-only
  baseline) vs qwen3-vl-embedding-8b (text+image).
- Deliverable doc table + one-line recommendation on which to configure.

## Task 4: Reranker (optional, same availability gate)

Qwen3-VL-Reranker-8B (single-tower yes/no scorer) as a second pass over
top-k results from Task 3's index. Only if the endpoint exposes it; measure
hit-rate delta vs embedding-only.

## Out of scope

- Local runtimes (Ollama/llama.cpp/vLLM) — user decision, hosted endpoints only.
- Video embeddings (no use case in this repo today).
