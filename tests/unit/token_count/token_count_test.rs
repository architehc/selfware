use super::*;

#[test]
fn test_estimate_content_tokens_non_zero() {
    let tokens = estimate_content_tokens("hello world");
    assert!(tokens > 0);
}

#[test]
fn test_estimate_tokens_with_overhead() {
    let tokens = estimate_tokens_with_overhead("hello", 10);
    assert!(tokens >= 11);
}

#[test]
fn test_estimate_content_tokens_code() {
    let tokens = estimate_content_tokens("fn main() { println!(\"hi\"); }");
    assert!(tokens > 0);
}

#[test]
fn test_cache_returns_consistent_results() {
    let content = "The quick brown fox jumps over the lazy dog";
    let first = estimate_content_tokens(content);
    // Second call should hit the cache and return the same value.
    let second = estimate_content_tokens(content);
    assert_eq!(first, second);
}

// ── Code token-count calibration (2026-09-21 review, P2) ────────────────
//
// The generic tokenizer (cl100k / HF fallbacks) systematically UNDER-COUNTS
// code: measured on the production context path, a 350k context cap
// admitted ~385k actual server tokens (≈9-10% low), pushing prefill to
// ~51s against a 60s cliff. `estimate_content_tokens` now multiplies
// code-shaped content by the measured `CODE_CALIBRATION_FACTOR`. These
// tests pin the calibration: representative code corpora must no longer be
// systematically low, prose must be untouched, and the measured constant
// must not drift.

/// Representative code samples large enough to trigger calibration
/// (raw token count ≥ CODE_SHAPE_MIN_TOKENS). Each must be classified as
/// code-shaped.
fn code_corpus() -> Vec<&'static str> {
    let rust = "
pub fn merge_sort<T: Ord + Clone>(mut items: Vec<T>) -> Vec<T> {
    if items.len() <= 1 { return items; }
    let mid = items.len() / 2;
    let right = merge_sort(items.split_off(mid));
    let mut left = merge_sort(items);
    let mut out = Vec::with_capacity(left.len() + right.len());
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        if left[i] <= right[j] { out.push(left[i].clone()); i += 1; }
        else { out.push(right[j].clone()); j += 1; }
    }
    out.extend(left.drain(i..));
    out.extend(right.drain(j..));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sorts_already_sorted() {
        assert_eq!(merge_sort(vec![1, 2, 3]), vec![1, 2, 3]);
    }
    #[test]
    fn sorts_reversed() {
        assert_eq!(merge_sort(vec![3, 2, 1]), vec![1, 2, 3]);
    }
}
";
    let python = "
import json
from collections import defaultdict


def build_index(documents):
    index = defaultdict(list)
    for doc_id, text in enumerate(documents):
        for token in text.lower().split():
            index[token].append(doc_id)
    return dict(index)


def search(index, query):
    results = set()
    for term in query.lower().split():
        if term in index:
            results.update(index[term])
    return sorted(results)


def main():
    docs = ['the quick brown fox', 'jumps over the lazy dog', 'the fox and the dog']
    idx = build_index(docs)
    found = search(idx, 'fox dog')
    for doc in found:
        print(doc)


if __name__ == '__main__':
    main()
";
    let typescript = "
interface Repository<T> {
    findById(id: string): Promise<T | null>;
    findAll(query?: Partial<T>): Promise<T[]>;
    save(entity: T): Promise<T>;
    delete(id: string): Promise<boolean>;
}

class InMemoryRepository<T> implements Repository<T> {
    private store = new Map<string, T>();
    async findById(id: string): Promise<T | null> {
        return this.store.get(id) ?? null;
    }
    async findAll(query?: Partial<T>): Promise<T[]> {
        if (!query) { return [...this.store.values()]; }
        return [...this.store.values()].filter((entry) =>
            Object.entries(query).every(([key, value]) => entry[key] === value)
        );
    }
    async save(entity: T): Promise<T> {
        const id = (entity as any).id ?? crypto.randomUUID();
        this.store.set(id, entity);
        return entity;
    }
    async delete(id: string): Promise<boolean> {
        return this.store.delete(id);
    }
}

export const getRepository = <T>(): Repository<T> => new InMemoryRepository<T>();
";
    let c = "
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct node {
    int value;
    struct node *next;
} node_t;

static node_t *head = NULL;

void push_front(int value) {
    node_t *n = malloc(sizeof(node_t));
    n->value = value;
    n->next = head;
    head = n;
}

int pop_front(void) {
    if (!head) { return -1; }
    node_t *old = head;
    int value = old->value;
    head = old->next;
    free(old);
    return value;
}

int contains(int value) {
    for (node_t *cur = head; cur; cur = cur->next) {
        if (cur->value == value) { return 1; }
    }
    return 0;
}

int main(void) {
    push_front(3);
    push_front(2);
    push_front(1);
    printf(\"%d %d %d\\n\", contains(1), contains(2), contains(9));
    return 0;
}
";
    let json = r#"{
  "service": "api-gateway",
  "listeners": [
    { "port": 8080, "protocol": "https", "tls": { "enabled": true, "min_version": "1.2" } },
    { "port": 8081, "protocol": "http", "tls": { "enabled": false } }
  ],
  "routing": [
    { "path": "/v1/users", "upstream": "users-svc", "methods": ["GET", "POST", "PUT"] },
    { "path": "/v1/orders", "upstream": "orders-svc", "methods": ["GET", "DELETE"] },
    { "path": "/v1/invoices", "upstream": "billing-svc", "methods": ["GET", "POST"] }
  ],
  "limits": {
    "requests_per_second": 1000,
    "body_max_bytes": 1048576,
    "timeout_ms": { "connect": 500, "read": 3000, "write": 3000 }
  }
}"#;
    vec![rust, python, typescript, c, json]
}

/// Representative PROSE samples: must NOT be classified code-shaped and
/// must pass through `apply_code_calibration` unchanged.
fn prose_corpus() -> Vec<String> {
    let article = "The gateway forwards requests to upstream services after authenticating \
        the caller. Retries are bounded and the circuit breaker opens after five consecutive \
        failures, giving the backend a chance to recover before the next attempt. Observability \
        stays unchanged: every hop emits a span so a slow read surfaces in the existing \
        dashboards without new instrumentation.";
    let prose = article.repeat(12); // large enough that raw count would clear the floor
    let chat = "Thanks for the detailed write-up. The tradeoff is clear now: we accept the \
        latency increase on cold starts and keep the simpler deployment model. I will \
        summarize the decision in the meeting notes and flag the follow-up items for next \
        week, then close the thread.";
    vec![prose, chat.to_string()]
}

#[test]
fn every_code_sample_is_classified_code_shaped() {
    for sample in code_corpus() {
        let raw = TOKENIZER.count(sample);
        assert!(
            raw >= CODE_SHAPE_MIN_TOKENS,
            "test corpus sample too small to exercise calibration: raw={raw} tokens"
        );
        assert!(
            is_code_shaped(sample),
            "representative code sample must be classified code-shaped"
        );
        let calibrated = apply_code_calibration(raw, sample);
        assert!(
            calibrated >= raw,
            "calibration must never shrink a code estimate"
        );
    }
}

#[test]
fn code_corpus_is_no_longer_systematically_low() {
    let samples = code_corpus();
    let raw_total: usize = samples.iter().map(|s| TOKENIZER.count(s)).sum();
    let calibrated_total: usize = samples
        .iter()
        .map(|s| apply_code_calibration(TOKENIZER.count(s), s))
        .sum();
    // The measured shortfall was ~9%; the calibration must lift the
    // aggregate by at least the measured amount (1.08 lower bound), or the
    // prefill-cliff regression is back.
    assert!(
        calibrated_total >= (raw_total as f32 * 1.08) as usize,
        "code corpus estimate still systematically low: raw={raw_total}, calibrated={calibrated_total}"
    );
    assert!(
        calibrated_total <= (raw_total as f32 * 1.15).ceil() as usize,
        "calibration overshoots the measured shortfall: raw={raw_total}, calibrated={calibrated_total}"
    );
}

#[test]
fn prose_corpus_is_not_calibrated() {
    for sample in prose_corpus() {
        let raw = TOKENIZER.count(&sample);
        assert!(
            !is_code_shaped(&sample),
            "prose must not be classified as code"
        );
        assert_eq!(
            apply_code_calibration(raw, &sample),
            raw,
            "prose token estimates must pass through unchanged"
        );
    }
}

#[test]
fn measured_calibration_factor_is_pinned() {
    // The constant is the measured 385/350 ratio from the 2026-09-21
    // review. If it drifts outside a tight band the pin fails, forcing a
    // re-measurement rather than a silent budget change.
    assert!(
        (1.05..=1.15).contains(&CODE_CALIBRATION_FACTOR),
        "CODE_CALIBRATION_FACTOR drifted from the measured value: {CODE_CALIBRATION_FACTOR}"
    );
}

#[test]
fn test_hash_content_deterministic() {
    let a = hash_content("hello");
    let b = hash_content("hello");
    assert_eq!(a, b);

    let c = hash_content("world");
    assert_ne!(a, c);
}

#[test]
fn test_hf_tokenizer_repo_known_families() {
    // Qwen models should map to a Qwen tokenizer repo
    assert!(hf_tokenizer_repo("Qwen/Qwen2.5-Coder-32B").is_some());
    assert!(hf_tokenizer_repo("qwen2.5-7b").is_some());

    // GLM models should map to a GLM tokenizer repo
    assert!(hf_tokenizer_repo("z-ai/glm-5.2").is_some());
    assert!(hf_tokenizer_repo("THUDM/glm-4-9b").is_some());

    // Other known families
    assert!(hf_tokenizer_repo("gpt-4o").is_some());
    assert!(hf_tokenizer_repo("llama-3-8b").is_some());
    assert!(hf_tokenizer_repo("mistral-7b").is_some());
}

#[test]
fn test_hf_tokenizer_repo_unknown_returns_none() {
    // Unknown model families should return None so the caller
    // falls back to cl100k instead of hardcoding a tokenizer.
    assert!(hf_tokenizer_repo("some-random-model").is_none());
    assert!(hf_tokenizer_repo("").is_none());
}

#[test]
fn test_for_model_falls_back_gracefully() {
    // Using a model name with no mapped HF repo should still produce
    // a working TokenizerState (cl100k or heuristic) — never panic.
    let state = TokenizerState::for_model(Some("unknown-model-xyz"));
    let count = state.count("fn main() { println!(\"hello\"); }");
    assert!(
        count > 0,
        "fallback tokenizer must produce a positive count"
    );
}

#[test]
fn test_for_model_none_falls_back_gracefully() {
    // No model at all should still work — cl100k or heuristic.
    let state = TokenizerState::for_model(None);
    let count = state.count("hello world");
    assert!(count > 0);
}

#[test]
fn test_configured_model_slot_used_when_env_absent() {
    // The CLI registers the loaded config's model here instead of the
    // tokenizer reloading the config itself (P2-11).
    let _guard = crate::test_support::EnvGuard::clear_selfware_env();
    set_configured_model("slot-model");
    assert_eq!(configured_model_name().as_deref(), Some("slot-model"));
}

#[test]
fn test_env_var_beats_configured_model_slot() {
    let _guard = crate::test_support::EnvGuard::clear_selfware_env();
    set_configured_model("slot-model");
    std::env::set_var("SELFWARE_MODEL", "env-model");
    assert_eq!(configured_model_name().as_deref(), Some("env-model"));
    // The guard re-clears SELFWARE_MODEL on drop.
}

// ---- Ported from tests/unit/tokens (wave-3 D1): estimate_messages_tokens ----

#[test]
fn test_estimate_messages_tokens_simple() {
    use crate::api::types::Message;

    let messages = vec![
        Message::system("You are a helpful assistant"),
        Message::user("Hello, how are you?"),
        Message::assistant("I'm doing well, thank you!"),
    ];

    let estimate = estimate_messages_tokens(&messages);
    // At least 4 tokens overhead per message (3 messages) + content
    assert!(estimate > 12);
}

#[test]
fn test_estimate_messages_tokens_with_tool_calls() {
    use crate::api::types::{Message, ToolCall, ToolFunction};

    let mut msg = Message::assistant("Let me read that file for you.");
    msg.tool_calls = Some(vec![ToolCall {
        id: "call_1".to_string(),
        call_type: "function".to_string(),
        function: ToolFunction {
            name: "file_read".to_string(),
            arguments: r#"{"path": "test.txt"}"#.to_string(),
        },
    }]);

    let messages = vec![msg];
    let estimate = estimate_messages_tokens(&messages);

    // Should include tool call overhead
    assert!(estimate > 20);
}

#[test]
fn test_estimate_messages_tokens_empty() {
    let messages: Vec<crate::api::types::Message> = vec![];
    let estimate = estimate_messages_tokens(&messages);
    assert_eq!(estimate, 0);
}

#[test]
fn test_estimate_messages_tokens_counts_reasoning_content() {
    use crate::api::types::Message;

    let mut msg_with_reasoning = Message::assistant("Final answer text.");
    let reasoning_text =
        "This is extensive internal chain-of-thought reasoning that takes up tokens.";
    msg_with_reasoning.reasoning_content = Some(reasoning_text.to_string());

    let msg_without_reasoning = Message::assistant("Final answer text.");

    let tokens_with = estimate_messages_tokens(&[msg_with_reasoning]);
    let tokens_without = estimate_messages_tokens(&[msg_without_reasoning]);

    let expected_diff = estimate_tokens(reasoning_text);
    assert!(
        expected_diff > 0,
        "reasoning text must have non-zero token estimate"
    );
    assert_eq!(
        tokens_with - tokens_without,
        expected_diff,
        "estimate_messages_tokens must count reasoning_content when present"
    );
}
