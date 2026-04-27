//! Hardware-aware quant recommendation.
//!
//! Inspects free VRAM (via `nvidia-smi`) + free RAM (`/proc/meminfo`) and
//! prints which Qwen3.6-27B HauhauCS quants will fit comfortably + which
//! the user should download. Run via:
//!
//! ```bash
//! cargo run --release --example quant_recommend
//! ```
//!
//! Designed as a stepping stone — eventually selfware should expose this
//! through `selfware autoconfig --quant-recommend` and offer to invoke
//! `huggingface-cli download` for the chosen quant directly.

use std::fs;
use std::process::Command;

#[derive(Debug)]
struct QuantInfo {
    name: &'static str,
    size_gb: f64,
    /// What an offline GGUF inference engine (llama.cpp `-ngl 99`) needs in
    /// VRAM to run this quant comfortably. Heuristic: model size ×1.15 (KV
    /// cache + activations + headroom).
    min_vram_gb: f64,
    bits: &'static str,
    quality_tier: QualityTier,
    notes: &'static str,
}

#[derive(Debug, Clone, Copy)]
enum QualityTier {
    Risky,
    Acceptable,
    Recommended,
    Premium,
}

impl QualityTier {
    fn label(self) -> &'static str {
        match self {
            QualityTier::Risky => "⚠️  risky",
            QualityTier::Acceptable => "ok",
            QualityTier::Recommended => "✓ recommended",
            QualityTier::Premium => "🌟 premium",
        }
    }
}

const QUANTS: &[QuantInfo] = &[
    QuantInfo {
        name: "IQ2_M",
        size_gb: 10.0,
        min_vram_gb: 12.0,
        bits: "2",
        quality_tier: QualityTier::Risky,
        notes: "Smallest. Tool-call agent loop is unreliable in our tests — fabricates 'task complete' without editing files.",
    },
    QuantInfo {
        name: "IQ3_XS",
        size_gb: 12.0,
        min_vram_gb: 14.0,
        bits: "3",
        quality_tier: QualityTier::Acceptable,
        notes: "3-bit, smallest. Borderline for agent work.",
    },
    QuantInfo {
        name: "IQ3_M",
        size_gb: 12.6,
        min_vram_gb: 15.0,
        bits: "3",
        quality_tier: QualityTier::Acceptable,
        notes: "3-bit, mid-quality. Better than IQ3_XS for similar memory cost.",
    },
    QuantInfo {
        name: "IQ4_XS",
        size_gb: 15.1,
        min_vram_gb: 18.0,
        bits: "4",
        quality_tier: QualityTier::Recommended,
        notes: "Smallest 4-bit. Good speed/quality balance for single 24GB GPU.",
    },
    QuantInfo {
        name: "Q4_K_P",
        size_gb: 17.5,
        min_vram_gb: 20.0,
        bits: "4",
        quality_tier: QualityTier::Recommended,
        notes: "Mid-tier 4-bit (HauhauCS K_P). Fixes coding scenarios reliably in our tests at ~40 tok/s on 24GB.",
    },
    QuantInfo {
        name: "Q5_K_P",
        size_gb: 20.8,
        min_vram_gb: 24.0,
        bits: "5",
        quality_tier: QualityTier::Recommended,
        notes: "5-bit. Slight quality bump over 4-bit; tighter VRAM fit.",
    },
    QuantInfo {
        name: "Q6_K_P",
        size_gb: 23.2,
        min_vram_gb: 27.0,
        bits: "6",
        quality_tier: QualityTier::Premium,
        notes: "6-bit. Needs >24GB VRAM or split across 2 GPUs.",
    },
    QuantInfo {
        name: "Q8_K_P",
        size_gb: 32.0,
        min_vram_gb: 36.0,
        bits: "8",
        quality_tier: QualityTier::Premium,
        notes: "Top quality. Requires 2× 24GB GPUs or one 48GB+ card.",
    },
];

fn detect_total_vram_gb() -> Option<f64> {
    let out = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mib_total: f64 = text
        .lines()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .sum();
    Some(mib_total / 1024.0)
}

fn detect_free_vram_gb() -> Option<f64> {
    let out = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.free", "--format=csv,noheader,nounits"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mib_total: f64 = text
        .lines()
        .filter_map(|l| l.trim().parse::<f64>().ok())
        .sum();
    Some(mib_total / 1024.0)
}

fn detect_total_ram_gb() -> Option<f64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let kib: f64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib / (1024.0 * 1024.0));
        }
    }
    None
}

fn detect_free_ram_gb() -> Option<f64> {
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kib: f64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib / (1024.0 * 1024.0));
        }
    }
    None
}

fn fits_vram(quant: &QuantInfo, vram_gb: f64) -> bool {
    quant.min_vram_gb <= vram_gb
}

fn fits_ram(quant: &QuantInfo, ram_gb: f64) -> bool {
    // CPU-only fallback: model needs ~size_gb of RAM plus working set.
    quant.size_gb * 1.2 <= ram_gb
}

fn main() {
    println!("Selfware quant recommender (Qwen3.6-27B HauhauCS Aggressive)\n");

    let total_vram = detect_total_vram_gb();
    let free_vram = detect_free_vram_gb();
    let total_ram = detect_total_ram_gb();
    let free_ram = detect_free_ram_gb();

    println!("Detected hardware:");
    match (total_vram, free_vram) {
        (Some(t), Some(f)) => {
            println!("  GPU: {t:.1} GB total / {f:.1} GB free (across all CUDA devices)")
        }
        _ => println!("  GPU: not detected (no nvidia-smi)"),
    }
    match (total_ram, free_ram) {
        (Some(t), Some(f)) => println!("  RAM: {t:.1} GB total / {f:.1} GB available"),
        _ => println!("  RAM: not detected"),
    }
    println!();

    let vram_budget = free_vram.or(total_vram).unwrap_or(0.0);
    let ram_budget = free_ram.or(total_ram).unwrap_or(0.0);

    // GPU recommendation
    println!("GPU-resident options (running with `llama-server -ngl 99`):");
    println!(
        "  {:<10}  {:<6}  {:<7}  {:<14}  {:<14}  Notes",
        "Quant", "Bits", "Size", "Status", "Tier"
    );
    let mut best_gpu: Option<&QuantInfo> = None;
    for q in QUANTS {
        let fits = fits_vram(q, vram_budget);
        let status = if fits {
            format!("✓ fits ({:.1} GB free)", vram_budget)
        } else {
            format!("✗ needs {:.0}+ GB", q.min_vram_gb)
        };
        // Trim notes to keep one row readable.
        let short_note: String = q.notes.chars().take(70).collect();
        println!(
            "  {:<10}  {:<6}  {:<5.1}GB  {:<14}  {:<14}  {}",
            q.name,
            q.bits,
            q.size_gb,
            status,
            q.quality_tier.label(),
            short_note
        );
        if fits {
            // Track the best quality tier that fits.
            best_gpu = Some(match (best_gpu, q.quality_tier) {
                (None, _) => q,
                (Some(_), QualityTier::Premium) => q,
                (Some(prev), QualityTier::Recommended)
                    if !matches!(prev.quality_tier, QualityTier::Premium) =>
                {
                    q
                }
                (Some(prev), QualityTier::Acceptable)
                    if matches!(prev.quality_tier, QualityTier::Risky) =>
                {
                    q
                }
                _ => best_gpu.unwrap(),
            });
        }
    }

    println!();
    if let Some(q) = best_gpu {
        println!(
            "Recommendation: download `{}` (~{:.1} GB).\n",
            q.name, q.size_gb
        );
        let url = format!(
            "https://huggingface.co/HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive/resolve/main/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-{}.gguf",
            q.name
        );
        println!("To download:");
        println!(
            "  huggingface-cli download HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive \\"
        );
        println!(
            "    Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-{}.gguf \\",
            q.name
        );
        println!("    mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf \\");
        println!("    --local-dir ~/models/qwen36-quants/");
        println!();
        println!("Or direct: {url}");
        println!();
        println!("To run after download:");
        println!(
            "  llama-server -m ~/models/qwen36-quants/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-{}.gguf \\",
            q.name
        );
        println!(
            "    --mmproj ~/models/qwen36-quants/mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf \\"
        );
        println!("    --jinja -c 65536 -ngl 99 --host 0.0.0.0 --port 8000");
        println!();
        println!("Then point selfware at it (selfware.toml or env):");
        println!("  endpoint = \"http://127.0.0.1:8000/v1\"");
        println!("  model = \"qwen3.6-27b\"");
    } else if ram_budget > 12.0 {
        println!("No quants fit your GPU; fall back to CPU-only inference (slower):");
        let mut best_cpu: Option<&QuantInfo> = None;
        for q in QUANTS.iter().rev() {
            if fits_ram(q, ram_budget) {
                best_cpu = Some(q);
                break;
            }
        }
        if let Some(q) = best_cpu {
            println!(
                "  recommended CPU quant: `{}` ({:.1} GB on disk, needs ~{:.1} GB RAM)",
                q.name,
                q.size_gb,
                q.size_gb * 1.2
            );
        } else {
            println!(
                "  no quant fits — even IQ2_M needs ~{:.1} GB available.",
                QUANTS[0].size_gb * 1.2
            );
        }
    } else {
        println!("Insufficient hardware for local inference. Consider:");
        println!("  • A cloud endpoint (vLLM, OpenRouter, etc.)");
        println!("  • Renting a GPU box (RunPod, Vast, etc.)");
    }
}
