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
// RecoveryDirective
// ============================================================================

/// A directive produced by the resolution tree.
///
/// `Action` wraps the classic [`RecoveryAction`] enum (Fallback, Retry, etc.)
/// whose 79+ match sites must not be touched.  The other variants carry
/// richer, module-specific recovery intent that goes beyond what
/// `RecoveryAction` can express — they are applied by
/// `Agent::apply_recovery_action` which now accepts a `&RecoveryDirective`.
#[derive(Debug, Clone)]
pub enum RecoveryDirective {
    /// A standard [`RecoveryAction`] (Fallback, Retry, …).
    Action(RecoveryAction),
    /// Compress the agent's conversation context to roughly `target_tokens`.
    CompressContext { target_tokens: usize },
    /// Reload API credentials from env-var / keyring and rebuild the client.
    ReloadCredentials,
}

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
    /// The resolver produced a concrete [`RecoveryDirective`].
    Resolved(RecoveryDirective),
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

        // Choose a local endpoint and emit a Fallback directive.
        let target = Self::pick_local_endpoint(ctx.config);
        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Fallback {
            target,
        }))
    }
}

// ============================================================================
// RateLimitBackoff — offline resolver for RateLimited
// ============================================================================

/// Offline resolver for rate-limiting failures (HTTP 429 / too many requests).
///
/// Returns `Resolved(Retry { delay_ms, max_attempts })` so the caller backs off
/// and retries the operation.  No network access is required.
pub struct RateLimitBackoff {
    /// Backoff delay in milliseconds between retry attempts.
    pub delay_ms: u64,
    /// Maximum number of retry attempts before giving up.
    pub max_attempts: u32,
}

impl RateLimitBackoff {
    /// Create a new resolver with the given backoff parameters.
    pub fn new(delay_ms: u64, max_attempts: u32) -> Self {
        Self {
            delay_ms,
            max_attempts,
        }
    }
}

impl Resolution for RateLimitBackoff {
    fn name(&self) -> &'static str {
        "RateLimitBackoff"
    }

    fn offline_capable(&self) -> bool {
        true
    }

    fn resolve(&self, ctx: &RecoveryContext) -> ResolutionOutcome {
        // Only relevant for rate-limiting failures.
        if classify(ctx.message) != FailureKind::RateLimited {
            return ResolutionOutcome::Escalate;
        }

        ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
            delay_ms: self.delay_ms,
            max_attempts: self.max_attempts,
        }))
    }
}

// ============================================================================
// ContextOverflowCompress — offline resolver for ContextOverflow
// ============================================================================

/// Offline resolver for context-overflow failures.
///
/// When the conversation exceeds the model's token window, this resolver
/// emits a [`RecoveryDirective::CompressContext`] directive so the agent can
/// compress its message history down to roughly `target_tokens`.  No network
/// access is required — the decision is purely based on the failure kind.
pub struct ContextOverflowCompress {
    /// Target token count to compress the context down to.
    pub target_tokens: usize,
}

impl Resolution for ContextOverflowCompress {
    fn name(&self) -> &'static str {
        "ContextOverflowCompress"
    }

    fn offline_capable(&self) -> bool {
        true
    }

    fn resolve(&self, ctx: &RecoveryContext) -> ResolutionOutcome {
        if classify(ctx.message) != FailureKind::ContextOverflow {
            return ResolutionOutcome::Escalate;
        }
        ResolutionOutcome::Resolved(RecoveryDirective::CompressContext {
            target_tokens: self.target_tokens,
        })
    }
}

// ============================================================================
// CredentialReload — offline resolver for AuthError
// ============================================================================

/// Offline resolver for authentication failures.
///
/// Checks whether a credential source is available **without network**:
/// - `SELFWARE_API_KEY` environment variable, or
/// - the OS system keyring (via [`crate::config::load_api_key_from_keyring`]).
///
/// If a source exists, emits [`RecoveryDirective::ReloadCredentials`] so the
/// agent can reload the key and rebuild its API client.  Otherwise escalates.
pub struct CredentialReload;

impl CredentialReload {
    /// Determine whether a credential source is available offline.
    ///
    /// Returns `Some(key)` if a key can be obtained, or `None` if no source
    /// is available.  This is a pure helper that can be unit-tested directly
    /// without constructing a full [`RecoveryContext`].
    pub fn find_available_key() -> Option<String> {
        if let Ok(key) = std::env::var("SELFWARE_API_KEY") {
            if !key.is_empty() {
                return Some(key);
            }
        }
        match crate::config::load_api_key_from_keyring() {
            Ok(Some(key)) if !key.is_empty() => Some(key),
            _ => None,
        }
    }
}

impl Resolution for CredentialReload {
    fn name(&self) -> &'static str {
        "CredentialReload"
    }

    fn offline_capable(&self) -> bool {
        true
    }

    fn resolve(&self, ctx: &RecoveryContext) -> ResolutionOutcome {
        if classify(ctx.message) != FailureKind::AuthError {
            return ResolutionOutcome::Escalate;
        }
        if Self::find_available_key().is_some() {
            ResolutionOutcome::Resolved(RecoveryDirective::ReloadCredentials)
        } else {
            ResolutionOutcome::Escalate
        }
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

        // RateLimited → RateLimitBackoff (offline)
        tree.register(
            FailureKind::RateLimited,
            Box::new(RateLimitBackoff::new(2000, 3)),
        );

        // ContextOverflow → ContextOverflowCompress (offline)
        tree.register(
            FailureKind::ContextOverflow,
            Box::new(ContextOverflowCompress {
                target_tokens: 8_000,
            }),
        );

        // AuthError → CredentialReload (offline)
        tree.register(FailureKind::AuthError, Box::new(CredentialReload));

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
                        ResolutionOutcome::Resolved(directive) => {
                            return ResolutionOutcome::Resolved(directive);
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
            ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Fallback { target })) => {
                assert!(
                    target.contains("localhost") || target.contains("127.0.0.1"),
                    "expected a local target, got {}",
                    target
                );
            }
            other => panic!("expected Resolved(Action(Fallback)), got {:?}", other),
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
            ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Fallback { target })) => {
                assert!(
                    target.contains("localhost") || target.contains("127.0.0.1"),
                    "expected a local target, got {}",
                    target
                );
            }
            other => panic!("expected Resolved(Action(Fallback)), got {:?}", other),
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
                ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
                    delay_ms: 500,
                    max_attempts: 1,
                }))
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
            ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry { delay_ms, .. })) => {
                assert_eq!(delay_ms, 500);
            }
            other => panic!("expected Resolved(Action(Retry)), got {:?}", other),
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
    // RateLimitBackoff
    // ------------------------------------------------------------------

    #[test]
    fn test_rate_limit_backoff_resolves_rate_limited() {
        let resolver = RateLimitBackoff::new(2000, 3);
        let config = make_config("https://api.example.com/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "429 Too Many Requests",
        };
        let outcome = resolver.resolve(&ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
                delay_ms,
                max_attempts,
            })) => {
                assert_eq!(delay_ms, 2000);
                assert_eq!(max_attempts, 3);
            }
            other => panic!("expected Resolved(Action(Retry)), got {:?}", other),
        }
    }

    #[test]
    fn test_rate_limit_backoff_non_rate_limited_escalates() {
        let resolver = RateLimitBackoff::new(2000, 3);
        let config = make_config("https://api.example.com/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "connection refused",
        };
        match resolver.resolve(&ctx) {
            ResolutionOutcome::Escalate => {}
            other => panic!("expected Escalate, got {:?}", other),
        }
    }

    #[test]
    fn test_rate_limit_backoff_offline_capable() {
        let resolver = RateLimitBackoff::new(2000, 3);
        assert!(resolver.offline_capable());
        assert_eq!(resolver.name(), "RateLimitBackoff");
    }

    #[test]
    fn test_recovery_tree_with_defaults_resolves_rate_limited() {
        let tree = RecoveryTree::with_defaults();
        let config = make_config("https://api.example.com/v1");
        let signal = FailureSignal {
            kind: FailureKind::RateLimited,
            message: "429 Too Many Requests".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        let outcome = tree.resolve(&signal, &ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryDirective::Action(RecoveryAction::Retry {
                delay_ms,
                max_attempts,
            })) => {
                assert_eq!(delay_ms, 2000);
                assert_eq!(max_attempts, 3);
            }
            other => panic!("expected Resolved(Action(Retry)), got {:?}", other),
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

    // ------------------------------------------------------------------
    // ContextOverflowCompress
    // ------------------------------------------------------------------

    #[test]
    fn test_context_overflow_compress_resolves_context_overflow() {
        let resolver = ContextOverflowCompress {
            target_tokens: 8000,
        };
        let config = make_config("https://api.example.com/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "context length exceeded",
        };
        let outcome = resolver.resolve(&ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryDirective::CompressContext {
                target_tokens,
            }) => {
                assert_eq!(target_tokens, 8000);
            }
            other => panic!("expected Resolved(CompressContext), got {:?}", other),
        }
    }

    #[test]
    fn test_context_overflow_compress_non_overflow_escalates() {
        let resolver = ContextOverflowCompress {
            target_tokens: 8000,
        };
        let config = make_config("https://api.example.com/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "connection refused",
        };
        match resolver.resolve(&ctx) {
            ResolutionOutcome::Escalate => {}
            other => panic!("expected Escalate, got {:?}", other),
        }
    }

    #[test]
    fn test_context_overflow_compress_offline_capable() {
        let resolver = ContextOverflowCompress {
            target_tokens: 8000,
        };
        assert!(resolver.offline_capable());
        assert_eq!(resolver.name(), "ContextOverflowCompress");
    }

    #[test]
    fn test_recovery_tree_with_defaults_resolves_context_overflow() {
        let tree = RecoveryTree::with_defaults();
        let config = make_config("https://api.example.com/v1");
        let signal = FailureSignal {
            kind: FailureKind::ContextOverflow,
            message: "context length exceeded".to_string(),
        };
        let ctx = RecoveryContext {
            config: &config,
            message: &signal.message,
        };
        let outcome = tree.resolve(&signal, &ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryDirective::CompressContext {
                target_tokens,
            }) => {
                assert_eq!(target_tokens, 8_000);
            }
            other => panic!("expected Resolved(CompressContext), got {:?}", other),
        }
    }

    // ------------------------------------------------------------------
    // CredentialReload
    // ------------------------------------------------------------------

    #[test]
    fn test_credential_reload_offline_capable() {
        let resolver = CredentialReload;
        assert!(resolver.offline_capable());
        assert_eq!(resolver.name(), "CredentialReload");
    }

    #[test]
    fn test_credential_reload_non_auth_escalates() {
        let resolver = CredentialReload;
        let config = make_config("https://api.example.com/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "connection refused",
        };
        match resolver.resolve(&ctx) {
            ResolutionOutcome::Escalate => {}
            other => panic!("expected Escalate, got {:?}", other),
        }
    }

    /// Test the credential-availability helper deterministically without
    /// relying on global env state.  We check that when the env var is set
    /// the helper returns Some, and when unset it falls back to keyring
    /// (which may or may not have a key — either way it must not panic).
    #[test]
    fn test_credential_reload_find_available_key_with_env() {
        // Save and restore the env var to avoid cross-test flakiness.
        let saved = std::env::var("SELFWARE_API_KEY").ok();
        // Set a unique value that won't collide with keyring entries.
        std::env::set_var(
            "SELFWARE_API_KEY",
            "test-credential-reload-finder-key-xyz",
        );
        let key = CredentialReload::find_available_key();
        assert!(
            key.is_some(),
            "find_available_key should return Some when SELFWARE_API_KEY is set"
        );
        assert_eq!(
            key.unwrap(),
            "test-credential-reload-finder-key-xyz"
        );
        // Restore.
        match saved {
            Some(v) => std::env::set_var("SELFWARE_API_KEY", v),
            None => std::env::remove_var("SELFWARE_API_KEY"),
        }
    }

    /// When no env var is set, the helper falls through to the keyring.
    /// It should return None (or Some if a real keyring entry exists) but
    /// never panic.  We just assert it completes.
    #[test]
    fn test_credential_reload_find_available_key_without_env() {
        let saved = std::env::var("SELFWARE_API_KEY").ok();
        std::env::remove_var("SELFWARE_API_KEY");
        // This call may hit the OS keyring — that's fine, it must not panic.
        let _ = CredentialReload::find_available_key();
        match saved {
            Some(v) => std::env::set_var("SELFWARE_API_KEY", v),
            None => {}
        }
    }

    /// When a key source is present, the resolver produces ReloadCredentials.
    #[test]
    fn test_credential_reload_resolves_when_key_present() {
        let saved = std::env::var("SELFWARE_API_KEY").ok();
        std::env::set_var(
            "SELFWARE_API_KEY",
            "test-credential-reload-resolves-key-abc",
        );
        let resolver = CredentialReload;
        let config = make_config("https://api.example.com/v1");
        let ctx = RecoveryContext {
            config: &config,
            message: "401 Unauthorized",
        };
        let outcome = resolver.resolve(&ctx);
        match outcome {
            ResolutionOutcome::Resolved(RecoveryDirective::ReloadCredentials) => {}
            other => panic!("expected Resolved(ReloadCredentials), got {:?}", other),
        }
        match saved {
            Some(v) => std::env::set_var("SELFWARE_API_KEY", v),
            None => std::env::remove_var("SELFWARE_API_KEY"),
        }
    }
}
