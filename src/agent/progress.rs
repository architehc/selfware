//! Structured progress events for live observability.
//!
//! This is the single source of truth for "what is the agent doing right now".
//! Both the TUI and the non-TUI / headless path consume the same event stream.
//!
//! Design:
//! - [`ProgressEvent`] is a lightweight, cloneable enum carrying the minimum
//!   information needed to drive a live status line / dashboard / log.
//! - [`ProgressEmitter`] is an `Arc<dyn …>` trait so multiple consumers can
//!   coexist (e.g. TUI + structured stderr log + future Prometheus exporter).
//! - Existing [`super::tui_events::AgentEvent`] / [`super::tui_events::EventEmitter`]
//!   continues to drive the TUI's rich rendering. Progress events are emitted
//!   alongside them — they do not replace the TUI channel.
//!
//! For non-TUI / headless mode (`selfware -p … --no-tui`), [`StderrProgressEmitter`]
//! writes one structured line per event to stderr so users can `tee` or `grep`
//! through long-running benches.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// A structured progress event emitted at well-defined points in the agent
/// execution loop. Cheap to clone.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// A new step is starting. `step` is 1-based.
    StepStarted {
        step: usize,
        model: String,
        tools_available: usize,
    },
    /// A request has been sent to the LLM.
    LlmRequestSent { tokens: usize },
    /// A response was received from the LLM.
    LlmResponseReceived {
        finish_reason: String,
        completion_tokens: u32,
    },
    /// A tool call has started.
    ToolCallStarted {
        tool: String,
        /// A short, human-readable summary of the arguments (e.g. `path=foo.py`).
        args_short: String,
    },
    /// A tool call has completed.
    ToolCallCompleted {
        tool: String,
        ok: bool,
        elapsed_ms: u64,
    },
    /// A guard fired (e.g. progress guard, no-action guard, loop guard).
    GuardFired {
        kind: String,
        /// How many times this guard kind has fired in the current run.
        count: usize,
    },
    /// A step has finished (one full iteration through the agent loop).
    StepCompleted {
        step: usize,
        mutating_tools_so_far: usize,
    },
    /// An external subprocess (cargo, pytest, etc.) was spawned.
    SubprocessStarted { name: String },
    /// An external subprocess exited.
    SubprocessCompleted {
        name: String,
        exit: i32,
        elapsed_ms: u64,
    },
    /// The task completed successfully.
    TaskCompleted { outcome: String },
    /// The task failed.
    TaskFailed { reason: String },
    /// A turn ended WITHOUT executing tools and WITHOUT completing — a refusal,
    /// a no-tool-call, an injected nudge, or an abort. Surfaced live so a
    /// refused/no-op turn is visible instead of a silent step gap.
    TurnDecision { decision: String, detail: String },
}

/// Build a short, log-safe args summary for a tool call. Pulls the most
/// useful field (path/command/pattern) and truncates aggressively so the
/// emitted line stays a single readable row.
pub fn short_args_for(tool_name: &str, args: &serde_json::Value) -> String {
    fn shorten(s: &str, max: usize) -> String {
        if s.chars().count() <= max {
            s.replace('\n', " ")
        } else {
            let mut out: String = s.chars().take(max).collect();
            out.push('…');
            out.replace('\n', " ")
        }
    }

    // Prefer common keys: path, file_path, command, cmd, pattern, query, url.
    let pull = |keys: &[&str]| -> Option<String> {
        for k in keys {
            if let Some(v) = args.get(*k).and_then(|v| v.as_str()) {
                return Some(format!("{}={}", k, shorten(v, 60)));
            }
        }
        None
    };

    if let Some(s) = pull(&["path", "file_path", "file"]) {
        return s;
    }
    if let Some(s) = pull(&["command", "cmd"]) {
        return s;
    }
    if let Some(s) = pull(&["pattern", "query", "search"]) {
        return s;
    }
    if let Some(s) = pull(&["url"]) {
        return s;
    }
    let _ = tool_name;
    String::new()
}

impl ProgressEvent {
    /// Short, stable name useful for log filtering.
    pub fn kind(&self) -> &'static str {
        match self {
            ProgressEvent::StepStarted { .. } => "step_started",
            ProgressEvent::LlmRequestSent { .. } => "llm_request_sent",
            ProgressEvent::LlmResponseReceived { .. } => "llm_response_received",
            ProgressEvent::ToolCallStarted { .. } => "tool_call_started",
            ProgressEvent::ToolCallCompleted { .. } => "tool_call_completed",
            ProgressEvent::GuardFired { .. } => "guard_fired",
            ProgressEvent::StepCompleted { .. } => "step_completed",
            ProgressEvent::SubprocessStarted { .. } => "subprocess_started",
            ProgressEvent::SubprocessCompleted { .. } => "subprocess_completed",
            ProgressEvent::TaskCompleted { .. } => "task_completed",
            ProgressEvent::TaskFailed { .. } => "task_failed",
            ProgressEvent::TurnDecision { .. } => "turn_decision",
        }
    }
}

/// Trait implemented by anything that consumes [`ProgressEvent`]s.
///
/// `Send + Sync` so a single `Arc<dyn ProgressEmitter>` can be shared across
/// async tasks. Implementations should be cheap on the hot path: blocking I/O
/// or heavy formatting belongs behind a channel/thread.
pub trait ProgressEmitter: Send + Sync {
    fn emit(&self, event: ProgressEvent);
}

/// No-op emitter — the default when no live observer is attached.
pub struct NoopProgressEmitter;

impl ProgressEmitter for NoopProgressEmitter {
    fn emit(&self, _event: ProgressEvent) {}
}

/// Forwards events to multiple inner emitters in order.
///
/// Used to attach a structured-log printer alongside the TUI bridge (or any
/// other observer the caller wires in).
pub struct MultiProgressEmitter {
    inner: Vec<Arc<dyn ProgressEmitter>>,
}

impl MultiProgressEmitter {
    pub fn new(inner: Vec<Arc<dyn ProgressEmitter>>) -> Self {
        Self { inner }
    }
}

impl ProgressEmitter for MultiProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        for em in &self.inner {
            em.emit(event.clone());
        }
    }
}

/// A simple stderr emitter that writes one structured line per event,
/// suitable for `--no-tui` / headless runs.
///
/// Output format: `[HH:MM:SS] kind=<kind> key1=value1 key2=value2 …`
pub struct StderrProgressEmitter {
    /// Wall-clock when this emitter was constructed; used for relative
    /// timestamps when the system clock is unavailable (deterministic tests).
    start: Instant,
    /// Whether to use the system clock for timestamps. Set to `false` in
    /// tests for stable output.
    use_system_clock: bool,
    /// Lock to keep multi-tool concurrent emissions atomic per line.
    lock: Mutex<()>,
}

impl Default for StderrProgressEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl StderrProgressEmitter {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
            use_system_clock: true,
            lock: Mutex::new(()),
        }
    }


    fn timestamp(&self) -> String {
        if self.use_system_clock {
            // Use chrono so we don't pull in extra deps.
            chrono::Local::now().format("%H:%M:%S").to_string()
        } else {
            let elapsed = self.start.elapsed();
            format!(
                "{:02}:{:02}:{:02}",
                elapsed.as_secs() / 3600,
                (elapsed.as_secs() / 60) % 60,
                elapsed.as_secs() % 60
            )
        }
    }
}

/// Render a [`ProgressEvent`] as a single structured-log line (without the
/// timestamp prefix). Pulled out so tests can call it directly.
pub fn render_event_kv(event: &ProgressEvent) -> String {
    match event {
        ProgressEvent::StepStarted {
            step,
            model,
            tools_available,
        } => format!(
            "kind=step_started step={} model={} tools_available={}",
            step, model, tools_available
        ),
        ProgressEvent::LlmRequestSent { tokens } => {
            format!("kind=llm_request_sent prompt_tokens={}", tokens)
        }
        ProgressEvent::LlmResponseReceived {
            finish_reason,
            completion_tokens,
        } => format!(
            "kind=llm_response_received finish_reason={} completion_tokens={}",
            finish_reason, completion_tokens
        ),
        ProgressEvent::ToolCallStarted { tool, args_short } => {
            if args_short.is_empty() {
                format!("kind=tool_call_started tool={}", tool)
            } else {
                format!("kind=tool_call_started tool={} args={}", tool, args_short)
            }
        }
        ProgressEvent::ToolCallCompleted {
            tool,
            ok,
            elapsed_ms,
        } => format!(
            "kind=tool_call_completed tool={} ok={} {}ms",
            tool, ok, elapsed_ms
        ),
        ProgressEvent::GuardFired { kind, count } => {
            format!("kind=guard_fired guard={} count={}", kind, count)
        }
        ProgressEvent::StepCompleted {
            step,
            mutating_tools_so_far,
        } => format!(
            "kind=step_completed step={} mutating_tools_so_far={}",
            step, mutating_tools_so_far
        ),
        ProgressEvent::SubprocessStarted { name } => {
            format!("kind=subprocess_started name={}", name)
        }
        ProgressEvent::SubprocessCompleted {
            name,
            exit,
            elapsed_ms,
        } => format!(
            "kind=subprocess_completed name={} exit={} {}ms",
            name, exit, elapsed_ms
        ),
        ProgressEvent::TaskCompleted { outcome } => {
            format!("kind=task_completed outcome={}", outcome)
        }
        ProgressEvent::TaskFailed { reason } => {
            // Reason can contain spaces; quote it.
            format!("kind=task_failed reason={:?}", reason)
        }
        ProgressEvent::TurnDecision { decision, detail } => {
            if detail.is_empty() {
                format!("kind=turn_decision decision={}", decision)
            } else {
                format!(
                    "kind=turn_decision decision={} detail=\"{}\"",
                    decision, detail
                )
            }
        }
    }
}

impl ProgressEmitter for StderrProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        // Suppress when the TUI owns the terminal — the TUI consumes events
        // through its own channel and rendering to stderr would corrupt the
        // alternate screen.
        if crate::output::is_tui_active() {
            return;
        }
        let line = format!("[{}] {}", self.timestamp(), render_event_kv(&event));
        // Best-effort: hold the global output lock so we don't interleave
        // with `cli_println!` chrome on stderr/stdout, then write the line.
        let _global = crate::output::OUTPUT_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let _local = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut stderr = std::io::stderr().lock();
        let _ = writeln!(stderr, "{}", line);
        let _ = stderr.flush();
    }
}

/// Test helper: an emitter that records events into a shared `Vec` so unit
/// tests can assert ordering.
#[cfg(test)]
#[derive(Default, Clone)]
pub struct RecordingProgressEmitter {
    pub events: Arc<Mutex<Vec<ProgressEvent>>>,
}

#[cfg(test)]
impl RecordingProgressEmitter {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn snapshot(&self) -> Vec<ProgressEvent> {
        self.events.lock().unwrap().clone()
    }

    pub fn kinds(&self) -> Vec<&'static str> {
        self.snapshot().iter().map(|e| e.kind()).collect()
    }
}

#[cfg(test)]
impl ProgressEmitter for RecordingProgressEmitter {
    fn emit(&self, event: ProgressEvent) {
        self.events.lock().unwrap().push(event);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/agent/progress/progress_test.rs"]
mod tests;
