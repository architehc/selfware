//! SWE-bench Pro × selfware harness (Rust port of `scripts/swebench_pro/run.py`).
//!
//! Boots a `llama-server` per quant, drives `selfware -p PROMPT -C WORKDIR
//! --yolo --no-tui --quiet` for each instance, captures the resulting
//! `git diff` as a `.pred`, and writes per-instance `result.json`.
//!
//! Adds `--trials N` for repeated runs and writes an aggregate `aggregate.json`
//! plus a ready-to-eval `patches.json` collation.

pub mod catalog;
pub mod dataset;
pub mod harness;
pub mod runner;

pub use catalog::{quant_catalog, QuantSpec};
pub use dataset::{load_instances, Instance};
pub use harness::{LlamaServer, LlamaServerOpts};
pub use runner::{run_swebench_pro, SwebenchProOpts};
