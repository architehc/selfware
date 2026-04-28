//! Quant catalog mirroring the Python `QUANT_CATALOG` dict.
//!
//! Keeping the entries in lock-step with `scripts/swebench_pro/run.py` so a user
//! invoking either path picks the same GGUF / alias / mmproj for a given label.

use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct QuantSpec {
    pub label: &'static str,
    /// GGUF filename (relative to ~/models/qwen36-quants).
    pub gguf: &'static str,
    /// llama-server `--alias`.
    pub alias: &'static str,
    /// mmproj filename (relative to ~/models/qwen36-quants).
    pub mmproj: &'static str,
}

const ENTRIES: &[QuantSpec] = &[
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-IQ2_M",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf",
        alias: "qwen3.6-27b-iq2m",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-IQ3_XS",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ3_XS.gguf",
        alias: "qwen3.6-27b-iq3xs",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-IQ3_M",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ3_M.gguf",
        alias: "qwen3.6-27b-iq3m",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-IQ4_XS",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ4_XS.gguf",
        alias: "qwen3.6-27b-iq4xs",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-Q2_K_P",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q2_K_P.gguf",
        alias: "qwen3.6-27b-q2kp",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-Q3_K_P",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q3_K_P.gguf",
        alias: "qwen3.6-27b-q3kp",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-Q4_K_P",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q4_K_P.gguf",
        alias: "qwen3.6-27b-q4kp",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-Q5_K_P",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q5_K_P.gguf",
        alias: "qwen3.6-27b-q5kp",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-Q6_K_P",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q6_K_P.gguf",
        alias: "qwen3.6-27b-q6kp",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    QuantSpec {
        label: "Qwen3.6-27B-HauhauCS-Q8_K_P",
        gguf: "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf",
        alias: "qwen3.6-27b-q8kp",
        mmproj: "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf",
    },
    // 35B-A3B baseline — note the relative paths point one dir up out of ~/models/qwen36-quants.
    QuantSpec {
        label: "Qwen3.6-35B-A3B-Q3_K_XL",
        gguf: "../Qwen3.6-35B-A3B-UD-Q3_K_XL.gguf",
        alias: "qwen3.6-35b-a3b",
        mmproj: "../mmproj-F16.gguf",
    },
];

pub fn quant_catalog() -> BTreeMap<&'static str, QuantSpec> {
    ENTRIES.iter().map(|q| (q.label, q.clone())).collect()
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
            assert!(cat.contains_key(q), "missing default quant: {}", q);
        }
    }

    #[test]
    fn catalog_entries_have_distinct_aliases() {
        let mut seen = std::collections::HashSet::new();
        for spec in quant_catalog().values() {
            assert!(seen.insert(spec.alias), "duplicate alias: {}", spec.alias);
        }
    }
}
