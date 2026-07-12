//! Structured trace events for SWE-bench Pro diagnosis at scale.
//!
//! Each agent run produces a `trace.jsonl` (newline-delimited JSON) in the
//! trial directory.  The harness enriches it with PatchCaptured and
//! FailureClassified events, then a post-process `diagnose` pass turns the
//! traces into per-run `diagnosis.json` and a sweep-level
//! `diagnosis_summary.json`.

use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::agent::progress::{ProgressEmitter, ProgressEvent};

/// A single lightweight trace event.
///
/// Events are tagged by `#[serde(tag = "event")]` so each line in the JSONL
/// file is self-describing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "event")]
pub enum TraceEvent {
    ToolCallStarted {
        step: u32,
        tool: String,
        args: Value,
    },
    ToolCallCompleted {
        step: u32,
        tool: String,
        success: bool,
        duration_ms: u64,
    },
    LlmRequest {
        step: u32,
        estimated_tokens: usize,
    },
    LlmResponse {
        step: u32,
        content_chars: usize,
        tool_calls_count: usize,
    },
    PatchCaptured {
        patch_lines: usize,
        patch_bytes: usize,
    },
    VerificationStarted {
        step: u32,
        command: String,
    },
    VerificationCompleted {
        step: u32,
        success: bool,
    },
    FailureClassified {
        kind: String,
        evidence: String,
    },
    GuardFired {
        kind: String,
        count: u32,
    },
}

/// All trace events for a single (quant, instance, trial) run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTrace {
    pub run_id: String,
    pub instance_id: String,
    pub quant: String,
    pub trial: u32,
    pub events: Vec<TraceEvent>,
}

impl RunTrace {
    pub fn new(run_id: String, instance_id: String, quant: String, trial: u32) -> Self {
        Self {
            run_id,
            instance_id,
            quant,
            trial,
            events: Vec::new(),
        }
    }

    pub fn emit(&mut self, event: TraceEvent) {
        self.events.push(event);
    }

    /// Write events as newline-delimited JSON.
    pub fn write_jsonl(&self, path: &Path) -> Result<()> {
        let mut file = std::fs::File::create(path)?;
        for event in &self.events {
            let line = serde_json::to_string(event)?;
            writeln!(file, "{}", line)?;
        }
        Ok(())
    }

    /// Read events from a newline-delimited JSON file.
    /// Metadata fields are left empty; the caller should patch them.
    pub fn read_jsonl(path: &Path) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let mut events = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let event: TraceEvent = serde_json::from_str(&line)?;
            events.push(event);
        }
        Ok(Self {
            run_id: String::new(),
            instance_id: String::new(),
            quant: String::new(),
            trial: 0,
            events,
        })
    }
}

/// [`ProgressEmitter`] implementation that appends [`TraceEvent`]s directly
/// to a JSONL file.
///
/// This is used in the CLI headless path when `SELFWARE_RESULT_DIR` is set
/// so the harness can later enrich the trace with PatchCaptured and
/// FailureClassified events.
pub struct TraceProgressEmitter {
    file: Arc<Mutex<std::fs::File>>,
    current_step: AtomicUsize,
    pending_tool_calls: AtomicUsize,
    pending_llm_response: Mutex<Option<(usize, u32)>>,
}

impl TraceProgressEmitter {
    pub fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        Ok(Self {
            file: Arc::new(Mutex::new(file)),
            current_step: AtomicUsize::new(0),
            pending_tool_calls: AtomicUsize::new(0),
            pending_llm_response: Mutex::new(None),
        })
    }

    fn flush_llm_response(&self) {
        if let Some((step, tokens)) = self.pending_llm_response.lock().unwrap().take() {
            let tool_count = self.pending_tool_calls.swap(0, Ordering::SeqCst);
            let ev = TraceEvent::LlmResponse {
                step: step as u32,
                content_chars: tokens as usize * 4,
                tool_calls_count: tool_count,
            };
            let mut file = self.file.lock().unwrap();
            if let Ok(line) = serde_json::to_string(&ev) {
                let _ = writeln!(file, "{}", line);
            }
        }
    }
}

impl ProgressEmitter for TraceProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        let current_step = self.current_step.load(Ordering::SeqCst);
        let mut file = self.file.lock().unwrap();

        match &event {
            ProgressEvent::StepStarted { step, .. } => {
                // Flush any pending response from the previous step.
                drop(file);
                self.flush_llm_response();
                self.current_step.store(*step, Ordering::SeqCst);
            }
            ProgressEvent::LlmRequestSent { tokens } => {
                let ev = TraceEvent::LlmRequest {
                    step: current_step as u32,
                    estimated_tokens: *tokens,
                };
                if let Ok(line) = serde_json::to_string(&ev) {
                    let _ = writeln!(file, "{}", line);
                }
            }
            ProgressEvent::LlmResponseReceived {
                completion_tokens, ..
            } => {
                drop(file);
                *self.pending_llm_response.lock().unwrap() =
                    Some((current_step, *completion_tokens));
            }
            ProgressEvent::ToolCallStarted { tool, args_short } => {
                self.pending_tool_calls.fetch_add(1, Ordering::SeqCst);
                let ev = TraceEvent::ToolCallStarted {
                    step: current_step as u32,
                    tool: tool.clone(),
                    args: Value::String(args_short.clone()),
                };
                if let Ok(line) = serde_json::to_string(&ev) {
                    let _ = writeln!(file, "{}", line);
                }
            }
            ProgressEvent::ToolCallCompleted {
                tool,
                ok,
                elapsed_ms,
            } => {
                let ev = TraceEvent::ToolCallCompleted {
                    step: current_step as u32,
                    tool: tool.clone(),
                    success: *ok,
                    duration_ms: *elapsed_ms,
                };
                if let Ok(line) = serde_json::to_string(&ev) {
                    let _ = writeln!(file, "{}", line);
                }
            }
            ProgressEvent::GuardFired { kind, count } => {
                let ev = TraceEvent::GuardFired {
                    kind: kind.clone(),
                    count: *count as u32,
                };
                if let Ok(line) = serde_json::to_string(&ev) {
                    let _ = writeln!(file, "{}", line);
                }
            }
            ProgressEvent::SubprocessStarted { name } => {
                let ev = TraceEvent::VerificationStarted {
                    step: current_step as u32,
                    command: name.clone(),
                };
                if let Ok(line) = serde_json::to_string(&ev) {
                    let _ = writeln!(file, "{}", line);
                }
            }
            ProgressEvent::SubprocessCompleted { name, exit, .. } => {
                let ev = TraceEvent::VerificationCompleted {
                    step: current_step as u32,
                    success: *exit == 0,
                };
                if let Ok(line) = serde_json::to_string(&ev) {
                    let _ = writeln!(file, "{}", line);
                }
                // Also record the command name as a lightweight hint.
                let _ = name;
            }
            ProgressEvent::StepCompleted { .. }
            | ProgressEvent::TaskCompleted { .. }
            | ProgressEvent::TaskFailed { .. }
            | ProgressEvent::TurnDecision { .. } => {
                drop(file);
                self.flush_llm_response();
            }
        }
    }
}

/// Per-run diagnosis derived from a [`RunTrace`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PerRunDiagnosis {
    pub read_loop_count: u32,
    pub fake_complete: bool,
    pub timeout: bool,
    pub api_errors: u32,
    pub syntax_failures: u32,
    pub total_steps: u32,
    pub total_tool_calls: u32,
    pub failure_kind: Option<String>,
}

impl PerRunDiagnosis {
    pub fn from_trace(trace: &RunTrace) -> Self {
        let mut d = Self::default();
        for event in &trace.events {
            match event {
                TraceEvent::GuardFired { kind, count } => {
                    let k = kind.to_lowercase();
                    if k.contains("progress") || k.contains("read") || k.contains("loop") {
                        d.read_loop_count = d.read_loop_count.max(*count);
                    }
                }
                TraceEvent::FailureClassified { kind, .. } => {
                    d.failure_kind = Some(kind.clone());
                    let k = kind.to_lowercase();
                    if k.contains("fake") {
                        d.fake_complete = true;
                    }
                    if k.contains("timeout") {
                        d.timeout = true;
                    }
                    if k.contains("prefill") || k.contains("api") {
                        d.api_errors = d.api_errors.max(1);
                    }
                }
                TraceEvent::ToolCallCompleted { success, .. } => {
                    d.total_tool_calls += 1;
                    if !success {
                        d.syntax_failures += 1;
                    }
                }
                TraceEvent::LlmRequest { step, .. } => {
                    d.total_steps = d.total_steps.max(*step);
                }
                _ => {}
            }
        }
        d
    }
}

/// Sweep-level diagnosis summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiagnosisSummary {
    pub total_runs: usize,
    pub failure_mode_histogram: BTreeMap<String, u64>,
    pub most_common_failing_tools: Vec<(String, u64)>,
    pub median_turns_to_first_edit: f64,
    pub read_loop_rate: f64,
    pub fake_complete_rate: f64,
    pub timeout_rate: f64,
    pub api_error_rate: f64,
    pub syntax_failure_rate: f64,
}

impl DiagnosisSummary {
    pub fn from_diagnoses(diagnoses: &[(RunTrace, PerRunDiagnosis)]) -> Self {
        let mut summary = Self {
            total_runs: diagnoses.len(),
            ..Default::default()
        };
        if diagnoses.is_empty() {
            return summary;
        }

        let mut tool_failures: BTreeMap<String, u64> = BTreeMap::new();
        let mut turns_to_first_edit: Vec<f64> = Vec::new();
        let mut read_loops = 0u64;
        let mut fake_completes = 0u64;
        let mut timeouts = 0u64;
        let mut api_errors = 0u64;
        let mut syntax_failures = 0u64;

        for (trace, diag) in diagnoses {
            if let Some(ref kind) = diag.failure_kind {
                *summary
                    .failure_mode_histogram
                    .entry(kind.clone())
                    .or_default() += 1;
            }
            if diag.read_loop_count > 0 {
                read_loops += 1;
            }
            if diag.fake_complete {
                fake_completes += 1;
            }
            if diag.timeout {
                timeouts += 1;
            }
            if diag.api_errors > 0 {
                api_errors += 1;
            }
            if diag.syntax_failures > 0 {
                syntax_failures += 1;
            }

            // Turns to first edit: first ToolCallStarted for a mutating tool.
            let first_edit_step = trace.events.iter().find_map(|e| match e {
                TraceEvent::ToolCallStarted { step, tool, .. } => {
                    if is_mutating_tool(tool) {
                        Some(*step)
                    } else {
                        None
                    }
                }
                _ => None,
            });
            if let Some(step) = first_edit_step {
                turns_to_first_edit.push(step as f64);
            }

            // Count failing tools
            for event in &trace.events {
                if let TraceEvent::ToolCallCompleted {
                    tool,
                    success: false,
                    ..
                } = event
                {
                    *tool_failures.entry(tool.clone()).or_default() += 1;
                }
            }
        }

        let n = diagnoses.len() as f64;
        summary.read_loop_rate = read_loops as f64 / n;
        summary.fake_complete_rate = fake_completes as f64 / n;
        summary.timeout_rate = timeouts as f64 / n;
        summary.api_error_rate = api_errors as f64 / n;
        summary.syntax_failure_rate = syntax_failures as f64 / n;

        // Most common failing tools
        let mut tool_vec: Vec<(String, u64)> = tool_failures.into_iter().collect();
        tool_vec.sort_by_key(|b| std::cmp::Reverse(b.1));
        summary.most_common_failing_tools = tool_vec.into_iter().take(10).collect();

        // Median turns to first edit
        if !turns_to_first_edit.is_empty() {
            turns_to_first_edit
                .sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            summary.median_turns_to_first_edit = median_f64(&turns_to_first_edit);
        }

        summary
    }
}

fn is_mutating_tool(tool: &str) -> bool {
    matches!(
        tool,
        "file_write" | "file_edit" | "write_file" | "edit_file" | "shell" | "bash"
    ) || tool.contains("write")
        || tool.contains("edit")
}

fn median_f64(sorted: &[f64]) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    } else {
        sorted[n / 2]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trace_roundtrip() {
        let mut trace = RunTrace::new("r1".into(), "i1".into(), "q1".into(), 1);
        trace.emit(TraceEvent::LlmRequest {
            step: 1,
            estimated_tokens: 100,
        });
        trace.emit(TraceEvent::ToolCallStarted {
            step: 1,
            tool: "file_read".into(),
            args: json!("path=foo"),
        });
        trace.emit(TraceEvent::ToolCallCompleted {
            step: 1,
            tool: "file_read".into(),
            success: true,
            duration_ms: 12,
        });
        trace.emit(TraceEvent::PatchCaptured {
            patch_lines: 5,
            patch_bytes: 120,
        });

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("trace.jsonl");
        trace.write_jsonl(&path).unwrap();

        let loaded = RunTrace::read_jsonl(&path).unwrap();
        assert_eq!(loaded.events.len(), 4);
        assert_eq!(
            loaded.events[0],
            TraceEvent::LlmRequest {
                step: 1,
                estimated_tokens: 100
            }
        );
        assert_eq!(
            loaded.events[1],
            TraceEvent::ToolCallStarted {
                step: 1,
                tool: "file_read".into(),
                args: json!("path=foo")
            }
        );
    }

    #[test]
    fn diagnosis_histogram() {
        let mut trace1 = RunTrace::new("r1".into(), "i1".into(), "q1".into(), 1);
        trace1.emit(TraceEvent::FailureClassified {
            kind: "FakeComplete".into(),
            evidence: "ev".into(),
        });
        trace1.emit(TraceEvent::GuardFired {
            kind: "progress".into(),
            count: 1,
        });
        trace1.emit(TraceEvent::ToolCallStarted {
            step: 2,
            tool: "file_write".into(),
            args: json!(""),
        });
        trace1.emit(TraceEvent::ToolCallCompleted {
            step: 2,
            tool: "file_write".into(),
            success: true,
            duration_ms: 10,
        });

        let mut trace2 = RunTrace::new("r2".into(), "i2".into(), "q1".into(), 1);
        trace2.emit(TraceEvent::FailureClassified {
            kind: "Timeout".into(),
            evidence: "ev".into(),
        });
        trace2.emit(TraceEvent::ToolCallCompleted {
            step: 1,
            tool: "file_read".into(),
            success: false,
            duration_ms: 5,
        });

        let d1 = PerRunDiagnosis::from_trace(&trace1);
        let d2 = PerRunDiagnosis::from_trace(&trace2);

        assert!(d1.fake_complete);
        assert!(!d1.timeout);
        assert!(d2.timeout);
        assert_eq!(d1.total_tool_calls, 1);
        assert_eq!(d2.syntax_failures, 1);

        let summary = DiagnosisSummary::from_diagnoses(&[(trace1, d1), (trace2, d2)]);
        assert_eq!(summary.total_runs, 2);
        assert_eq!(
            summary.failure_mode_histogram.get("FakeComplete").copied(),
            Some(1)
        );
        assert_eq!(
            summary.failure_mode_histogram.get("Timeout").copied(),
            Some(1)
        );
        assert_eq!(summary.median_turns_to_first_edit, 2.0);
        assert!((summary.fake_complete_rate - 0.5).abs() < 1e-9);
        assert!((summary.timeout_rate - 0.5).abs() < 1e-9);
    }
}
