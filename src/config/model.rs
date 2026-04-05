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
}

impl ModelProfile {
    /// Returns `true` if this model profile lists `"vision"` among its modalities.
    pub fn supports_vision(&self) -> bool {
        self.modalities.iter().any(|m| m == "vision")
    }
}

pub fn default_modalities() -> Vec<String> {
    vec!["text".to_string()]
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // RedactedString tests
    // =========================================================================

    #[test]
    fn test_redacted_string_new() {
        let rs = RedactedString::new("my-secret-key");
        assert_eq!(rs.expose(), "my-secret-key");
    }

    #[test]
    fn test_redacted_string_display_hides_secret() {
        let rs = RedactedString::new("super-secret");
        assert_eq!(format!("{}", rs), "[REDACTED]");
        assert!(!format!("{}", rs).contains("super-secret"));
    }

    #[test]
    fn test_redacted_string_debug_hides_secret() {
        let rs = RedactedString::new("my-api-key");
        assert_eq!(format!("{:?}", rs), "[REDACTED]");
        assert!(!format!("{:?}", rs).contains("my-api-key"));
    }

    #[test]
    fn test_redacted_string_expose() {
        let rs = RedactedString::new("exposed-value");
        assert_eq!(rs.expose(), "exposed-value");
    }

    #[test]
    fn test_redacted_string_eq() {
        let a = RedactedString::new("same");
        let b = RedactedString::new("same");
        assert_eq!(a, b);
    }

    #[test]
    fn test_redacted_string_ne() {
        let a = RedactedString::new("one");
        let b = RedactedString::new("two");
        assert_ne!(a, b);
    }

    #[test]
    fn test_redacted_string_eq_str() {
        let rs = RedactedString::new("hello");
        assert!(rs == *"hello");
        assert!(!(rs == *"world"));
    }

    #[test]
    fn test_redacted_string_from_string() {
        let rs: RedactedString = "test-key".to_string().into();
        assert_eq!(rs.expose(), "test-key");
    }

    #[test]
    fn test_redacted_string_from_str() {
        let rs: RedactedString = "test-key".into();
        assert_eq!(rs.expose(), "test-key");
    }

    #[test]
    fn test_redacted_string_clone() {
        let rs = RedactedString::new("cloneable");
        let cloned = rs.clone();
        assert_eq!(rs, cloned);
        assert_eq!(cloned.expose(), "cloneable");
    }

    #[test]
    fn test_redacted_string_serialize() {
        let rs = RedactedString::new("serialized-value");
        let json = serde_json::to_string(&rs).unwrap();
        assert_eq!(json, "\"serialized-value\"");
    }

    #[test]
    fn test_redacted_string_deserialize() {
        let rs: RedactedString = serde_json::from_str("\"deserialized-value\"").unwrap();
        assert_eq!(rs.expose(), "deserialized-value");
    }

    #[test]
    fn test_redacted_string_serde_roundtrip() {
        let original = RedactedString::new("roundtrip-secret");
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RedactedString = serde_json::from_str(&json).unwrap();
        assert_eq!(original, deserialized);
    }

    #[test]
    fn test_redacted_string_empty() {
        let rs = RedactedString::new("");
        assert_eq!(rs.expose(), "");
        assert_eq!(format!("{}", rs), "[REDACTED]");
    }

    #[test]
    fn test_redacted_string_special_chars() {
        let rs = RedactedString::new("key=abc&token=xyz!@#$%");
        assert_eq!(rs.expose(), "key=abc&token=xyz!@#$%");
    }

    // =========================================================================
    // ModelProfile tests
    // =========================================================================

    #[test]
    fn test_model_profile_supports_vision_true() {
        let profile = ModelProfile {
            endpoint: "http://localhost:8080/v1".to_string(),
            model: "test-model".to_string(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.7,
            modalities: vec!["text".to_string(), "vision".to_string()],
            context_length: 32768,
            extra_body: None,
        };
        assert!(profile.supports_vision());
    }

    #[test]
    fn test_model_profile_supports_vision_false() {
        let profile = ModelProfile {
            endpoint: "http://localhost:8080/v1".to_string(),
            model: "test-model".to_string(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.7,
            modalities: vec!["text".to_string()],
            context_length: 32768,
            extra_body: None,
        };
        assert!(!profile.supports_vision());
    }

    #[test]
    fn test_model_profile_supports_vision_empty_modalities() {
        let profile = ModelProfile {
            endpoint: "http://localhost:8080/v1".to_string(),
            model: "test-model".to_string(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.7,
            modalities: vec![],
            context_length: 32768,
            extra_body: None,
        };
        assert!(!profile.supports_vision());
    }

    #[test]
    fn test_model_profile_serialization() {
        let profile = ModelProfile {
            endpoint: "http://localhost:8080/v1".to_string(),
            model: "qwen-72b".to_string(),
            api_key: Some(RedactedString::new("sk-test")),
            max_tokens: 8192,
            temperature: 0.5,
            modalities: vec!["text".to_string()],
            context_length: 131072,
            extra_body: None,
        };
        let json = serde_json::to_string(&profile).unwrap();
        assert!(json.contains("qwen-72b"));
        assert!(json.contains("131072"));
        // api_key should serialize as plain string (not redacted)
        assert!(json.contains("sk-test"));
    }

    #[test]
    fn test_model_profile_deserialization() {
        let json = r#"{
            "endpoint": "http://example.com/v1",
            "model": "test-model",
            "api_key": "my-key",
            "max_tokens": 2048,
            "temperature": 0.3,
            "modalities": ["text", "vision"],
            "context_length": 65536
        }"#;
        let profile: ModelProfile = serde_json::from_str(json).unwrap();
        assert_eq!(profile.endpoint, "http://example.com/v1");
        assert_eq!(profile.model, "test-model");
        assert_eq!(profile.api_key.as_ref().unwrap().expose(), "my-key");
        assert_eq!(profile.max_tokens, 2048);
        assert_eq!(profile.temperature, 0.3);
        assert!(profile.supports_vision());
        assert_eq!(profile.context_length, 65536);
    }

    #[test]
    fn test_model_profile_with_extra_body() {
        let mut extra = serde_json::Map::new();
        extra.insert("top_p".to_string(), serde_json::json!(0.95));
        extra.insert("repetition_penalty".to_string(), serde_json::json!(1.1));

        let profile = ModelProfile {
            endpoint: "http://localhost/v1".to_string(),
            model: "test".to_string(),
            api_key: None,
            max_tokens: 4096,
            temperature: 0.7,
            modalities: vec!["text".to_string()],
            context_length: 32768,
            extra_body: Some(extra),
        };
        let extra = profile.extra_body.as_ref().unwrap();
        assert_eq!(extra["top_p"], serde_json::json!(0.95));
    }

    #[test]
    fn test_default_modalities_returns_text() {
        let mods = default_modalities();
        assert_eq!(mods, vec!["text".to_string()]);
    }

    #[test]
    fn test_model_profile_clone() {
        let profile = ModelProfile {
            endpoint: "http://localhost/v1".to_string(),
            model: "model".to_string(),
            api_key: Some(RedactedString::new("key")),
            max_tokens: 1024,
            temperature: 0.5,
            modalities: vec!["text".to_string()],
            context_length: 8192,
            extra_body: None,
        };
        let cloned = profile.clone();
        assert_eq!(cloned.model, "model");
        assert_eq!(cloned.api_key.as_ref().unwrap().expose(), "key");
    }
}
