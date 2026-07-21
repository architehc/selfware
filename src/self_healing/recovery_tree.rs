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
    let auth_markers = [
        "401",
        "403",
        "unauthorized",
        "invalid api key",
        "authentication",
    ];
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
    let context_markers = [
        "context",
        "token limit",
        "maximum context",
        "too many tokens",
    ];
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
/// **OPT-IN ONLY.** Rerouting the agent to a different endpoint is a
/// high-surprise action: the old default resolved this on ANY "unreachable"
/// classification and the agent then silently ran against `localhost:11434`
/// for the rest of the session (whole-repo review, self-healing P2). The
/// resolver now stays inert (`Escalate`) unless the operator explicitly
/// opts in for the process/session via `SELFWARE_ENDPOINT_FALLBACK=1`
/// (or constructs it with [`LocalEndpointFallback::opt_in`]). When it does
/// fire, it logs a loud warning that the switch is session-scoped and how to
/// make it permanent — never a silent permanent reroute.
///
/// Resolution logic:
/// 1. If not opted in → `Escalate`.
/// 2. If `config.endpoint` is **already** local → `Escalate` (nothing to
///    fall back *to*).
/// 3. Otherwise prefer a local endpoint that may already be present in
///    config metadata.  Since the current `Config` struct only exposes a
///    single `endpoint`, we fall back to a well-known local default:
///    `http://localhost:11434/v1` (Ollama's OpenAI-compatible server).
/// 4. Return `Resolved(Fallback { target })`.
///
/// This resolver is fully offline: it only inspects config strings and returns
/// an action.  The agent loop (or a future increment) is responsible for
/// actually switching the endpoint and retrying.
pub struct LocalEndpointFallback {
    /// Whether the operator explicitly opted in to endpoint rerouting.
    enabled: bool,
}

/// Well-known local endpoint for Ollama's OpenAI-compatible API.
const OLLAMA_LOCAL_ENDPOINT: &str = "http://localhost:11434/v1";

/// Env var that opts the process into local-endpoint fallback rerouting.
const ENDPOINT_FALLBACK_ENV: &str = "SELFWARE_ENDPOINT_FALLBACK";

impl LocalEndpointFallback {
    /// A resolver that WILL emit fallback directives. Use only when the
    /// operator has explicitly asked for endpoint rerouting.
    pub fn opt_in() -> Self {
        Self { enabled: true }
    }

    /// A resolver gated on the `SELFWARE_ENDPOINT_FALLBACK` environment
    /// variable — this is what [`RecoveryTree::with_defaults`] registers, so
    /// the default auto-recovery path never reroutes endpoints silently.
    pub fn from_env() -> Self {
        Self {
            enabled: Self::env_opted_in(std::env::var(ENDPOINT_FALLBACK_ENV).ok().as_deref()),
        }
    }

    /// Pure opt-in check: truthy values are `1/true/yes/on` (case-insensitive).
    /// Kept separate so it is deterministically unit-testable without touching
    /// the process environment.
    fn env_opted_in(value: Option<&str>) -> bool {
        matches!(
            value.map(|v| v.trim().to_ascii_lowercase()),
            Some(ref v) if v == "1" || v == "true" || v == "yes" || v == "on"
        )
    }

    /// Determine the best local endpoint to fall back to.
    ///
    /// An explicit `SELFWARE_LOCAL_ENDPOINT` override wins, so operators running
    /// llama.cpp (:8080), LM Studio (:1234), or vLLM (:8000) instead of Ollama
    /// get a fallback that actually points at their server. Without an override
    /// we use the well-known Ollama default. (Reachability probing of multiple
    /// candidates is the async caller's responsibility — this resolver stays
    /// offline/synchronous.)
    fn pick_local_endpoint(_config: &crate::config::Config) -> String {
        Self::choose_local_endpoint(std::env::var("SELFWARE_LOCAL_ENDPOINT").ok().as_deref())
    }

    /// Pure decision used by `pick_local_endpoint`: a non-empty override string
    /// wins; otherwise fall back to the Ollama default. Kept separate so it is
    /// deterministically unit-testable without touching process environment.
    fn choose_local_endpoint(override_endpoint: Option<&str>) -> String {
        if let Some(ep) = override_endpoint {
            let ep = ep.trim();
            if !ep.is_empty() {
                return ep.to_string();
            }
        }
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

        // Rerouting the agent's endpoint is opt-in only — never silently
        // switch the session to a local server on a bare "unreachable".
        if !self.enabled {
            tracing::debug!(
                "LocalEndpointFallback: endpoint rerouting not enabled \
                 (set {}=1 to opt in) — escalating",
                ENDPOINT_FALLBACK_ENV
            );
            return ResolutionOutcome::Escalate;
        }

        let current = &ctx.config.endpoint;

        // If we're already on a local endpoint, there's nowhere to fall back.
        if crate::config::is_local_endpoint(current) {
            return ResolutionOutcome::Escalate;
        }

        // Choose a local endpoint and emit a Fallback directive.
        let target = Self::pick_local_endpoint(ctx.config);
        // Loud notice: the switch applies for the REST OF THIS SESSION (the
        // agent mutates its in-memory config) and is NOT persisted. Tell the
        // operator exactly what happened and how to make it permanent.
        tracing::warn!(
            "⚠️  Self-healing is switching the LLM endpoint from '{}' to '{}' \
             for the rest of this session because the configured endpoint was \
             classified as unreachable. This change is session-scoped and NOT \
             saved; update the `endpoint` in selfware.toml to make it \
             permanent, or unset {} to disable automatic fallback.",
            current,
            target,
            ENDPOINT_FALLBACK_ENV
        );
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
    pub fn find_available_key(endpoint: &str) -> Option<String> {
        if let Ok(key) = std::env::var("SELFWARE_API_KEY") {
            if !key.is_empty() {
                return Some(key);
            }
        }
        match crate::config::load_api_key_from_keyring(endpoint) {
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
        if Self::find_available_key(&ctx.config.endpoint).is_some() {
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
    /// - [`LocalEndpointFallback`] under [`FailureKind::LlmUnreachable`] —
    ///   gated on `SELFWARE_ENDPOINT_FALLBACK` so the default tree NEVER
    ///   reroutes the agent's endpoint unless the operator opted in.
    ///
    /// Resolvers are ordered offline-first.
    pub fn with_defaults() -> Self {
        let mut tree = Self::new();

        // LlmUnreachable → LocalEndpointFallback (offline, opt-in gated)
        tree.register(
            FailureKind::LlmUnreachable,
            Box::new(LocalEndpointFallback::from_env()),
        );

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
#[allow(clippy::field_reassign_with_default)] // test config builders: default-then-tweak is clearer
#[path = "../../tests/unit/self_healing/recovery_tree/recovery_tree_test.rs"]
mod tests;
