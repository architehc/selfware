//! Model profile and RedactedString types.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{default_context_length, default_max_tokens, default_temperature};

/// A string wrapper that prevents accidental logging of secrets.
///
/// `Display` and `Debug` both emit `[REDACTED]`.  To access the
/// underlying value, call [`expose()`](RedactedString::expose).
///
/// Serializes / deserializes transparently as a plain string so that
/// existing TOML config files continue to work unchanged.
#[derive(Clone)]
pub struct RedactedString(String);

impl RedactedString {
    /// Create a new `RedactedString` wrapping the given secret.
    pub fn new(secret: impl Into<String>) -> Self {
        Self(secret.into())
    }

    /// Return a reference to the underlying secret.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl std::fmt::Debug for RedactedString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[REDACTED]")
    }
}

impl PartialEq for RedactedString {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for RedactedString {}

impl PartialEq<str> for RedactedString {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl Serialize for RedactedString {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RedactedString {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(RedactedString(s))
    }
}

impl From<String> for RedactedString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for RedactedString {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Marker substituted for secrets in serialized views meant for untrusted
/// consumers (e.g. the MCP `selfware://config` resource).
pub const REDACTED_SECRET_MARKER: &str = "<redacted>";

/// Redact secrets from a JSON view of the configuration, in place.
///
/// [`RedactedString`]'s `Serialize` impl intentionally emits the real value
/// so config round-trips (TOML save/load) keep working — which means a
/// plain `serde_json::to_string(&config)` leaks the raw API key to whatever
/// consumes it. Views handed to third parties must pass through this
/// instead:
///
/// - every string value under an `api_key` key (the top-level
///   `Config.api_key` and each `[models.*]` profile key) is replaced with
///   [`REDACTED_SECRET_MARKER`];
/// - every string value inside an `env` object (per-MCP-server environment
///   maps, e.g. `GITHUB_TOKEN = "…"`) is replaced as well.
///
/// Both recurse through arbitrary sub-objects/arrays, so secrets nested at
/// any depth are covered. Deserialization is unaffected — this is a
/// one-way view transformation.
pub fn redact_config_secrets(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, val) in map.iter_mut() {
                if key == "api_key" {
                    if val.is_string() {
                        *val = serde_json::Value::String(REDACTED_SECRET_MARKER.to_string());
                    }
                    continue;
                }
                if key == "env" {
                    if let serde_json::Value::Object(env_map) = val {
                        for env_val in env_map.values_mut() {
                            if env_val.is_string() {
                                *env_val =
                                    serde_json::Value::String(REDACTED_SECRET_MARKER.to_string());
                            }
                        }
                    }
                    continue;
                }
                redact_config_secrets(val);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                redact_config_secrets(item);
            }
        }
        _ => {}
    }
}

/// A named model profile, allowing multiple LLM backends (e.g. a text coder
/// and a vision critic) to coexist in a single selfware config.
///
/// Profiles are defined under `[models.<name>]` in selfware.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelProfile {
    /// API endpoint (e.g. `"http://192.168.1.170:1234/v1"`)
    pub endpoint: String,
    /// Model identifier
    pub model: String,
    /// Optional API key for this specific model
    pub api_key: Option<RedactedString>,
    /// Max response tokens
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Sampling temperature
    #[serde(default = "default_temperature")]
    pub temperature: f32,
    /// Supported modalities: `["text"]` or `["text", "vision"]`
    #[serde(default = "default_modalities")]
    pub modalities: Vec<String>,
    /// Context window length in tokens
    #[serde(default = "default_context_length")]
    pub context_length: usize,
    /// Extra fields merged into every chat-completion request body.
    #[serde(default)]
    pub extra_body: Option<serde_json::Map<String, serde_json::Value>>,
    /// Whether this profile supports native (OpenAI-style) tool calling.
    ///
    /// `None` (default) means "fall back to the parent
    /// `agent.native_function_calling`", preserving prior behaviour for
    /// existing TOML configs. `Some(true)` / `Some(false)` overrides for
    /// that profile only.
    ///
    /// This was previously hard-coded to `false` inside
    /// `chat_with_profile`, which silently disabled native FC for any
    /// swarm code that routed through a `ModelProfile`. See
    /// `src/api/tool_calling.rs`.
    #[serde(default)]
    pub native_function_calling: Option<bool>,
    /// Per-profile retry-count override for failed requests. `None` falls
    /// back to the parent's `retry.max_retries`. Hosted-slow and
    /// local-flaky endpoints want different retry shapes.
    #[serde(default)]
    pub max_retries: Option<u32>,
    /// Per-profile response-timeout floor in seconds. The response wait is
    /// adaptive (measured endpoint speed), but before the first measurement
    /// lands the default TPS assumption can be badly wrong for unusual
    /// backends (e.g. hybrid CPU/GPU ktransformers); the floor protects the
    /// cold-start window. `None` uses the global floor.
    #[serde(default)]
    pub response_timeout_floor_secs: Option<u64>,
}

impl ModelProfile {
    /// Returns `true` if this model profile lists `"vision"` among its modalities.
    pub fn supports_vision(&self) -> bool {
        self.modalities.iter().any(|m| m == "vision")
    }

    /// Resolve the effective native FC flag for this profile.
    ///
    /// If the profile sets [`Self::native_function_calling`] explicitly,
    /// that wins. Otherwise we fall back to the supplied default
    /// (typically the parent `Config.agent.native_function_calling`).
    pub fn effective_native_function_calling(&self, default_native: bool) -> bool {
        self.native_function_calling.unwrap_or(default_native)
    }

    /// Resolve the effective retry count for this profile.
    pub fn effective_max_retries(&self, default_max_retries: u32) -> u32 {
        self.max_retries.unwrap_or(default_max_retries)
    }
}

pub fn default_modalities() -> Vec<String> {
    vec!["text".to_string()]
}

#[cfg(test)]
#[path = "../../tests/unit/config/model/model_test.rs"]
mod tests;
