//! Quant catalog mirroring the Python `QUANT_CATALOG` dict.
//!
//! Keeping the entries in lock-step with `scripts/swebench_pro/run.py` so a user
//! invoking either path picks the same GGUF / alias / mmproj for a given label.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BackendProfile {
    LlamaCpp,
    SGLang,
    VLLM,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ThinkingPolicy {
    Enable,
    Disable,
    Preserve,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QuantSpec {
    pub label: String,
    /// GGUF filename (relative to ~/models/qwen36-quants).
    pub gguf: String,
    /// llama-server `--alias`.
    pub alias: String,
    /// mmproj filename (relative to ~/models/qwen36-quants).
    pub mmproj: String,
    /// Human-readable name.
    pub name: String,
    /// Recommended context size for this quant.
    pub ctx: u32,
    /// Maximum parallel slots for this quant (safety ceiling).
    pub max_parallel: u32,
    /// KV cache quant type: `"q4_0"`, `"q8_0"`, `"f16"`.
    pub kv_cache_type: String,
    /// `--tensor-split` value (e.g. `"24,24"` for 2x4090).
    pub tensor_split: Option<String>,
    /// Sampling temperature.
    pub temperature: f32,
    /// Whether thinking tokens are emitted.
    pub thinking_policy: ThinkingPolicy,
    /// Which backend engine serves this quant.
    pub backend: BackendProfile,
}

static CATALOG: Lazy<BTreeMap<String, QuantSpec>> = Lazy::new(|| {
    vec![
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-IQ2_M".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf".into(),
            alias: "qwen3.6-27b-iq2m".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-IQ2_M".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-IQ3_XS".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ3_XS.gguf".into(),
            alias: "qwen3.6-27b-iq3xs".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-IQ3_XS".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-IQ3_M".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ3_M.gguf".into(),
            alias: "qwen3.6-27b-iq3m".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-IQ3_M".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-IQ4_XS".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ4_XS.gguf".into(),
            alias: "qwen3.6-27b-iq4xs".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-IQ4_XS".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-Q2_K_P".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q2_K_P.gguf".into(),
            alias: "qwen3.6-27b-q2kp".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-Q2_K_P".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-Q3_K_P".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q3_K_P.gguf".into(),
            alias: "qwen3.6-27b-q3kp".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-Q3_K_P".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-Q4_K_P".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf".into(),
            alias: "qwen3.6-27b-q4kp".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-Q4_K_P".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-Q5_K_P".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q5_K_P.gguf".into(),
            alias: "qwen3.6-27b-q5kp".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-Q5_K_P".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-Q6_K_P".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q6_K_P.gguf".into(),
            alias: "qwen3.6-27b-q6kp".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-Q6_K_P".into(),
            ctx: 262_144,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-HauhauCS-Q8_K_P".into(),
            gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf".into(),
            alias: "qwen3.6-27b-q8kp".into(),
            mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf".into(),
            name: "Qwen3.6-27B-HauhauCS-Q8_K_P".into(),
            ctx: 262_144,
            max_parallel: 1,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
        // 35B-A3B baseline — note the relative paths point one dir up out of ~/models/qwen36-quants.
        QuantSpec {
            label: "Qwen3.6-35B-A3B-Q3_K_XL".into(),
            gguf: "../Qwen3.6-35B-A3B-UD-Q3_K_XL.gguf".into(),
            alias: "qwen3.6-35b-a3b".into(),
            mmproj: "../mmproj-F16.gguf".into(),
            name: "Qwen3.6-35B-A3B-Q3_K_XL".into(),
            ctx: 262_144,
            max_parallel: 1,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 1.0,
            thinking_policy: ThinkingPolicy::Disable,
            backend: BackendProfile::LlamaCpp,
        },
    ]
    .into_iter()
    .map(|q| (q.label.clone(), q))
    .collect()
});

pub fn quant_catalog() -> BTreeMap<String, QuantSpec> {
    CATALOG.clone()
}

/// Default quant set used when `--quants` is not provided.
pub const DEFAULT_QUANTS: &[&str] = &[
    "Qwen3.6-35B-A3B-Q3_K_XL",
    "Qwen3.6-27B-HauhauCS-Q4_K_P",
    "Qwen3.6-27B-HauhauCS-IQ4_XS",
    "Qwen3.6-27B-HauhauCS-Q2_K_P",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_contains_default_quants() {
        let cat = quant_catalog();
        for q in DEFAULT_QUANTS {
            assert!(cat.contains_key(*q), "missing default quant: {}", q);
        }
    }

    #[test]
    fn catalog_entries_have_distinct_aliases() {
        let mut seen = std::collections::HashSet::new();
        for spec in quant_catalog().values() {
            assert!(
                seen.insert(spec.alias.clone()),
                "duplicate alias: {}",
                spec.alias
            );
        }
    }

    #[test]
    fn quant_spec_serializes_roundtrip() {
        let spec = QuantSpec {
            label: "test".into(),
            gguf: "test.gguf".into(),
            alias: "test-alias".into(),
            mmproj: "mmproj.gguf".into(),
            name: "Test".into(),
            ctx: 65536,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        };
        let json = serde_json::to_string(&spec).unwrap();
        let restored: QuantSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(spec.label, restored.label);
        assert_eq!(spec.backend, restored.backend);
        assert_eq!(spec.thinking_policy, restored.thinking_policy);
    }
}
