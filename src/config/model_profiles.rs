//! Built-in model defaults profiles.
//!
//! Different model families need different sampling parameters and request
//! shapes to perform well.  Hard-coding these in user configs is a footgun:
//! a fresh user pointing selfware at Qwen 3.6 with default settings sees
//! 0/10 SWE-bench Pro because the model wants `presence_penalty = 1.5`,
//! `top_p = 0.8`, `min_p = 0.0`, `enable_thinking = true`, etc.
//!
//! This module ships built-in [`ModelDefaultsProfile`] rules keyed by a
//! glob pattern on the model name.  At config-load time the loader looks up
//! the first matching profile and fills in any field the user did NOT set
//! explicitly.  Explicit user config always wins over a profile.
//!
//! These profiles are intentionally **static** — they are derived from
//! `config.model` only and never make network calls.  Live capability
//! detection lives in [`crate::config::auto_config`].
//!
//! Naming note: the existing [`crate::config::ModelProfile`] type is a
//! per-named-model TOML section ("coder", "vision", ...).  The struct here
//! is a *defaults rule* keyed by model-name glob — different concept,
//! different name.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// What one main-loop model call is for. Selects the call's per-turn quota
/// ([`WorkloadQuota`]): whether the model thinks, and its completion cap.
///
/// The agent classifies every main turn before sending it
/// (`agent::turn_workload`): the first call of a task is `Planning`; a call
/// that continues right after read-only tool results is `Mechanical`; after
/// a mutating tool (edit, write, shell, test) it is `Edit`; anything else —
/// a gate/correction directive, a user message, the final answer — is
/// `Synthesis`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnWorkload {
    Planning,
    Mechanical,
    Edit,
    Synthesis,
}

impl TurnWorkload {
    /// Every workload, in display order.
    pub const ALL: [TurnWorkload; 4] = [
        TurnWorkload::Planning,
        TurnWorkload::Mechanical,
        TurnWorkload::Edit,
        TurnWorkload::Synthesis,
    ];

    /// Stable name, also the `[workloads.<name>]` config key.
    pub fn as_str(self) -> &'static str {
        match self {
            TurnWorkload::Planning => "planning",
            TurnWorkload::Mechanical => "mechanical",
            TurnWorkload::Edit => "edit",
            TurnWorkload::Synthesis => "synthesis",
        }
    }
}

impl std::fmt::Display for TurnWorkload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-turn request quota for one [`TurnWorkload`]. `None` fields leave the
/// request as the session config builds it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkloadQuota {
    /// Sent as `chat_template_kwargs.enable_thinking` on this turn only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_thinking: Option<bool>,
    /// Completion cap (`max_tokens`) for this turn only; still clamped to
    /// what the context window leaves.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
}

impl WorkloadQuota {
    pub fn is_empty(&self) -> bool {
        self.enable_thinking.is_none() && self.max_tokens.is_none()
    }
}

/// One [`WorkloadQuota`] per [`TurnWorkload`] — the `[workloads]` config
/// section and a profile's per-workload table.
///
/// Precedence per field, most specific first: an explicit
/// `[workloads.<kind>]` value; then, for `enable_thinking`, a user
/// `extra_body` pin of `enable_thinking` (it holds for EVERY turn, so the
/// profile's per-turn toggle is not applied) and, for `max_tokens`, an
/// explicit top-level `max_tokens` (likewise); then the matched profile's
/// table; else unset.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct WorkloadQuotas {
    pub planning: WorkloadQuota,
    pub mechanical: WorkloadQuota,
    pub edit: WorkloadQuota,
    pub synthesis: WorkloadQuota,
}

impl WorkloadQuotas {
    pub fn get(&self, kind: TurnWorkload) -> WorkloadQuota {
        match kind {
            TurnWorkload::Planning => self.planning,
            TurnWorkload::Mechanical => self.mechanical,
            TurnWorkload::Edit => self.edit,
            TurnWorkload::Synthesis => self.synthesis,
        }
    }

    pub fn get_mut(&mut self, kind: TurnWorkload) -> &mut WorkloadQuota {
        match kind {
            TurnWorkload::Planning => &mut self.planning,
            TurnWorkload::Mechanical => &mut self.mechanical,
            TurnWorkload::Edit => &mut self.edit,
            TurnWorkload::Synthesis => &mut self.synthesis,
        }
    }

    pub fn is_empty(&self) -> bool {
        TurnWorkload::ALL.iter().all(|k| self.get(*k).is_empty())
    }

    /// The largest per-turn completion cap in the table, if any.
    pub fn max_max_tokens(&self) -> Option<usize> {
        TurnWorkload::ALL
            .iter()
            .filter_map(|k| self.get(*k).max_tokens)
            .max()
    }
}

/// One measured endpoint quota, for display (`llm-doctor`, docs): the
/// value the profile pins and the measurement it comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MeasuredQuota {
    pub name: &'static str,
    pub value: &'static str,
    pub basis: &'static str,
}

/// A built-in defaults profile for a family of models, matched by a glob
/// pattern on the model name (e.g. `"qwen3.6-*"`).
///
/// Each `Option` field is "apply only if the user did not set it";
/// the `extra_body` map is merged key-by-key (user keys win).
#[derive(Debug, Clone)]
pub struct ModelDefaultsProfile {
    /// Stable identifier for diagnostics (e.g. `"qwen3.6"`).
    pub name: &'static str,
    /// Glob pattern matched against `config.model` (case-insensitive).
    /// Supports `*` and `?` wildcards.  Matching tries the full model id
    /// AND its last `/`-separated segment, so an anchored pattern like
    /// `qwen3.6-*` also matches provider-prefixed (`qwen/qwen3.6-27b`) and
    /// path-qualified local (`/home/rig/models/qwen3.6-27b`) ids — see
    /// [`pattern_matches_model`].
    pub pattern: &'static str,
    pub native_function_calling: Option<bool>,
    pub streaming: Option<bool>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub context_length: Option<usize>,
    pub max_streams: Option<usize>,
    pub max_global: Option<usize>,
    /// Default for `agent.max_call_secs` (per-call wall-time cap).
    pub max_call_secs: Option<u64>,
    /// Default for `agent.context_content_ratio`: the share of the history
    /// budget (`max_context_tokens`) at which compaction starts. Per
    /// endpoint, because the right point depends on the endpoint's measured
    /// per-turn prompt growth against its history budget (global default
    /// 0.75).
    pub context_content_ratio: Option<f32>,
    /// Measured p99 per-turn prompt growth (tokens) on this endpoint. When
    /// set (and `context_content_ratio` is not explicit), the compaction
    /// ratio is DERIVED from it against the session's actual history budget
    /// (`AgentConfig::context_growth_p99_tokens`), so an explicit
    /// `max_tokens` / `context_length` moves the threshold with the budget.
    pub context_growth_p99_tokens: Option<usize>,
    /// Whether a `reasoning_effort` setting measurably changes how much this
    /// model reasons. `false` makes the reasoning-budget step-down retry
    /// switch thinking off at once instead of "lowering" an effort pin the
    /// model ignores (a retry that burns the whole budget again).
    pub reasoning_effort_honored: bool,
    /// Extra JSON fields to merge into `config.extra_body`.
    /// Keys already present in the user's `extra_body` are preserved.
    pub extra_body: Value,
    /// Per-workload quotas (thinking toggle, completion cap per turn kind);
    /// fills `config.workloads` fields the user did not set (see
    /// [`WorkloadQuotas`] for precedence).
    pub workload_quotas: Option<WorkloadQuotas>,
    /// The measured endpoint quotas behind this profile, for display.
    pub measured: &'static [MeasuredQuota],
}

impl ModelDefaultsProfile {
    /// The per-call wall-time cap this profile implies for `max_tokens`.
    ///
    /// A profile's `max_call_secs` is sized for its OWN `max_tokens` (decode
    /// time dominates: qwen38's 1,628 s = 24,576 tokens at the slowest
    /// measured 15.1 tok/s, prefill included). When `max_tokens` exceeds the
    /// profile's value the cap scales by the same ratio, rounded up:
    /// `ceil(max_call_secs * max_tokens / profile_max_tokens)` — e.g. 65,536
    /// tokens → ceil(1628 * 65536 / 24576) = 4,342 s. At or below the
    /// profile's value, or for a profile without both fields, the profile
    /// cap applies unchanged. Returns `(secs, scaled)`.
    pub fn max_call_secs_for(&self, max_tokens: usize) -> Option<(u64, bool)> {
        let secs = self.max_call_secs?;
        match self.max_tokens {
            Some(pm) if pm > 0 && max_tokens > pm => {
                let scaled = (secs as u128 * max_tokens as u128).div_ceil(pm as u128);
                Some((u64::try_from(scaled).unwrap_or(u64::MAX), true))
            }
            _ => Some((secs, false)),
        }
    }
}

/// Names of fields a profile filled in for a particular config.  Returned
/// by [`apply_profile`] for diagnostic / introspection purposes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedFields {
    pub native_function_calling: bool,
    pub streaming: bool,
    pub temperature: bool,
    pub max_tokens: bool,
    pub context_length: bool,
    pub max_streams: bool,
    pub max_global: bool,
    pub max_call_secs: bool,
    pub context_content_ratio: bool,
    /// Set when the applied `max_call_secs` was SCALED because the user
    /// raised `max_tokens` above the profile's own value: holds that
    /// user `max_tokens` (see [`ModelDefaultsProfile::max_call_secs_for`]).
    pub max_call_secs_scaled_for_max_tokens: Option<usize>,
    /// Names of `extra_body` keys that were filled from the profile.
    pub extra_body_keys: Vec<String>,
    /// Dotted `workloads.<kind>.<field>` names filled from the profile.
    pub workload_fields: Vec<String>,
    /// Profile workload quotas NOT applied because an explicit user setting
    /// holds for every turn (one human-readable line each), e.g. an
    /// `extra_body` `enable_thinking` pin. Shown by `llm-doctor` and the
    /// run summary so an overridden table is never reported as active.
    pub workload_overrides: Vec<String>,
}

impl AppliedFields {
    /// Provenance label for `agent.max_call_secs` under profile `name`:
    /// `"qwen38"`, or `"qwen38, scaled for max_tokens=65536"` when the cap
    /// was scaled to the user's larger `max_tokens`.
    pub fn max_call_secs_provenance(&self, name: &str) -> String {
        match self.max_call_secs_scaled_for_max_tokens {
            Some(n) => format!("{name}, scaled for max_tokens={n}"),
            None => name.to_string(),
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.native_function_calling
            && !self.streaming
            && !self.temperature
            && !self.max_tokens
            && !self.context_length
            && !self.max_streams
            && !self.max_global
            && !self.max_call_secs
            && !self.context_content_ratio
            && self.extra_body_keys.is_empty()
            && self.workload_fields.is_empty()
    }

    /// Render as a stable, sorted, comma-separated list for human output.
    pub fn render(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.native_function_calling {
            parts.push("native_function_calling".to_string());
        }
        if self.streaming {
            parts.push("streaming".to_string());
        }
        if self.temperature {
            parts.push("temperature".to_string());
        }
        if self.max_tokens {
            parts.push("max_tokens".to_string());
        }
        if self.context_length {
            parts.push("context_length".to_string());
        }
        if self.max_streams {
            parts.push("concurrency.max_streams".to_string());
        }
        if self.max_global {
            parts.push("concurrency.max_global".to_string());
        }
        if self.max_call_secs {
            parts.push("agent.max_call_secs".to_string());
        }
        if self.context_content_ratio {
            parts.push("agent.context_content_ratio".to_string());
        }
        for k in &self.extra_body_keys {
            parts.push(format!("extra_body.{}", k));
        }
        parts.extend(self.workload_fields.iter().cloned());
        parts.join(", ")
    }
}

fn qwen38_defaults_profile(name: &'static str, pattern: &'static str) -> ModelDefaultsProfile {
    ModelDefaultsProfile {
        name,
        pattern,
        native_function_calling: Some(false),
        streaming: Some(true),
        temperature: Some(0.7),
        // Only max_tokens (and turning thinking off, per workload below)
        // bounds this model's reasoning — no reasoning_effort / budget knob is
        // honored (re-checked 2026-09-27, see QWEN38_WORKLOAD_QUOTAS): at ~40
        // tok/s a runaway 65k-token reasoning stream runs ~27 minutes.  The
        // longest real completion need measured on llm.selfware.design
        // (2026-09-24) was 13.4k tokens (14,105 on a 2026-09-27 replay of the
        // same turn), so 24,576 leaves ~1.8x headroom while capping a runaway
        // at ~10 minutes.
        max_tokens: Some(24_576),
        // Pin context_length to 163,840 (160k) from measurements against
        // llm.selfware.design (SGLang, 2026-09-24): the largest real agent
        // prompt was 127k tokens, while bigger windows cost more than they
        // give -- time to first token was 13 s at 99k vs 40 s at 257k, and
        // decode dropped to 17 tok/s at the large end.  The model accepts far
        // more (823,538 tokens succeeded 2026-09-15), so this is an
        // operational choice, not an architectural ceiling.  Re-measured
        // 2026-09-27 (scripts/endpoint_quota_bench.py context, one stream,
        // light load): prefill is LINEAR at ~7.2-7.8k tok/s from 48k to 208k
        // prompt tokens (TTFT 6.4 s at 48k, 16.3 s at 124k, 23.2 s at 170k,
        // 28.2 s at 208k) with no knee to cut at, and an identical repeat
        // request is no faster (5.6-7.9k tok/s; no prefix-cache reuse), so
        // every turn pays its whole prompt. Decode, one stream on an idle
        // server: 63.5 tok/s at 30 tokens, 55 at 52k, 42-51 at 106k, 36-47
        // at 158k (41 -> 34 with three other streams). A cold prompt whose
        // prefill exceeds ~60 s (≳ 430k tokens idle, less under load) is cut
        // by the gateway before its first byte (367k and 500k probes cut at
        // 62-63 s). The window is kept: the cost is per prompt token, which the
        // compaction threshold (context_content_ratio) governs.
        context_length: Some(163_840),
        // The endpoint runs SGLang with max_running_requests = 8: a 9th
        // concurrent stream only queues server-side (and its prefill wait
        // counts against our timeouts), so pin max_streams to exactly 8.
        max_streams: Some(8),
        // Pin max_global = max_streams + 8 so 8 inflight streams still leave
        // 8 global permits (the default max_tools) for simultaneous tool
        // execution, avoiding tool starvation.
        max_global: Some(16),
        // Fail a stuck call with a typed CallTimeBudgetExceeded instead of
        // hanging, while never killing a call max_tokens allows. Sized as a
        // full 24,576-token stream at the SLOWEST whole-call rate measured
        // on this endpoint: 15.1 tok/s (val083 b2_350000, 3,495 tokens in
        // 231 s at a 154k prompt; across the 42 val082/val083 calls with
        // >= 1,500 completion tokens p05 16.5, p10 19.0). Whole-call rate
        // includes prefill, so no separate allowance: ceil(24,576 / 15.1) =
        // 1,628 s. The former 600 s assumed ~42 tok/s; under load the
        // endpoint decodes at 17–20 tok/s and a legitimate 13,799-token
        // mid-run turn took 701 s (val083 review step 19) — the 600 s cap
        // killed calls max_tokens permits. Runaway protection stays with
        // max_tokens (it bounds any stream); this cap only has to catch a
        // stalled or pathological call. Static rather than tracking the
        // run's decode rate: the first call of a run has no measurement and
        // must not be killed either. When the user raises max_tokens,
        // `max_call_secs_for` scales the cap proportionally (0.8.2 D7).
        max_call_secs: Some(1_628),
        // Compaction starts as late as the measured per-turn prompt growth
        // allows: threshold = history budget − p99 growth of ONE turn, so a
        // single step from just under the threshold does not overshoot the
        // hard budget (which forces the lossy hard fallback). History budget
        // with these defaults: 163,840 − 24,576 (max_tokens) − 32,768 (20%
        // margin) = 106,496. Per-turn prompt growth over 1,206 growing
        // steps in the val082–val090 runs (turn artifacts, server-counted
        // prompt tokens): p50 962, p90 5,658, p99 21,221, max 41,677. So
        // 1 − 21,221 / 106,496 = 0.8007 (threshold 85,275; the gap equals
        // the p99). The global 0.75 (79,872) compacted ~5.3k tokens
        // earlier than the growth requires; a review keeps more of what it
        // read, at +0.7 s prefill per turn (~7.2-7.8k tok/s measured
        // 2026-09-27, no prefix-cache reuse). The threshold is compared
        // with the ESTIMATED history, which for histories ≥ 80k ran at or
        // above the server count (server/estimate p99 0.992, 154 turns).
        //
        // What was measured is the HEADROOM (p99 growth), not the ratio: a
        // fixed 0.80 derived for this 106,496 budget was applied whatever
        // max_tokens / context_length the user set (review 2026-09-27). The
        // ratio is derived at load from the session's own budget
        // (`Config::effective_context_content_ratio`).
        context_content_ratio: None,
        context_growth_p99_tokens: Some(21_221),
        // No reasoning_effort value bounds this model's reasoning (2026-09-27
        // knobs run: "low" 643–8,192 and "xhigh" 7,072–8,192 reasoning tokens
        // on one prompt, the same spread as no setting). The step-down retry
        // after a reasoning-exhausted turn therefore switches thinking off at
        // once: with an `xhigh` pin it used to retry at "medium", which the
        // model ignores, and a review synthesis turn that had spent ~80k
        // characters of reasoning on an empty answer burned its budget again.
        reasoning_effort_honored: false,
        extra_body: json!({
            "top_p": 0.95,
            "top_k": 20,
            "min_p": 0.0,
            "presence_penalty": 0.0,
            "repetition_penalty": 1.0,
            "chat_template_kwargs": {
                "enable_thinking": true,
                "preserve_thinking": false,
            },
        }),
        workload_quotas: Some(QWEN38_WORKLOAD_QUOTAS),
        measured: QWEN38_MEASURED,
    }
}

/// Per-workload quotas for qwen38 on llm.selfware.design, measured
/// 2026-09-27 (docs/model-playbook.md § 7): replays of recorded agent
/// requests with `scripts/endpoint_quota_bench.py turns` (thinking on/off,
/// 2 reps, `max_tokens` 24,576), then live runs (slugify review and edit,
/// before/after).
///
/// - `planning` (the first call of a task): thinking OFF, `max_tokens`
///   12,288. Over 6 recorded first turns (4.5–23k prompts; 12 replies per
///   mode) off opened with a tool call 12/12 times, on 10/12 — the 2 misses
///   answered a citation task "from memory" with no read (71–96 s, 414–828
///   reasoning tokens) where off read the file first (2–3 s). A reply
///   without a tool call is re-asked once under `synthesis` (a greeting thus
///   costs ~1.7 s more than before; the gain is on tasks). 12,288 covers
///   the largest visible output in 2,142 recorded turns (7,788 tokens).
/// - `mechanical` (after a read-only tool batch): thinking ON — the
///   evidence for switching it off did not survive live runs. Replays
///   favoured off (6/6 valid tool calls either way, generation 13.5 s →
///   1.6 s median), but in 3 live reviews with it off one ended on a 6-word
///   thinking-off fragment as its answer (before the escalation existed),
///   one had a thinking-off turn "repair" correct code with a FIM edit
///   mid-review, and one produced a good report but needed 5 escalations in
///   20 read turns (three 1-character replies). The thinking-on baselines
///   varied from a good report in 968 s to no answer at the 3,600 s limit,
///   so wall time does not separate the modes at n = 2–3. Opt in with
///   `[workloads.mechanical] enable_thinking = false` (the escalation
///   still guards the answer).
/// - `edit` (after a mutating/verification batch): thinking on — replays
///   4/4 vs 4/4 equivalent; no evidence to take thinking from code edits.
/// - `synthesis` (the review's synthesis phase, a turn after a
///   gate/correction directive, or any turn not following a tool batch):
///   thinking on, `max_tokens` 16,384. The longest successful answer or
///   report measured is 14,105 tokens (a REVIEW.md turn replayed; recorded
///   answer turns p99 10,431, max 12,362 of 287). A live review synthesis
///   turn spent ~80k characters (~21k tokens) of reasoning on an EMPTY
///   answer under 24,576; at 16,384 such a runaway ends ~8k tokens sooner
///   and the step-down retry answers with thinking off
///   (`reasoning_effort_honored = false`).
///
/// Only `enable_thinking=false` and `max_tokens` bound reasoning here: no
/// budget knob is honored (`thinking_budget` / `max_thinking_tokens` = 128,
/// in `chat_template_kwargs` or top level → 1,373–8,192 reasoning tokens;
/// `reasoning_effort` low vs xhigh overlap, 643–8,192; 2 reps each).
pub const QWEN38_WORKLOAD_QUOTAS: WorkloadQuotas = WorkloadQuotas {
    planning: WorkloadQuota {
        enable_thinking: Some(false),
        max_tokens: Some(12_288),
    },
    mechanical: WorkloadQuota {
        enable_thinking: Some(true),
        max_tokens: None,
    },
    edit: WorkloadQuota {
        enable_thinking: Some(true),
        max_tokens: None,
    },
    synthesis: WorkloadQuota {
        enable_thinking: Some(true),
        max_tokens: Some(16_384),
    },
};

/// The measured endpoint quotas behind the qwen38 profile, as `llm-doctor`
/// shows them. Keep in step with the profile fields and
/// docs/model-playbook.md § 7.
pub const QWEN38_MEASURED: &[MeasuredQuota] = &[
    MeasuredQuota {
        name: "context_length",
        value: "163840",
        basis: "prefill ~7.2-7.9k tok/s, linear, no prefix-cache reuse (TTFT 6.4 s at 48k, \
                16.3 s at 124k, 23.2 s at 170k, 28.2 s at 208k); decode 55 tok/s at 52k, 36-47 \
                at 158k idle; cold prompts over ~60 s of prefill (367k, 500k) are cut by the \
                gateway; largest real agent prompt 127k (2026-09-24)",
    },
    MeasuredQuota {
        name: "max_tokens",
        value: "24576",
        basis: "longest real completion 13,799 tokens (val083 review), 14,105 on replay \
                2026-09-27",
    },
    MeasuredQuota {
        name: "agent.max_call_secs",
        value: "1628",
        basis: "24,576 tokens at the slowest whole-call rate, 15.1 tok/s (val083)",
    },
    MeasuredQuota {
        name: "agent.context_growth_p99_tokens",
        value: "21221",
        basis: "p99 per-turn prompt growth (1,206 steps, val082-val090); compaction ratio \
                derived at load = 1 - 21,221 / history budget (0.80 at 106,496)",
    },
    MeasuredQuota {
        name: "workloads.planning",
        value: "thinking off, max_tokens 12288",
        basis: "first call opened with a tool call 12/12 off vs 10/12 on (2 answered a citation \
                task from memory, 71-96 s); a reply without a tool call is re-asked once as \
                synthesis",
    },
    MeasuredQuota {
        name: "workloads.mechanical",
        value: "thinking on",
        basis: "replays favoured off (6/6 valid calls, 13.5 s -> 1.6 s); 3 live reviews with it \
                off: a degenerate answer, a mid-review code edit, 5 escalations in 20 read turns; \
                opt in with [workloads.mechanical] enable_thinking = false",
    },
    MeasuredQuota {
        name: "workloads.synthesis",
        value: "thinking on, max_tokens 16384",
        basis: "longest successful answer/report 14,105 tokens; a live synthesis spent ~21k \
                reasoning tokens on an empty answer - the cap ends that sooner and the step-down \
                retry answers with thinking off",
    },
    MeasuredQuota {
        name: "thinking budget knobs",
        value: "none honored",
        basis: "thinking_budget / max_thinking_tokens = 128 -> 1,373-8,192 reasoning tokens; \
                reasoning_effort low vs xhigh overlap (643-8,192); only enable_thinking=false \
                and max_tokens bound reasoning",
    },
];

/// Where a resolved `workloads.<kind>.<field>` value came from, for display.
fn workload_field_origin(config: &crate::config::Config, key: &str, set: bool) -> &'static str {
    use crate::config::ConfigSource;
    match config.sources.get(key) {
        Some(ConfigSource::Profile(_)) => "profile",
        _ if set => "config",
        _ => "session",
    }
}

/// The active per-workload quota table for `config`, one line per workload
/// kind, followed by any overridden-profile notes. Empty when no workload
/// quota is configured and no profile table was overridden. Shared by
/// `llm-doctor` and the run summary so both show the same table.
pub fn workload_quota_lines(config: &crate::config::Config) -> Vec<String> {
    let mut lines = Vec::new();
    if config.workloads.is_empty() && config.workload_overrides.is_empty() {
        return lines;
    }
    for kind in TurnWorkload::ALL {
        let q = config.workloads.get(kind);
        let think_key = format!("workloads.{kind}.enable_thinking");
        let max_key = format!("workloads.{kind}.max_tokens");
        let thinking = match q.enable_thinking {
            Some(true) => format!(
                "thinking on [{}]",
                workload_field_origin(config, &think_key, true)
            ),
            Some(false) => format!(
                "thinking off [{}]",
                workload_field_origin(config, &think_key, true)
            ),
            None => "thinking as extra_body [session]".to_string(),
        };
        let max_tokens = match q.max_tokens {
            Some(n) => format!(
                "max_tokens {n} [{}]",
                workload_field_origin(config, &max_key, true)
            ),
            None => format!("max_tokens {} [session]", config.max_tokens),
        };
        lines.push(format!("{:<10}  {thinking}, {max_tokens}", kind.as_str()));
    }
    for note in &config.workload_overrides {
        lines.push(format!("! {note}"));
    }
    lines
}

/// The matched profile's measured endpoint quotas as display lines
/// (`name = value — measured: basis`); empty when the profile records none.
pub fn measured_quota_lines(config: &crate::config::Config) -> Vec<String> {
    let Some(profile) = match_profile(&config.model) else {
        return Vec::new();
    };
    profile
        .measured
        .iter()
        .map(|m| format!("{} = {} — measured: {}", m.name, m.value, m.basis))
        .collect()
}

/// Built-in profile rules.  Order matters: the *first* matching pattern
/// wins.  Keep more-specific patterns before more-general ones.
pub fn builtin_profiles() -> Vec<ModelDefaultsProfile> {
    vec![
        // Nemotron 3 Ultra (nvidia) — the project's default model via the
        // OpenRouter free tier (`nvidia/nemotron-3-ultra-550b-a55b:free`):
        // 1M context, 65,536 completion cap, $0, tools + tool_choice
        // supported (verified against the OpenRouter model catalog
        // 2026-09-08).  No model-specific sampling kit is pinned yet —
        // global defaults (temperature 1.0) apply.
        ModelDefaultsProfile {
            name: "nemotron-3-ultra",
            pattern: "*nemotron-3-ultra*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: None,
            max_tokens: Some(65536),
            context_length: None,
            max_streams: None,
            max_global: None,
            max_call_secs: None,
            context_content_ratio: None,
            context_growth_p99_tokens: None,
            reasoning_effort_honored: true,
            extra_body: json!({}),
            workload_quotas: None,
            measured: &[],
        },
        // GLM-5.2 (z-ai) — card-recommended
        // reasoning-mode sampling: temperature=1.0, top_p=0.95, with the
        // model's thinking enabled via the chat template.  selfware parses the
        // resulting reasoning_content automatically, so no client change is
        // needed.  Provider pinning (full 1M context) stays in user config.
        ModelDefaultsProfile {
            name: "glm-5.2",
            pattern: "*glm-5.2*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: Some(1.0),
            max_tokens: Some(65536),
            context_length: None,
            max_streams: None,
            max_global: None,
            max_call_secs: None,
            context_content_ratio: None,
            context_growth_p99_tokens: None,
            reasoning_effort_honored: true,
            extra_body: json!({
                "top_p": 0.95,
                "chat_template_kwargs": {
                    "enable_thinking": true,
                },
            }),
            workload_quotas: None,
            measured: &[],
        },
        // Qwen 3.8 / Qwen3.8-Flash-Next — endpoint deployment uses SGLang.
        // Follows upstream card defaults: temp=0.7 for agentic reproducibility,
        // top_p=0.95, top_k=20, min_p=0.0, presence_penalty=0.0, repetition_penalty=1.0.
        // Sets preserve_thinking=false as the agent-loop default to keep multi-turn
        // context growth compact. Operational limits measured on llm.selfware.design
        // (2026-09-24): context 163,840, max_tokens 24,576, 8 streams (the server's
        // max_running_requests) with max_global = 16.  See qwen38_defaults_profile.
        // Native FC is false since SGLang Qwen3.8 emits XML tool calls in content.
        qwen38_defaults_profile("qwen3.8", "qwen3.8-*"),
        qwen38_defaults_profile("qwen38", "qwen38-flash-*"),
        // Qwen 3.6 — needs the high presence_penalty / min_p kit and the
        // SGLang `preserve_thinking` template knob to produce its best
        // function-calling output.  Without these, SWE-bench Pro hovers
        // around 0/10 even on a 27B parameter checkpoint.
        ModelDefaultsProfile {
            name: "qwen3.6",
            pattern: "qwen3.6-*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: Some(0.7),
            max_tokens: Some(32768),
            context_length: None,
            max_streams: None,
            max_global: None,
            max_call_secs: None,
            context_content_ratio: None,
            context_growth_p99_tokens: None,
            reasoning_effort_honored: true,
            extra_body: json!({
                "top_p": 0.8,
                "top_k": 20,
                "min_p": 0.0,
                "presence_penalty": 1.5,
                "chat_template_kwargs": {
                    "enable_thinking": true,
                    "preserve_thinking": true,
                },
            }),
            workload_quotas: None,
            measured: &[],
        },
        // Qwen 3.5 — earlier generation, recommended sampling is closer to
        // a vanilla nucleus setup with no presence penalty.
        ModelDefaultsProfile {
            name: "qwen3.5",
            pattern: "qwen3.5-*",
            native_function_calling: Some(true),
            streaming: None,
            temperature: Some(0.6),
            max_tokens: Some(32768),
            context_length: None,
            max_streams: None,
            max_global: None,
            max_call_secs: None,
            context_content_ratio: None,
            context_growth_p99_tokens: None,
            reasoning_effort_honored: true,
            extra_body: json!({
                "top_p": 0.95,
                "top_k": 20,
                "presence_penalty": 0.0,
            }),
            workload_quotas: None,
            measured: &[],
        },
        // Anthropic Claude — native tools + streaming work out of the box.
        // Sampling/extra_body left to user (Claude API rejects most knobs).
        ModelDefaultsProfile {
            name: "claude",
            pattern: "claude-*",
            native_function_calling: Some(true),
            streaming: Some(true),
            temperature: None,
            max_tokens: None,
            context_length: None,
            max_streams: None,
            max_global: None,
            max_call_secs: None,
            context_content_ratio: None,
            context_growth_p99_tokens: None,
            reasoning_effort_honored: true,
            extra_body: Value::Null,
            workload_quotas: None,
            measured: &[],
        },
        // OpenAI GPT — same story: native tools + streaming.
        ModelDefaultsProfile {
            name: "gpt",
            pattern: "gpt-*",
            native_function_calling: Some(true),
            streaming: Some(true),
            temperature: None,
            max_tokens: None,
            context_length: None,
            max_streams: None,
            max_global: None,
            max_call_secs: None,
            context_content_ratio: None,
            context_growth_p99_tokens: None,
            reasoning_effort_honored: true,
            extra_body: Value::Null,
            workload_quotas: None,
            measured: &[],
        },
    ]
}

/// Match `model` against `pattern`.  Glob semantics: `*` matches any run
/// of characters (including empty), `?` matches exactly one.  Comparison
/// is case-insensitive so e.g. `Qwen3.6-27B` matches `qwen3.6-*`.
pub fn glob_matches(pattern: &str, model: &str) -> bool {
    let pattern = pattern.to_ascii_lowercase();
    let model = model.to_ascii_lowercase();
    glob_matches_inner(pattern.as_bytes(), model.as_bytes())
}

fn glob_matches_inner(pat: &[u8], s: &[u8]) -> bool {
    // Iterative backtracking matcher — small, no allocations, no deps.
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star_i, mut star_j) = (None::<usize>, 0usize);
    while j < s.len() {
        if i < pat.len() && (pat[i] == b'?' || pat[i] == s[j]) {
            i += 1;
            j += 1;
        } else if i < pat.len() && pat[i] == b'*' {
            star_i = Some(i);
            star_j = j;
            i += 1;
        } else if let Some(si) = star_i {
            i = si + 1;
            star_j += 1;
            j = star_j;
        } else {
            return false;
        }
    }
    while i < pat.len() && pat[i] == b'*' {
        i += 1;
    }
    i == pat.len()
}

/// Match a model id against a profile `pattern`, trying the full id AND its
/// last `/`-separated segment.
///
/// Model ids arrive in several shapes:
/// - bare:                 `qwen3.6-27b`
/// - provider-prefixed:    `qwen/qwen3.6-27b` (OpenRouter-style `vendor/model`)
/// - path-qualified local: `/home/rig/models/qwen3.6-27b` (sglang/vLLM serve dir)
///
/// Anchored profile globs describe the *model name*, not the routing prefix
/// or the serving directory, so after trying the full id we retry against
/// the substring after the final `/`.  Deterministic: same glob engine,
/// fixed two-candidate list, first success wins.  No regex is introduced.
pub fn pattern_matches_model(pattern: &str, model: &str) -> bool {
    if glob_matches(pattern, model) {
        return true;
    }
    match model.rsplit_once('/') {
        Some((_, tail)) if !tail.is_empty() => glob_matches(pattern, tail),
        _ => false,
    }
}

/// Find the first built-in profile whose pattern matches `model`.
pub fn match_profile(model: &str) -> Option<ModelDefaultsProfile> {
    builtin_profiles()
        .into_iter()
        .find(|p| pattern_matches_model(p.pattern, model))
}

/// Apply `profile`'s defaults to `config` for any field the user did not
/// set explicitly.  `user_explicit` describes which fields were present
/// in the user's TOML; profile values do NOT override those.
///
/// Returns the set of fields that were actually filled in by the profile
/// so callers can surface this in `selfware autoconfig` output.
pub fn apply_profile(
    config: &mut crate::config::Config,
    profile: &ModelDefaultsProfile,
    user_explicit: &UserExplicitFields,
) -> AppliedFields {
    let mut applied = AppliedFields::default();

    if !user_explicit.native_function_calling {
        if let Some(v) = profile.native_function_calling {
            config.agent.native_function_calling = v;
            applied.native_function_calling = true;
        }
    }
    if !user_explicit.streaming {
        if let Some(v) = profile.streaming {
            config.agent.streaming = v;
            applied.streaming = true;
        }
    }
    if !user_explicit.temperature {
        if let Some(v) = profile.temperature {
            config.temperature = v;
            applied.temperature = true;
        }
    }
    if !user_explicit.max_tokens {
        if let Some(v) = profile.max_tokens {
            config.max_tokens = v;
            applied.max_tokens = true;
        }
    }
    if !user_explicit.context_length {
        if let Some(v) = profile.context_length {
            config.context_length = v;
            applied.context_length = true;
        }
    }
    if !user_explicit.max_streams {
        if let Some(v) = profile.max_streams {
            config.concurrency.max_streams = v;
            applied.max_streams = true;
        }
    }
    if !user_explicit.max_global {
        if let Some(v) = profile.max_global {
            config.concurrency.max_global = v;
            applied.max_global = true;
        }
    }
    if !user_explicit.context_content_ratio {
        if let Some(v) = profile.context_content_ratio {
            config.agent.context_content_ratio = v;
            applied.context_content_ratio = true;
        }
        if let Some(growth) = profile.context_growth_p99_tokens {
            if config.agent.context_growth_p99_tokens.is_none() {
                config.agent.context_growth_p99_tokens = Some(growth);
                config.agent.context_content_ratio = config.effective_context_content_ratio();
                applied.context_content_ratio = true;
            }
        }
    }

    // Per-workload quotas. Resolved BEFORE max_call_secs so the cap is
    // sized for the largest completion any turn may ask for.
    apply_workload_quotas(config, profile, user_explicit, &mut applied);

    if !user_explicit.max_call_secs {
        // Scale only for an EXPLICIT user max_tokens (top-level or a
        // `[workloads.<kind>] max_tokens`): a profile-filled value never
        // exceeds the profile's own max_tokens (no scaling), and an explicit
        // max_call_secs never reaches this branch (user wins).
        let base_max_tokens = if user_explicit.max_tokens {
            config.max_tokens
        } else {
            profile.max_tokens.unwrap_or(config.max_tokens)
        };
        let effective_max_tokens = user_explicit
            .workload_max_tokens
            .map_or(base_max_tokens, |w| w.max(base_max_tokens));
        if let Some((v, scaled)) = profile.max_call_secs_for(effective_max_tokens) {
            config.agent.max_call_secs = Some(v);
            applied.max_call_secs = true;
            if scaled {
                applied.max_call_secs_scaled_for_max_tokens = Some(effective_max_tokens);
            }
        }
    }

    // Merge extra_body — only keys NOT already present in the user's map.
    if let Value::Object(profile_extra) = &profile.extra_body {
        let dest = config.extra_body.get_or_insert_with(Map::new);
        for (k, v) in profile_extra {
            if !user_explicit.extra_body_keys.iter().any(|uk| uk == k) && !dest.contains_key(k) {
                dest.insert(k.clone(), v.clone());
                applied.extra_body_keys.push(k.clone());
            }
        }
        applied.extra_body_keys.sort();
    }

    applied
}

/// Whether an `extra_body` pins `enable_thinking` for every request (top
/// level or under `chat_template_kwargs`).
pub fn extra_body_pins_enable_thinking(extra_body: Option<&Map<String, Value>>) -> bool {
    extra_body.is_some_and(|map| {
        map.contains_key("enable_thinking")
            || map
                .get("chat_template_kwargs")
                .and_then(Value::as_object)
                .is_some_and(|kw| kw.contains_key("enable_thinking"))
    })
}

/// Fill `config.workloads` from the profile's table, field by field, in the
/// precedence documented on [`WorkloadQuotas`]. Must run while
/// `config.extra_body` still holds only the USER's keys (before the
/// profile's `extra_body` merge) so a user `enable_thinking` pin is seen as
/// the user's.
fn apply_workload_quotas(
    config: &mut crate::config::Config,
    profile: &ModelDefaultsProfile,
    user_explicit: &UserExplicitFields,
    applied: &mut AppliedFields,
) {
    let Some(table) = profile.workload_quotas else {
        return;
    };
    let thinking_pinned = extra_body_pins_enable_thinking(config.extra_body.as_ref());
    let mut thinking_overridden = false;
    let mut max_tokens_overridden = false;
    for kind in TurnWorkload::ALL {
        let from_profile = table.get(kind);
        let slot = config.workloads.get_mut(kind);
        if slot.enable_thinking.is_none() {
            if let Some(v) = from_profile.enable_thinking {
                if thinking_pinned {
                    thinking_overridden = true;
                } else {
                    slot.enable_thinking = Some(v);
                    applied
                        .workload_fields
                        .push(format!("workloads.{kind}.enable_thinking"));
                }
            }
        }
        if slot.max_tokens.is_none() {
            if let Some(v) = from_profile.max_tokens {
                if user_explicit.max_tokens {
                    max_tokens_overridden = true;
                } else {
                    slot.max_tokens = Some(v);
                    applied
                        .workload_fields
                        .push(format!("workloads.{kind}.max_tokens"));
                }
            }
        }
    }
    if thinking_overridden {
        applied.workload_overrides.push(
            "per-turn enable_thinking not applied: extra_body pins enable_thinking for every \
             turn (explicit wins; remove the pin or set [workloads.<kind>] enable_thinking)"
                .to_string(),
        );
    }
    if max_tokens_overridden {
        applied.workload_overrides.push(format!(
            "per-turn max_tokens not applied: max_tokens = {} is set explicitly for every turn \
             (explicit wins; set [workloads.<kind>] max_tokens to cap a kind)",
            config.max_tokens
        ));
    }
}

/// Set of fields the user explicitly set in their TOML config.  Built by
/// the loader before profile application.
#[derive(Debug, Default, Clone)]
pub struct UserExplicitFields {
    pub native_function_calling: bool,
    pub streaming: bool,
    pub temperature: bool,
    pub max_tokens: bool,
    pub context_length: bool,
    pub max_streams: bool,
    pub max_global: bool,
    pub max_call_secs: bool,
    pub context_content_ratio: bool,
    pub extra_body_keys: Vec<String>,
    /// The largest explicit `[workloads.<kind>] max_tokens`, if any — the
    /// per-call wall-time cap scales to it like to a raised `max_tokens`.
    pub workload_max_tokens: Option<usize>,
}

impl UserExplicitFields {
    /// Build from raw TOML content (what was on disk before defaults).
    /// Unknown / unparseable content yields an empty set so that profiles
    /// still apply.
    pub fn from_toml(content: &str) -> Self {
        let mut out = Self::default();
        let table = match toml::from_str::<toml::Value>(content) {
            Ok(toml::Value::Table(t)) => t,
            _ => return out,
        };
        if table.contains_key("temperature") {
            out.temperature = true;
        }
        if table.contains_key("max_tokens") {
            out.max_tokens = true;
        }
        if table.contains_key("context_length") {
            out.context_length = true;
        }
        if let Some(toml::Value::Table(concurrency)) = table.get("concurrency") {
            if concurrency.contains_key("max_streams") {
                out.max_streams = true;
            }
            if concurrency.contains_key("max_global") {
                out.max_global = true;
            }
        }
        if let Some(toml::Value::Table(agent)) = table.get("agent") {
            if agent.contains_key("native_function_calling") {
                out.native_function_calling = true;
            }
            if agent.contains_key("streaming") {
                out.streaming = true;
            }
            if agent.contains_key("max_call_secs") {
                out.max_call_secs = true;
            }
            if agent.contains_key("context_content_ratio") {
                out.context_content_ratio = true;
            }
        }
        if let Some(toml::Value::Table(extra)) = table.get("extra_body") {
            out.extra_body_keys = extra.keys().cloned().collect();
        }
        if let Some(toml::Value::Table(workloads)) = table.get("workloads") {
            out.workload_max_tokens = workloads
                .values()
                .filter_map(|w| w.get("max_tokens")?.as_integer())
                .filter_map(|n| usize::try_from(n).ok())
                .max();
        }
        out
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config/model_profiles/model_profiles_test.rs"]
mod tests;
