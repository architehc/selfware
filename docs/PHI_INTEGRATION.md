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

Port `phi_state`, `phi_friction` and `phi_steward` to Rust and drive them from
the hook bus. The events originate in the agent loop, not the browser; today the
browser *estimates* them, which is the weakest part of the current build.

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
