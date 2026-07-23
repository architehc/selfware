//! Self-improvement presets — the directions selfware can expand in, locally.
//!
//! Each preset is a runnable self-improvement task carrying everything a safe run
//! needs: the task prompt, the invariants it must not break (drawn from the
//! logical layer), the context recipe to assemble, and how to verify success.
//! Together they turn "implement anything locally" into a menu: pick a direction,
//! the harness runs it against the configured model with the guardrails attached.

use serde::{Deserialize, Serialize};

/// One self-improvement direction.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Preset {
    pub id: String,
    pub title: String,
    /// Broad direction: context, refactor, capability, safety, comprehension, automation.
    pub direction: String,
    /// Logical-layer capability this touches (for invariants + scoping).
    pub capability: String,
    pub summary: String,
    /// The task prompt handed to the model.
    pub task: String,
    /// Contracts the run must preserve — refuse the change if any would break.
    pub invariants: Vec<String>,
    /// Context recipe: how to assemble what the model sees.
    pub context_recipe: String,
    /// How the run proves it succeeded.
    pub verify: String,
}

struct Seed {
    id: &'static str,
    title: &'static str,
    direction: &'static str,
    capability: &'static str,
    summary: &'static str,
    task: &'static str,
    invariants: &'static [&'static str],
    context_recipe: &'static str,
    verify: &'static str,
}

const VERIFY_DEFAULT: &str = "cargo build && cargo test; no clippy regressions; public API unchanged unless intended.";

const SEEDS: &[Seed] = &[
    // ---- The four selected targets -------------------------------------------
    Seed {
        id: "symbol-context-selection",
        title: "Symbol-level context selection",
        direction: "context",
        capability: "memory",
        summary: "Select relevant functions/signatures for a task, not whole files.",
        task: "In src/evolve/context_selector.rs and src/evolve/map.rs, add symbol-level \
               selection: given a task target, pick only the relevant functions/impls/signatures \
               (via the AST/structure analyzer) instead of whole files, and assemble those as the \
               context. Add an endpoint /api/context/select/symbols and a 'specialized' tier. \
               Measure the token reduction vs file-level selection.",
        invariants: &[
            "Cited evidence stays line-accurate to the original source.",
            "Selection is deterministic for the same target + graph revision.",
            "No symbol is included without its enclosing type/trait context.",
        ],
        context_recipe: "map + task-scoped signatures",
        verify: "New unit tests for symbol extraction; token count strictly lower than file-level for the same task.",
    },
    Seed {
        id: "dedup-clutter-strip",
        title: "Dedup + clutter-strip before tokenizing",
        direction: "context",
        capability: "memory",
        summary: "Remove duplicate content and strip comments/imports/inline-tests before the tokenizer.",
        task: "Add a context-reduction pass that runs before token counting and before content is \
               sent: reuse src/evolve/fn_dedup.rs to drop duplicated function bodies, strip comments \
               (src/evolve/context_reduce.rs), use-imports, and inline #[cfg(test)] blocks. Apply it \
               to map expansion and evidence assembly. Report tokens saved.",
        invariants: &[
            "Stripping never changes the meaning of retained code.",
            "Line numbers in emitted evidence stay self-consistent with the stripped text.",
            "Deduplication keeps at least one canonical copy and links the rest.",
        ],
        context_recipe: "compact (comment-stripped) + dedup",
        verify: "Golden tests: stripped output parses; dedup count matches fn_dedup; measured token drop.",
    },
    Seed {
        id: "split-giant-files",
        title: "Split the 3 giant files",
        direction: "refactor",
        capability: "tool_dispatch",
        summary: "Break tool_dispatch.rs / cli/mod.rs / interactive.rs into cohesive modules.",
        task: "Refactor src/agent/tool_dispatch.rs (~4000 lines), src/cli/mod.rs (~4400), and \
               src/agent/interactive.rs (~3500) into submodules along responsibility seams \
               (e.g. tool_dispatch: result_summary, error_kind, command_analysis, confirmation, \
               execution). Move one cohesive group at a time; keep impl blocks split across files. \
               Preserve the public API via re-exports.",
        invariants: &[
            "Zero behavior change — pure refactor.",
            "The full test suite is green after every moved group.",
            "Public API (paths, signatures) is preserved via re-exports.",
        ],
        context_recipe: "full (target file) + dependents' signatures",
        verify: "cargo build && cargo test green at each step; git diff shows moves, not rewrites.",
    },
    Seed {
        id: "unify-safety-gate-context",
        title: "Unify safety engines + gate context",
        direction: "safety",
        capability: "safety",
        summary: "Merge evolve::context_trust into safety::context_guard and gate context before send.",
        task: "Consolidate src/evolve/context_trust.rs into src/safety/context_guard.rs (keep the \
               richer provenance/taint model), expose one API, and gate the assistant flow: every \
               assembled context block is scanned and any high-severity finding from an untrusted \
               source blocks the send (or requires explicit override) before it reaches the model.",
        invariants: &[
            "Untrusted content never reaches the model unflagged.",
            "A high-severity pollution finding from an untrusted source blocks the send by default.",
            "The gate is auditable — every decision is logged to the JSONL trail.",
        ],
        context_recipe: "signatures of both modules + the assistant flow",
        verify: "Tests: poisoned untrusted input is blocked; clean workspace source passes; audit entry written.",
    },
    // ---- Expansion directions (templates) ------------------------------------
    Seed {
        id: "add-tool",
        title: "Add a new tool",
        direction: "capability",
        capability: "tool_dispatch",
        summary: "Register a new tool in the ToolRegistry with safety + tests.",
        task: "Add a new tool to src/tools/ and register it in the ToolRegistry. Wire it through the \
               safety gate, add its schema, and cover it with unit + integration tests. Replace \
               <TOOL> with the concrete tool to build.",
        invariants: &[
            "Every tool call passes the safety check before execution.",
            "The tool declares whether it is parallel-safe.",
            "A denied call never mutates state.",
        ],
        context_recipe: "map + tools module signatures",
        verify: VERIFY_DEFAULT,
    },
    Seed {
        id: "add-context-tier",
        title: "Add a context tier / recipe",
        direction: "context",
        capability: "memory",
        summary: "Add a new named context recipe for a common interaction.",
        task: "Add a new context recipe to the tier system (src/evolve/context.rs + map.rs) that \
               auto-composes the right content for a named interaction (e.g. 'fix-bug', 'extend', \
               'review'), including the logical-layer invariants of the touched capability.",
        invariants: &[
            "Token size is reported honestly (measured, not guessed).",
            "The recipe is reproducible for the same inputs.",
        ],
        context_recipe: "logical capability + selection",
        verify: VERIFY_DEFAULT,
    },
    Seed {
        id: "add-loop-type",
        title: "Add a loop type",
        direction: "automation",
        capability: "task_loop",
        summary: "Add a first-class loop type (review / watch / cron) to the loop registry.",
        task: "Add a new loop type as a first-class object with type, trigger, context recipe, state, \
               and history, integrated with the AgentState machine and the evolution loop.",
        invariants: &[
            "Every iteration runs budget and cancellation checks before work.",
            "State advances only through the defined state machine.",
        ],
        context_recipe: "logical (task_loop capability) + loop modules",
        verify: VERIFY_DEFAULT,
    },
    Seed {
        id: "add-perspective",
        title: "Add a comprehension perspective",
        direction: "comprehension",
        capability: "interface",
        summary: "Add a new graph lens / view for understanding the system.",
        task: "Add a new perspective (lens) to the graph UI that recolors/regroups nodes by a new \
               dimension (e.g. churn, coverage, complexity, coupling), with a legend and backing data.",
        invariants: &[
            "Rendering never blocks the task loop.",
            "The lens degrades gracefully when a dimension is absent on a node.",
        ],
        context_recipe: "map + web assets signatures",
        verify: "Served assets updated; lens data present on nodes; UI smoke test green.",
    },
];

/// The full preset library.
pub fn presets() -> Vec<Preset> {
    SEEDS
        .iter()
        .map(|s| Preset {
            id: s.id.to_string(),
            title: s.title.to_string(),
            direction: s.direction.to_string(),
            capability: s.capability.to_string(),
            summary: s.summary.to_string(),
            task: normalize_ws(s.task),
            invariants: s.invariants.iter().map(|i| i.to_string()).collect(),
            context_recipe: s.context_recipe.to_string(),
            verify: s.verify.to_string(),
        })
        .collect()
}

/// Look up one preset by id.
pub fn preset(id: &str) -> Option<Preset> {
    presets().into_iter().find(|p| p.id == id)
}

/// Render the full task prompt for a preset: the task plus its invariants and
/// verification, so a self-improvement run carries its own guardrails.
pub fn render_prompt(p: &Preset) -> String {
    let invariants = p
        .invariants
        .iter()
        .map(|i| format!("- {i}"))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "{task}\n\nInvariants you MUST preserve (refuse the change if any would break):\n{invariants}\n\nContext recipe: {recipe}\nVerify: {verify}",
        task = p.task,
        recipe = p.context_recipe,
        verify = p.verify,
    )
}

fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
#[path = "../../tests/unit/evolve/presets_test.rs"]
mod presets_test;
