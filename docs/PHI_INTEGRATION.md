# Integrating Phi into the Selfware product

## What Phi actually is

The fox is a renderer. The product is the **signal layer** underneath it, and that
layer has no DOM in it:

| module | what it owns | DOM? |
| --- | --- | --- |
| `phi_state.js` | the 6-axis loop model; `debt` = produced minus checked | no |
| `phi_friction.js` | phantom APIs, oscillation, bulk, **sycophantic reversal** | no |
| `phi_steward.js` | what to do next, ranked by what is unpaid | no |
| `phi_mediator.js` | augments outbound prompts, annotates inbound answers | no |
| `phi_presence.js` | the attention economy — when Phi may speak at all | no |
| `phi_rig.js` / `phi_fox.js` / `phi_sound.js` | one possible renderer | yes |

That split is what makes integration tractable: the CLI does not need a fox, it
needs the model. Porting only the renderer would be porting the least valuable
part.

## The integration point already exists

### Runtime evidence bridge

The CLI agent now publishes task-scoped, atomic receipts through
`src/phi/activity.rs`, driven by task start, observed tool execution, and typed
terminal outcomes. The local workspace serves their bounded projection at
`GET /api/phi/activity`, authenticated with its existing workspace session.
`phi_activity.js` renders the observed agents and selects the fox's working,
concerned, guarded, or resting pose while narration is inactive.

Activity capture follows `agent.disable_turn_artifacts`: set it to `false` in
the agent configuration to opt in. Default-off capture remains off. Receipts
live under `.selfware/phi/activity/`, one file per agent session and task; they
contain identities, timestamps, lifecycle, and bounded evidence summaries, with
commands, prompts, diagnostic messages, and source citations omitted. The API
also hides raw identities and paths. An absent, old, malformed, truncated, or
uncertain capture is identified as such. Completion describes lifecycle, not
proof of correctness; the browser never turns a completed receipt into a green
verification badge or repays its heuristic debt from it.

On Unix, receipt writes and reads use pinned directory handles so directory or
symlink replacement cannot redirect them. Other platforms report capture as
unavailable. Unexpected execution-loop errors close unfinished receipts as
failed while preserving any previously recorded typed terminal outcome.

An owned supervisor can set `SELFWARE_PHI_WORKSPACE` to an existing absolute
workspace directory to group isolated child agents, and `SELFWARE_PHI_AGENT_ID`
to a distinct label for each worker. Without these overrides, each agent uses
its current working directory and session identity. Browser monitoring remains
read-only and does not alter agent prompts, tools, or acceptance decisions.

### Activity Capture Retention and Endpoint Semantics

The activity receipt endpoint (`GET /api/phi/activity`) provides a strictly read-only, non-destructive projection of `.selfware/phi/activity`. It obeys HTTP GET idempotence:
- **Zero Read-Time Deletions**: Multiple concurrent readers or monitoring tools attaching to `/api/phi/activity` will never delete or truncate receipt files on disk.
- **Write-Time Retention Bounding**: Receipt retention is enforced exclusively during writes (`ActivityCapture::write`). Orphaned temporary files (`.tmp-*`) older than 5 minutes and expired receipts older than 24 hours are pruned. If receipt count exceeds 256 (`MAX_SCAN`), excess oldest receipts are pruned during write operations.
- **Directory Advisory Locking (`.lock`)**: An advisory lock file (`.selfware/phi/activity/.lock`) synchronizes concurrent writers and pruning passes via exclusive flocking (`flock(LOCK_EX)`). Writers wait for the lock or fail closed if lock acquisition errors; prune operations skip unlinking candidates if lock acquisition fails. The `.lock` file persists across runs and is strictly excluded from receipt scanning, parsing, and pruning.
- **Latest-Run Semantics**: If a task previously failed a check but subsequently passed on a retry or completion, the latest run outcome is preserved in the evidence snapshot, preventing historical failures from falsely categorizing current workspace health as an error.

The real-agent validation harness is:

```sh
cargo build --bin selfware
python3 scripts/validate_phi_agents.py \
  --binary target/debug/selfware \
  --output target/phi-agents-live \
  --endpoint https://llm.selfware.design/v1 \
  --model qwen38-flash-next --agents 16 --concurrency 16
```

The output directory must be new. This launches sixteen separate agent
processes on isolated repair tasks, saves their tool/session evidence, and
checks repairs against supervisor-held assertions. It measures overlapping
processes, not simultaneous provider requests. `--prepare-only` builds fixtures
without making model requests. To inspect the actual captures, launch the Phi
workspace against that output directory using its generated configuration.
The harness result and the live UI are separate checks: a passing task report
does not itself prove that the browser displayed the captures.

The hook mapping and remaining tiers below describe proposed integration work.
The implemented runtime bridge above observes evidence; it does not yet mediate
requests, generate skills, or promote harness mutations.

### Runtime-aware stewardship

Both **Ask Phi** and idle suggestions consume the same activity snapshots as
the roster. Proposals retain the captured agent, session, and task identity;
counts from separate worktrees are never summed into a workspace debt score.
Failed commands are described as recorded failures, including any recorded
passing runs, rather than proof that a test is currently failing. Stale,
partial, truncated, and missing evidence block a clean-workspace conclusion.
Dismissal suppresses a suggestion without resolving its underlying evidence.

**Inspect activity** opens that exact captured task locally. If the task is no
longer present, Phi reports that instead of opening another task from the same
agent. It never sends a different selected source file to the model as a stand-in
for an agent's missing diagnostics. Unsolicited suggestions pause while fresh
captures show running agents or narration owns the interface.

Existing file-based proposals prepare a grounded reading of their named file.
They retain the proposal when the buffer is unsaved, the target is unavailable,
or no reading job was accepted. A reading does not execute a repair or test.

Selfware's hook system fires `PreToolUse`, `PostToolUse` and `Stop` from
`src/agent/execution.rs`. Those map onto Phi's three jobs without inventing
anything:

| hook | Phi's job | event emitted |
| --- | --- | --- |
| `PreToolUse` | **mediate** — augment or gate the outbound request | — |
| `PostToolUse` | **observe** — production vs verification | `tool_call`, `diff_accepted`, `diff_accepted_unread`, `build_failed` |
| `Stop` | **steward** — Selfware is done and waiting on the user | triggers `propose()` |

`HookEvent::Stop` is exactly the moment described as "Selfware is done and in
mode waiting on user input". No new lifecycle is required.

## Three tiers, shipped in order

### Tier 1 — the loop model in Rust (`src/phi/`)

The Rust ledger and observer already record revision-aware obligations and
executed checks. The runtime evidence bridge exposes that record independently
of the browser's mood estimates. Porting friction/steward policy and enabling
mediation are separate future steps that need measured evidence first.

Where the real signals come from:

- `debt` up: a tool call writes a file and the diff is not opened.
  `src/session/edit_history.rs` already records every edit.
- `debt` down: the user opens a diff, `/undo` fires, a test is written, a test
  runs green. `src/session/edit_history.rs` and the QA gates in `src/testing/`.
- `sycophantic_reversal`: assistant turn reverses a prior stance, carries an
  agreement marker, cites no `file:line` or command output. Detectable in
  `src/agent/assistant_response.rs`.
- `phantom_api`: already detectable — `src/self_healing/` error learning sees
  unresolved-symbol errors.

This tier alone is shippable and useful with **no UI at all**: a `selfware doctor`
line, or one status row.

### Tier 2 — surfaces

The same model, three renderings, cheapest first:

- **CLI status line.** `phase` and `debt` in the prompt. `src/ui/task_display.rs`
  already owns that row. `drifting · debt 0.71` is the whole feature.
- **`Stop`-hook steward.** On idle, print the top proposal with its evidence.
  Text only. This is the product for most users.
- **Web workspace.** The fox, at `/phi/`, as it is now — for people who want an
  ambient presence rather than a line of text.
- **Zed extension.** `zed-extension/` already exists; the fox goes in a panel.

### Tier 3 — mediation

`phi_mediator` hooks the LLM call path (`src/llm/`). Augmentations are appended
to the outgoing request, labelled, and shown in `--verbose`. The gate becomes a
confirm prompt, with an override, exactly as in the browser.

This tier changes what the model receives, so it ships last and behind a flag
(`--phi-mediate`) until its effect on task success is measured.

## What must not be ported

The presence rules are not decoration; they are the reason this is usable:

- Posture (or one status token) carries almost everything. Speaking is rationed.
- A signal must persist before it speaks, and **ignoring it makes Phi quieter**.
- Interrupts are budgeted per session; a useful one is refunded.
- Dismissal is permanent for the session.
- Being asked is free — `selfware phi` should always answer in full.

A CLI port that prints every finding immediately is Clippy with a fox, and will
be turned off on the first day. `phi_presence.js` is the spec.

## Open items

1. **Attention is inferred, not measured.** `debt` is estimated from events.
   `src/session/edit_history.rs` plus the IDE document endpoints know what was
   actually opened. Measuring it is the single biggest accuracy win available.
2. **One drawing, two homes.** `design/mascot/` and `src/evolve/web/phi/` now
   share the mood vocabulary and the geometry, enforced by
   `scripts/tests/test_phi_expression.py`. Keep that test green or they fork again.
3. **Thresholds are unvalidated.** `debt > .45` / `> .7` and the persistence
   counts are chosen, not measured. They need tuning against real sessions
   before the CLI surfaces them by default.
