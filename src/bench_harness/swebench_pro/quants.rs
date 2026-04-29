//! Per-quant scheduling policies and memory estimation for the 2x4090 rig.

use super::catalog::{BackendProfile, QuantSpec, ThinkingPolicy};

/// Total available VRAM for a 2x4090 rig in MB (≈48 GB).
pub const VRAM_2X4090_MB: u64 = 2 * 24_000;

/// Rough parameter count for Qwen3.6 27B.
const PARAMS_27B: u64 = 27_000_000_000;

/// Layer count for Qwen3.6 27B.
const LAYER_COUNT: u64 = 64;

/// Head dimension for Qwen3.6 27B.
const HEAD_DIM: u64 = 128;

/// Estimate per-quant memory footprint.
///
/// Returns `(model_memory_mb, kv_memory_mb)`.
///
/// Model size is derived from the quant label (bits-per-param heuristic).
/// KV size follows the rough formula:
/// `ctx * parallel * layer_count * head_dim * 2 * kv_bytes_per_element / (1024*1024)`.
pub fn estimate_memory_mb(spec: &QuantSpec) -> (u64, u64) {
    let model_mb = estimate_model_mb(spec);
    let kv_mb = estimate_kv_mb(spec);
    (model_mb, kv_mb)
}

fn estimate_model_mb(spec: &QuantSpec) -> u64 {
    let bits = quant_bits_per_param(&spec.label);
    // params * bits / 8 = bytes, then / 1024 / 1024 = MB
    let bytes = PARAMS_27B.saturating_mul(bits) / 8;
    bytes / 1024 / 1024
}

fn estimate_kv_mb(spec: &QuantSpec) -> u64 {
    let kv_bytes = kv_bytes_per_element(&spec.kv_cache_type);
    let total_bytes =
        spec.ctx as u64 * spec.max_parallel as u64 * LAYER_COUNT * HEAD_DIM * 2 * kv_bytes;
    total_bytes / 1024 / 1024
}

fn quant_bits_per_param(label: &str) -> u64 {
    let lower = label.to_lowercase();
    if lower.contains("iq2") || lower.contains("q2_") {
        2
    } else if lower.contains("iq3") || lower.contains("q3_") {
        3
    } else if lower.contains("iq4") || lower.contains("q4_") {
        4
    } else if lower.contains("q5_") {
        5
    } else if lower.contains("q6_") {
        6
    } else if lower.contains("q8_") || lower.contains("fp8") {
        8
    } else {
        4
    }
}

fn kv_bytes_per_element(kv_type: &str) -> u64 {
    match kv_type {
        "q4_0" => 1, // conservative: treat as 1 byte
        "q8_0" => 1,
        "f16" => 2,
        _ => 1,
    }
}

/// Fail-fast safety check: total estimated memory must fit in 2x4090 VRAM.
pub fn validate_safety(spec: &QuantSpec) -> Result<(), String> {
    let (model_mb, kv_mb) = estimate_memory_mb(spec);
    let total_mb = model_mb + kv_mb;
    if total_mb > VRAM_2X4090_MB {
        return Err(format!(
            "unsafe config: {} requires ~{} MB (model {} + KV {}) but 2x4090 only has {} MB",
            spec.label, total_mb, model_mb, kv_mb, VRAM_2X4090_MB
        ));
    }
    Ok(())
}

/// Predefined specs for the 2x4090 rig.
pub fn policies_2x4090() -> Vec<QuantSpec> {
    vec![
        QuantSpec {
            label: "Qwen3.6-27B-Q4_K_M".into(),
            gguf: "Qwen3.6-27B-Q4_K_M.gguf".into(),
            alias: "qwen3.6-27b-q4km".into(),
            mmproj: "mmproj-Qwen3.6-27B-f16.gguf".into(),
            name: "Qwen3.6 27B Q4_K_M".into(),
            ctx: 131_072,
            max_parallel: 4,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-Q5_K_M".into(),
            gguf: "Qwen3.6-27B-Q5_K_M.gguf".into(),
            alias: "qwen3.6-27b-q5km".into(),
            mmproj: "mmproj-Qwen3.6-27B-f16.gguf".into(),
            name: "Qwen3.6 27B Q5_K_M".into(),
            ctx: 131_072,
            max_parallel: 3,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-Q6_K".into(),
            gguf: "Qwen3.6-27B-Q6_K.gguf".into(),
            alias: "qwen3.6-27b-q6k".into(),
            mmproj: "mmproj-Qwen3.6-27B-f16.gguf".into(),
            name: "Qwen3.6 27B Q6_K".into(),
            ctx: 65_536,
            max_parallel: 2,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-Q8_0".into(),
            gguf: "Qwen3.6-27B-Q8_0.gguf".into(),
            alias: "qwen3.6-27b-q8".into(),
            mmproj: "mmproj-Qwen3.6-27B-f16.gguf".into(),
            name: "Qwen3.6 27B Q8_0".into(),
            ctx: 65_536,
            max_parallel: 1,
            kv_cache_type: "q8_0".into(),
            tensor_split: Some("24,24".into()),
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::LlamaCpp,
        },
        QuantSpec {
            label: "Qwen3.6-27B-FP8".into(),
            gguf: "Qwen3.6-27B-FP8.gguf".into(),
            alias: "qwen3.6-27b-fp8".into(),
            mmproj: "mmproj-Qwen3.6-27B-f16.gguf".into(),
            name: "Qwen3.6 27B FP8".into(),
            ctx: 65_536,
            max_parallel: 1,
            kv_cache_type: "f16".into(),
            tensor_split: Some("24,24".into()),
            temperature: 0.7,
            thinking_policy: ThinkingPolicy::Enable,
            backend: BackendProfile::VLLM,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_estimation_q4_k_m() {
        let policies = policies_2x4090();
        let spec = policies
            .iter()
            .find(|s| s.label == "Qwen3.6-27B-Q4_K_M")
            .unwrap();
        let (model_mb, kv_mb) = estimate_memory_mb(spec);
        // 27B params @ 4 bits = ~13.5 GB = ~13_824 MB
        assert!(
            model_mb > 12_000 && model_mb < 16_000,
            "model_mb = {}",
            model_mb
        );
        // ctx=131072, parallel=4, layers=64, head_dim=128, 2, 1 byte = ~8 GB = 8_192 MiB
        assert!(kv_mb > 7_000 && kv_mb < 9_000, "kv_mb = {}", kv_mb);
    }

    #[test]
    fn memory_estimation_fp8() {
        let policies = policies_2x4090();
        let spec = policies
            .iter()
            .find(|s| s.label == "Qwen3.6-27B-FP8")
            .unwrap();
        let (model_mb, kv_mb) = estimate_memory_mb(spec);
        // 27B params @ 8 bits = ~27 GB = ~27_648 MB
        assert!(
            model_mb > 25_000 && model_mb < 30_000,
            "model_mb = {}",
            model_mb
        );
        // ctx=65536, parallel=1, layers=64, head_dim=128, 2, 2 bytes = ~2 GB = ~2_048 MB
        assert!(kv_mb > 1_000 && kv_mb < 3_000, "kv_mb = {}", kv_mb);
    }

    #[test]
    fn safety_check_passes_for_all_2x4090_policies() {
        for spec in policies_2x4090() {
            validate_safety(&spec).unwrap_or_else(|e| {
                panic!("safety check failed for {}: {}", spec.label, e);
            });
        }
    }

    #[test]
    fn safety_check_fails_for_oversized_config() {
        let mut spec = policies_2x4090()
            .into_iter()
            .find(|s| s.label == "Qwen3.6-27B-FP8")
            .unwrap();
        // Blow up ctx and parallel so it no longer fits
        spec.ctx = 1_000_000;
        spec.max_parallel = 16;
        assert!(validate_safety(&spec).is_err());
    }

    #[test]
    fn policies_have_distinct_aliases() {
        let mut seen = std::collections::HashSet::new();
        for spec in policies_2x4090() {
            assert!(
                seen.insert(spec.alias.clone()),
                "duplicate alias: {}",
                spec.alias
            );
        }
    }

    #[test]
    fn quant_bits_per_param_heuristic() {
        assert_eq!(quant_bits_per_param("Q4_K_M"), 4);
        assert_eq!(quant_bits_per_param("Q5_K_M"), 5);
        assert_eq!(quant_bits_per_param("Q6_K"), 6);
        assert_eq!(quant_bits_per_param("Q8_0"), 8);
        assert_eq!(quant_bits_per_param("FP8"), 8);
        assert_eq!(quant_bits_per_param("IQ2_M"), 2);
        assert_eq!(quant_bits_per_param("IQ3_XS"), 3);
    }
}
