# Phi integration completion audit

Requested outcome: “wire this deeply into selfware
`/Users/ivo/selfware/design/mascot/` validate it works with our
`llm.selfware.design` endpoint with 16 agents”.

The referenced mascot README identifies that directory as the authoring studio
and `src/evolve/web/phi/` as the shipped assistant. Its command simulator remains
explicitly illustrative. The production integration connects the shared fox to
real task observations, evidence-based suggestions, and grounded source reading.

| Requirement | Authoritative evidence | Result |
| --- | --- | --- |
| Use the studio character and expression vocabulary in the shipped assistant | Shared geometry and expression modules; 84 state/expression/steward tests; rendered desktop/mobile screenshots | Verified |
| Connect real Selfware execution and verification evidence | Rust activity writer, tool observer, task-outcome/fallback wiring, authenticated activity endpoint; 16 real runtime receipts | Verified |
| Preserve failure, uncertainty, freshness, and task identity | Writer/observer tests, activity contracts, exact agent/session/task inspection tests; live incomplete and stale capture displays | Verified |
| Make the existing steward use those observations | Both Ask Phi and idle paths consume activity; actual running probe returned no suggestion or speech interruption; actual terminal Ask Phi produced evidence-linked inspection proposals | Verified |
| Keep actions truthful and directed at their target | Inspection makes no model request; file proposals preserve unsaved buffers and await accepted jobs; one live proposal selected file A while file B was open and posted only A | Verified |
| Make expression sounds available without unsolicited audio | Separate default-off Sounds control, independent narration state, browser tests and live explicit-toggle evidence | Verified for control wiring; no audio-quality claim |
| Validate the configured endpoint with 16 agents | `target/phi-agents-live-20260913-steward/report.json`: 16/16 independent task checks, 16 distinct valid captures, measured peak of 16 overlapping processes | Verified |
| Validate grounded model output | One targeted reading completed; source excerpts matched the saved named file; trust remained structural and explicitly did not claim code tests | Verified |
| Keep suggestion actions usable on desktop and mobile | Bounded scrollable proposal stack; inspection clears the overlay without dismissing other suggestions; actual mascot overlap, button hit tests and polling-focus checks at desktop and 390px mobile | Verified in the final live UI recheck |
| Meet repository checks and retain reviewable work | Final build, formatting, strict all-target Clippy; 84 state/expression/steward tests, 24 activity tests and six workspace browser regressions; focused Rust checks; no commits, original failure reports retained | Verified |

Evidence lives in `target/phi-validation-20260913/integrated/`; the chronology,
exact binary hashes, first-run failures and corrected validator contract are
documented in `docs/quant_bench/phi-agent-validation.md`.

The sixteen-agent execution proof uses binary `379329a9…`. Subsequent UI-only
patches change `app.js` and `style.css`; `ui-patch-scope.json` records those changes
and that no Rust or Cargo inputs were modified after that run's build receipt.
Their verification reuses the actual captures, preserves their current stale
status, and makes no replacement agent or model requests. Running-state and
targeted-reading proof remain attributed to their original browser/run.

Final UI binary: `19c5a83eeca9379c702ffc308d66f92db3d3c69e9cbf15d085340e712da439e5`.
`overlay-final/live-overlay-evidence.json` records the passing live interaction
checks and served asset hashes, matching `build-receipt-ui-final2.json` and the
current files. Inspection made zero model requests. The earlier mascot-layer
occlusion was preserved as a failed attempt, fixed, and rechecked with the real
mascot and speech bubble visible. The inspected task is visible and its full
identity remains focused across the next real activity poll.

This implements the mascot's observation, inspection, and grounded-reading
connection. The survey-inspired automatic skill-distillation, compiled-tool,
and autonomous harness-promotion designs remain separate roadmap work; these
artifacts do not assert that Selfware has a validated recursive learner.
