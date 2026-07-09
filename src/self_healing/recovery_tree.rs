//! Failure-Resolution Tree for offline-capable error recovery.
//!
//! This module provides:
//! - [`FailureKind`] — a fine-grained taxonomy of failure categories.
//! - [`classify`] — case-insensitive substring matching from an error message
//!   to a [`FailureKind`].
//! - [`FailureSignal`] / [`ResolutionOutcome`] / [`RecoveryContext`] — the
//!   data types that flow through the resolution pipeline.
//! - [`Resolution`] trait — pluggable resolvers that inspect config and
//!   produce a [`RecoveryAction`] **without needing network access**.
//! - [`LocalEndpointFallback`] — flagship resolver that switches to a local
//!   LLM endpoint when the remote one is unreachable.
//! - [`RecoveryTree`] — a registry that maps each [`FailureKind`] to an
//!   ordered list of resolvers (offline-first).
//! - [`DeadlockDetector`] — a lightweight heuristic detector for deadlock
//!   conditions based on repeated no-progress waits.
//!
//! **INCREMENT 1**: This module is self-contained and is NOT yet wired into
//! the agent loop.  Future increments will connect it.

use super::RecoveryAction;
use std::collections::HashMap;

// ============================================================================
// Failure taxonomy
// ============================================================================

/// Fine-grained classification of failures that the recovery system can
/// reason about.
///
/// Every variant maps (or will map, in future increments) to one or more
/// [`Resolution`](trait.Resolution.html) strategies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureKind {
    /// The LLM endpoint cannot be reached (network/DNS/refused).
    LlmUnreachable,
    /// Authentication or authorization failure (401/403/bad key).
    AuthError,
    /// Rate limiting (HTTP 429 / too many requests).
    RateLimited,
    /// A tool call timed out.
    ToolTimeout,
    /// A tool call returned a non-timeout error.
    ToolError,
    /// The conversation exceeded the model's context/token window.
    ContextOverflow,
    /// A deadlock was detected (no progress while waiting).
    Deadlock,
    /// The configuration is invalid or incomplete.
    ConfigInvalid,
    /// The disk is full (ENOSPC).
    DiskFull,
    /// Anything we don't recognise yet.
    Unknown,
}

// ============================================================================
// classify()
// ============================================================================

/// Classify an error message into a [`FailureKind`] using case-insensitive
/// substring matching.
///
/// The classification order is deliberate:
///
/// 1. **LlmUnreachable** is checked first so that connection-oriented phrases
///    like `"timed out connecting"` are not mis-attributed to a generic
///    `"timed out"`.
/// 2. **AuthError** and **RateLimited** are checked before **ToolTimeout** so
///    HTTP status codes (401/403/429) take priority over a generic
///    `"timeout"` substring.
/// 3. **ToolTimeout** catches `"tool"`, `"timeout"`, or `"timed out"` that
///    did not match a connect phrase above.
/// 4. **ContextOverflow**, **ConfigInvalid**, **DiskFull** follow.
/// 5. Anything else falls through to **Unknown**.
///
/// `ToolError` is intentionally never returned by `classify` — it is reserved
/// for callers that already know the failure originated inside a tool but
/// want to route it through the recovery tree.
pub fn classify(error_message: &str) -> FailureKind {
    let msg = error_message.to_lowercase();

    // ---- LlmUnreachable (connection-level) ----
    let llm_unreachable_markers = [
        "connection refused",
        "connect",
        "timed out connecting",
        "dns",
        "unreachable",
        "tcp",
    ];
    for marker in &llm_unreachable_markers {
        if msg.contains(marker) {
            return FailureKind::LlmUnreachable;
        }
    }

    // ---- AuthError ----
    let auth_markers = ["401", "403", "unauthorized", "invalid api key", "authentication"];
    for marker in &auth_markers {
        if msg.contains(marker) {
            return FailureKind::AuthError;
        }
    }

    // ---- RateLimited ----
    let rate_markers = ["429", "rate limit", "too many requests"];
    for marker in &rate_markers {
        if msg.contains(marker) {
            return FailureKind::RateLimited;
        }
    }

    // ---- ToolTimeout (tool/timeout/timed out, but not a connect phrase) ----
    let tool_timeout_markers = ["tool", "timeout", "timed out"];
    for marker in &tool_timeout_markers {
        if msg.contains(marker) {
            return FailureKind::ToolTimeout;
        }
    }

    // ---- ContextOverflow ----
    let context_markers = ["context", "token limit", "maximum context", "too many tokens"];
    for marker in &context_markers {
        if msg.contains(marker) {
            return FailureKind::ContextOverflow;
        }
    }

    // ---- ConfigInvalid ----
    let config_markers = ["config", "invalid configuration", "missing field"];
    for marker in &config_markers {
        if msg.contains(marker) {
            return FailureKind::ConfigInvalid;
        }
    }

    // ---- DiskFull ----
    let disk_markers = ["no space", "disk full", "enospc"];
    for marker in &disk_markers {
        if msg.contains(marker) {
            return FailureKind::DiskFull;
        }
    }

    // ---- Fallback ----
    FailureKind::Unknown
}

// ============================================================================
// Signal / Outcome / Context
// ============================================================================

/// A signal emitted when a failure is observed, carrying the classified kind
/// and the original message for downstream context.
#[derive(Debug, Clone)]
pub struct FailureSignal {
    /// The classified failure category.
    pub kind: FailureKind,
    /// The original error message (for logging or further inspection).
    pub message: String,
}

impl FailureSignal {
    /// Convenience constructor that classifies the message for you.
    pub fn from_message(message: &str) -> Self {
        Self {
            kind: classify(message),
            message: message.to_string(),
        }
    }
}

/// The result of attempting to resolve a failure.
#[derive(Debug, Clone)]
pub enum ResolutionOutcome {
    /// The resolver produced a concrete [`RecoveryAction`].
    Resolved(RecoveryAction),
    /// The resolver recognised the failure but cannot handle it — escalate
    /// to the next resolver (or to a human/outer loop).
    Escalate,
    /// No resolver is registered for this failure kind.
    Unresolvable,
}

/// Context handed to every [`Resolution`] implementation.
///
/// The context is deliberately lightweight: it borrows the active config and
/// the failure message.  Resolvers must **not** perform network I/O — they
/// inspect config and return an action, keeping the recovery decision
/// offline-capable and deterministic.
pub struct RecoveryContext<'a> {
    /// The active Selfware configuration.
    pub config: &'a crate::config::Config,
    /// The raw error message that triggered recovery.
    pub message: &'a str,
}

// ============================================================================
// Resolution trait
// ============================================================================

/// A pluggable resolver that examines a [`RecoveryContext`] and decides
/// whether it can produce a [`RecoveryAction`].
///
/// Implementations are stored inside [`RecoveryTree`] and invoked in order
/// (offline-first) until one returns [`ResolutionOutcome::Resolved`].
pub trait Resolution: Send + Sync {
    /// Human-readable name for logging/telemetry.
    fn name(&self) -> &'static str;

    /// Whether this resolver works without network access.
    ///
    /// `RecoveryTree::with_defaults` orders resolvers so that offline-capable
    /// ones are tried before any online ones.
    fn offline_capable(&self) -> bool;

    /// Attempt to resolve the failure described by `ctx`.
    fn resolve(&self, ctx: &RecoveryContext) -> ResolutionOutcome;
}

// ============================================================================
// LocalEndpointFallback — flagship resolver
// ============================================================================

/// Flagship offline resolver: when the configured LLM endpoint is unreachable
/// **and** the current endpoint is remote, switch to a local endpoint.
///
/// Resolution logic:
/// 1. If `config.endpoint` is **already** local → `Escalate` (nothing to
///    fall back *to*).
/// 2. Otherwise prefer a local endpoint that may already be present in
///    config metadata.  Since the current `Config` struct only exposes a
///    single `endpoint`, we fall back to a well-known local default:
///    `http://localhost:11434/v1` (Ollama's OpenAI-compatible server).
/// 3. Return `Resolved(Fallback { target })`.
///
/// This resolver is fully offline: it only inspects config strings and returns
/// an action.  The agent loop (or a future increment) is responsible for
/// actually switching the endpoint and retrying.
pub struct LocalEndpointFallback;

/// Well-known local endpoint for Ollama's OpenAI-compatible API.
const OLLAMA_LOCAL_ENDPOINT: &str = "http://localhost:11434/v1";

impl LocalEndpointFallback {
    /// Determine the best local endpoint to fall back to.
    ///
    /// Currently this is the well-known Ollama default.  Future increments
    /// may inspect a list of configured local endpoints and prefer the first
    /// reachable one.
    fn pick_local_endpoint(_config: &crate::config::Config) -> String {
        OLLAMA_LOCAL_ENDPOINT.to_string()
    }
}

impl Resolution for LocalEndpointFallback {
    fn name(&self) -> &'static str {
        "LocalEndpointFallback"
    }

    fn offline_capable(&self) -> bool {
        true
    }

    fn resolve(&self, ctx: &RecoveryContext) -> ResolutionOutcome {
        // Only relevant for LLM reachability failures.
        if classify(ctx.message) != FailureKind::LlmUnreachable {
            return ResolutionOutcome::Escalate;
        }

        let current = &ctx.config.endpoint;

        // If we're already on a local endpoint, there's nowhere to fall back.
        if crate::config::is_local_endpoint(current) {
            return ResolutionOutcome::Escalate;
        }

        // Choose a local endpoint and emit a Fallback action.
        let target = Self::pick_local_endpoint(ctx.config);
        ResolutionOutcome::Resolved(RecoveryAction::Fallback { target })
    }
}

// ============================================================================
// RecoveryTree
// ============================================================================

/// A registry mapping each [`FailureKind`] to an ordered list of resolvers.
///
/// Resolvers are tried in registration order.  `with_defaults` inserts
/// offline-capable resolvers first, then online ones, so that the system
/// prefers resolutions that do not require network access.
pub struct RecoveryTree {
    resolvers: HashMap<FailureKind, Vec<Box<dyn Resolution>>>,
}

impl RecoveryTree {
    /// Create an empty tree.
    pub fn new() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    /// Create a tree pre-populated with the default set of resolvers.
    ///
    /// Currently registers:
    /// - [`LocalEndpointFallback`] under [`FailureKind::LlmUnreachable`].
    ///
    /// Resolvers are ordered offline-first.
    pub fn with_defaults() -> Self {
        let mut tree = Self::new();

        // LlmUnreachable → LocalEndpointFallback (offline)
        tree.register(FailureKind::LlmUnreachable, Box::new(LocalEndpointFallback));

        tree
    }

    /// Register an additional resolver for a given failure kind.
    ///
    /// Resolvers added later are appended to the end of the list and thus
    /// tried after previously registered ones.
    pub fn register(&mut self, kind: FailureKind, resolver: Box<dyn Resolution>) {
        self.resolvers.entry(kind).or_default().push(resolver);
    }

    /// Attempt to resolve a [`FailureSignal`] using the registered resolvers.
    ///
    /// Walks the resolvers for `signal.kind` in order:
    /// - Returns the first [`ResolutionOutcome::Resolved`].
    /// - If every resolver returns [`ResolutionOutcome::Escalate`], returns
    ///   [`ResolutionOutcome::Escalate`].
    /// - If no resolvers are registered for this kind, returns
    ///   [`ResolutionOutcome::Unresolvable`].
    pub fn resolve(&self, signal: &FailureSignal, ctx: &RecoveryContext) -> ResolutionOutcome {
        match self.resolvers.get(&signal.kind) {
            Some(list) => {
                let mut saw_escalate = false;
                for resolver in list {
                    match resolver.resolve(ctx) {
                        ResolutionOutcome::Resolved(action) => {
                            return ResolutionOutcome::Resolved(action);
                        }
                        ResolutionOutcome::Escalate => {
                            saw_escalate = true;
                        }
                        ResolutionOutcome::Unresolvable => {
                            // A single Unresolvable short-circuits.
                            return ResolutionOutcome::Unresolvable;
                        }
                    }
                }
                if saw_escalate {
                    ResolutionOutcome::Escalate
                } else {
                    // All resolvers returned Unresolvable — but we only reach
                    // here if the loop didn't short-circuit, which means the
                    // list was empty (handled above) or every entry was
                    // Unresolvable and we returned early.  Defensive fallback:
                    ResolutionOutcome::Escalate
                }
            }
            None => ResolutionOutcome::Unresolvable,
        }
    }
}

impl Default for RecoveryTree {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// ============================================================================
// DeadlockDetector
// ============================================================================

/// A lightweight heuristic detector for deadlock conditions.
///
/// The detector counts consecutive observations where the system was
/// **waiting** (e.g. for a lock or a tool result) but made **no progress**
/// (no state change, no tokens produced).  When the count reaches the
/// configured threshold, a [`FailureKind::Deadlock`] is emitted and the
/// counter resets.
///
/// Any observation that reports progress resets the counter to zero.
pub struct DeadlockDetector {
    /// Current number of consecutive no-progress waits.
    no_progress_waits: u32,
    /// Number of consecutive no-progress waits required to flag a deadlock.
    threshold: u32,
}

impl DeadlockDetector {
    /// Create a detector that flags a deadlock after `threshold` consecutive
    /// waiting-and-no-progress observations.
    pub fn new(threshold: u32) -> Self {
        Self {
            no_progress_waits: 0,
            threshold,
        }
    }

    /// Observe one step of system behaviour.
    ///
    /// - If `made_progress` is `true`, reset the counter and return `None`.
    /// - If `is_waiting` is `true` and `made_progress` is `false`, increment
    ///   the counter.  When it reaches `threshold`, return
    ///   `Some(FailureKind::Deadlock)` and reset the counter.
    /// - Otherwise return `None`.
    pub fn observe(&mut self, is_waiting: bool, made_progress: bool) -> Option<FailureKind> {
        if made_progress {
            self.no_progress_waits = 0;
            return None;
        }

        if is_waiting {
            self.no_progress_waits += 1;
            if self.no_progress_waits >= self.threshold {
                self.no_progress_waits = 0;
                return Some(FailureKind::Deadlock);
            }
        }

        None
    }

    /// Current number of consecutive no-progress waits (for testing).
    pub fn current_count(&self) -> u32 {
        self.no_progress_waits
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    // ------------------------------------------------------------------
    // classify()
    // ------------------------------------------------------------------

    #[test]
    fn test_classify_llm_unreachable() {
        assert_eq!(classify("Connection refused"), FailureKind::LlmUnreachable);
        assert_eq!(classify("dns resolution failed"), FailureKind::LlmUnreachable);
        assert_eq!(classify("host unreachable"), FailureKind::LlmUnreachable);
        assert_eq!(classify("tcp reset"), FailureKind::LlmUnreachable);
        assert_eq!(
            classify("timed out connecting to upstream"),
            FailureKind::LlmUnreachable
        );
    }

    #[test]
    fn test_classify_auth_error() {
        assert_eq!(classify("401 Unauthorized"), FailureKind::AuthError);
        assert_eq!(classify("invalid api key"), FailureKind::AuthError);
        assert_eq!(classify("authentication failed"), FailureKind::AuthError);
        assert_eq!(classify("403 Forbidden"), FailureKind::AuthError);
    }

    #[test]
    fn test_classify_rate_limited() {
        assert_eq!(classify("429 Too Many Requests"), FailureKind::RateLimited);
        assert_eq!(classify("rate limit exceeded"), FailureKind::RateLimited);
    }

    #[test]
    fn test_classify_tool_timeout() {
        assert_eq!(
            classify("tool execution timed out"),
            FailureKind::ToolTimeout
        );
        assert_eq!(classify("operation timeout"), FailureKind::ToolTimeout);
    }

    #[test]
    fn test_classify_context_overflow() {
        assert_eq!(
            classify("context length exceeded"),
            FailureKind::ContextOverflow
        );
        assert_eq!(classify("token limit reached"), FailureKind::ContextOverflow);
        assert_eq!(
            classify("maximum context window exceeded"),
            FailureKind::ContextOverflow
        );
        assert_eq!(classify("too many tokens"), FailureKind::ContextOverflow);
    }

    #[test]
    fn test_classify_config_invalid() {
        assert_eq!(
            classify("invalid configuration: missing endpoint"),
            FailureKind::ConfigInvalid
        );
        assert_eq!(classify("missing field 'api_key'"), FailureKind::ConfigInvalid);
        assert_eq!(classify("config parse error"), FailureKind::ConfigInvalid);
    }

    #[test]
    fn test_classify_disk_full() {
        assert_eq!(classify("no space left on device"), FailureKind::DiskFull);
        assert_eq!(classify("disk full"), FailureKind::DiskFull);
        assert_eq!(classify("ENOSPC"), FailureKind::DiskFull);
    }

    #[test]
    fn test_classify_unknown() {
        assert_eq!(
            classify("something completely unexpected"),
            FailureKind::Unknown
        );
        assert_eq!(classify("wibble"), FailureKind::Unknown);
    }

    #[test]
    fn test_classify_case_insensitive() {
        assert_eq!(classify("CONNECTION REFUSED"), FailureKind::LlmUnreachable);
        assert_eq!(classify("Unauthorized"), FailureKind::AuthError);
        assert_eq!(classify("RATE LIMIT"), FailureKind::RateLimited);
    }

    #[test]
    fn test_classify_connect_phrase_wins_over_timeout() {
        // "timed out connecting" must classify as LlmUnreachable, not ToolTimeout
        assert_eq!(
            classify("Error: timed out connecting to https://api"),
            FailureKind::LlmUnreachable
        );
    }

    // ------------------------------------------------------------------
    // FailureSignal
    // ------------------------------------------------------------------

    #[test]
    fn test_failure_signal_from_message() {
        let sig = FailureSignal::from_message("connection refused");
        assert_eq!(sig.kind, FailureKind::LlmUnreachable);
        assert_eq!(sig.message, "connection refused");
    }

    // ------------------------------------------------------------------
    // LocalEndpointFallback
    // ------------------------------------------------------------------

    fn make_config(endpoint: &str) -> Config {
        let mut config = Config::default();
        config.endpoint = endpoint.to_string();
        config
    }

    #[test]
    fn test_local_endpoint_fallback_remote_config_resolves() {
        let config = make_config("https://openrouter.ai/api/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "connection refused",
        };
        let resolver = LocalEndpointFallback;
        let outcome = resolver.resolve(&ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryAction::Fallback { target }) => {
                assert!(
                    target.contains("localhost") || target.contains("127.0.0.1"),
                    "expected a local target, got {}",
                    target
                );
            }
            other => panic!("expected Resolved(Fallback), got {:?}", other),
        }
    }

    #[test]
    fn test_local_endpoint_fallback_local_config_escalates() {
        let config = make_config("http://localhost:11434/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "connection refused",
        };
        let resolver = LocalEndpointFallback;
        let outcome = resolver.resolve(&ctx);
        match outcome {
            ResolutionOutcome::Escalate => {}
            other => panic!("expected Escalate, got {:?}", other),
        }
    }

    #[test]
    fn test_local_endpoint_fallback_non_llm_escalates() {
        let config = make_config("https://openrouter.ai/api/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "rate limit exceeded",
        };
        let resolver = LocalEndpointFallback;
        // Non-LlmUnreachable should escalate
        match resolver.resolve(&ctx) {
            ResolutionOutcome::Escalate => {}
            other => panic!("expected Escalate, got {:?}", other),
        }
    }

    #[test]
    fn test_local_endpoint_fallback_offline_capable() {
        let resolver = LocalEndpointFallback;
        assert!(resolver.offline_capable());
        assert_eq!(resolver.name(), "LocalEndpointFallback");
    }

    // ------------------------------------------------------------------
    // RecoveryTree
    // ------------------------------------------------------------------

    #[test]
    fn test_recovery_tree_with_defaults_resolves_llm_unreachable() {
        let tree = RecoveryTree::with_defaults();
        let config = make_config("https://openrouter.ai/api/v1");
        let signal = FailureSignal {
            kind: FailureKind::LlmUnreachable,
            message: "connection refused".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        let outcome = tree.resolve(&signal, &ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryAction::Fallback { target }) => {
                assert!(
                    target.contains("localhost") || target.contains("127.0.0.1"),
                    "expected a local target, got {}",
                    target
                );
            }
            other => panic!("expected Resolved(Fallback), got {:?}", other),
        }
    }

    #[test]
    fn test_recovery_tree_no_resolvers_returns_unresolvable() {
        let tree = RecoveryTree::new(); // empty
        let config = make_config("https://example.com");
        let signal = FailureSignal {
            kind: FailureKind::Deadlock,
            message: "deadlock".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        match tree.resolve(&signal, &ctx) {
            ResolutionOutcome::Unresolvable => {}
            other => panic!("expected Unresolvable, got {:?}", other),
        }
    }

    #[test]
    fn test_recovery_tree_all_escalate_returns_escalate() {
        // LocalEndpointFallback on a local config escalates.
        let tree = RecoveryTree::with_defaults();
        let config = make_config("http://localhost:11434/v1");
        let signal = FailureSignal {
            kind: FailureKind::LlmUnreachable,
            message: "connection refused".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        match tree.resolve(&signal, &ctx) {
            ResolutionOutcome::Escalate => {}
            other => panic!("expected Escalate, got {:?}", other),
        }
    }

    #[test]
    fn test_recovery_tree_custom_resolver_registered() {
        struct AlwaysRetry;
        impl Resolution for AlwaysRetry {
            fn name(&self) -> &'static str {
                "AlwaysRetry"
            }
            fn offline_capable(&self) -> bool {
                true
            }
            fn resolve(&self, _ctx: &RecoveryContext) -> ResolutionOutcome {
                ResolutionOutcome::Resolved(RecoveryAction::Retry {
                    delay_ms: 500,
                    max_attempts: 1,
                })
            }
        }

        let mut tree = RecoveryTree::new();
        tree.register(FailureKind::Unknown, Box::new(AlwaysRetry));

        let config = make_config("https://example.com");
        let signal = FailureSignal {
            kind: FailureKind::Unknown,
            message: "weird error".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        let outcome = tree.resolve(&signal, &ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryAction::Retry { delay_ms, .. }) => {
                assert_eq!(delay_ms, 500);
            }
            other => panic!("expected Resolved(Retry), got {:?}", other),
        }
    }

    #[test]
    fn test_recovery_tree_default_impl() {
        // Default::default() should equal with_defaults()
        let tree = RecoveryTree::default();
        let config = make_config("https://openrouter.ai/api/v1");
        let signal = FailureSignal {
            kind: FailureKind::LlmUnreachable,
            message: "connection refused".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        // Should resolve (not Unresolvable or Escalate)
        let outcome = tree.resolve(&signal, &ctx);
        match outcome {
            ResolutionOutcome::Resolved(_) => {}
            other => panic!("expected Resolved, got {:?}", other),
        }
    }

    // ------------------------------------------------------------------
    // DeadlockDetector
    // ------------------------------------------------------------------

    #[test]
    fn test_deadlock_detector_triggers_after_threshold() {
        let mut detector = DeadlockDetector::new(3);

        // First two no-progress waits → None
        assert_eq!(detector.observe(true, false), None);
        assert_eq!(detector.observe(true, false), None);
        assert_eq!(detector.current_count(), 2);

        // Third → Deadlock, counter resets
        let result = detector.observe(true, false);
        assert_eq!(result, Some(FailureKind::Deadlock));
        assert_eq!(detector.current_count(), 0);
    }

    #[test]
    fn test_deadlock_detector_progress_resets() {
        let mut detector = DeadlockDetector::new(3);

        detector.observe(true, false);
        detector.observe(true, false);
        assert_eq!(detector.current_count(), 2);

        // Progress resets
        assert_eq!(detector.observe(true, true), None);
        assert_eq!(detector.current_count(), 0);

        // Need 3 more to trigger
        assert_eq!(detector.observe(true, false), None);
        assert_eq!(detector.observe(true, false), None);
        assert_eq!(detector.observe(true, false), Some(FailureKind::Deadlock));
    }

    #[test]
    fn test_deadlock_detector_not_waiting_does_not_increment() {
        let mut detector = DeadlockDetector::new(2);
        // Not waiting, no progress → counter stays 0
        assert_eq!(detector.observe(false, false), None);
        assert_eq!(detector.current_count(), 0);
        assert_eq!(detector.observe(false, false), None);
        assert_eq!(detector.current_count(), 0);
    }

    #[test]
    fn test_deadlock_detector_threshold_one() {
        let mut detector = DeadlockDetector::new(1);
        assert_eq!(detector.observe(true, false), Some(FailureKind::Deadlock));
        assert_eq!(detector.current_count(), 0);
    }
}
