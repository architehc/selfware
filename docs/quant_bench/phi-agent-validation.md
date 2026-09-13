# Phi agent validation

The latest run includes the runtime-aware steward, exact-task inspection,
correctly targeted reading proposals, and the opt-in expression-sound control.
Its report is `target/phi-agents-live-20260913-steward/report.json`; earlier
experiments below are retained for provenance.

`scripts/validate_phi_agents.py` runs 16 actual `selfware run` processes in
separate disposable Git fixtures. Each has a failing Python implementation,
an immutable test contract, a unique task, and an independent supervisor check.
The fixtures cover clamping, sums of squares, affine transforms, and inclusive
divisibility counts. They test bounded tool/edit/test integration, not general
coding ability or visual design quality.

Build a binary containing the Phi runtime producer first, then run:

```sh
python3 scripts/validate_phi_agents.py \
  --binary target/release/selfware \
  --output artifacts/phi-agents-20260913 \
  --agents 16 --concurrency 16
```

The output directory must not already exist. `--prepare-only` creates fixtures
and a manifest without running agents or contacting the endpoint; use a different
output directory for the subsequent live run. `--agents 1 --concurrency 1` is a
smaller preflight. The default endpoint is `https://llm.selfware.design/v1`, model
`qwen38-flash-next`. Public access uses the explicit dummy key `none`; inherited
API credentials are not forwarded. Do not use this script for a private endpoint
that needs authentication without adding an explicit credential contract.

The generated config caps context at 32,768 tokens, output at 2,048, and each
agent at 10 turns, 120,000 cumulative tokens, and 600 seconds. It explicitly turns
thinking off for these small tasks and records Phi artifacts. These are test
settings, not model recommendations. The supervisor terminates a worker process
group if it exceeds the wall limit plus 30 seconds of shutdown allowance.

Every worker receives `SELFWARE_PHI_WORKSPACE=<output>` and a distinct
`SELFWARE_PHI_AGENT_ID`; runtime snapshots therefore collect under
`<output>/.selfware/phi/activity/`. Point the Phi service at this output workspace
to inspect the actual activity. The CLI's final `session_id` is a checkpoint task
ID and must match snapshot `task_id`; snapshot `session_id` identifies its audit
session separately. The harness verifies both sets of identities are distinct.

`report.json` succeeds only when every requested worker completes, edits its
source, preserves the tests, passes the original independent assertions, emits
successful tool evidence, and produces a fresh completed schema-v1 runtime
snapshot bound to that worker and task. A zero process exit, empty response,
partial run, or missing telemetry cannot count as success. Outstanding evidence
debt is retained, and a task passing does not claim that the ledger debt is zero.

Artifacts include `manifest.json` (binary/config hashes and fixture contracts),
`process-events.json` (PIDs and observed process overlap), `progress.json`, and
per-agent stdout, stderr, independent-test output, final runtime snapshot, and
classification under `logs/`. The binary is hashed again after the run. Peak
overlapping processes is measured; overlapping provider requests is not measured.
`multi-chat -n 16` would only change a concurrency limit over four default role
agents and makes one completion per role without tool execution, so it does not
provide this test's agentic coverage.

Offline checks:

```sh
python3 scripts/tests/test_validate_phi_agents.py -v
```

These check classification failures and all sixteen broken/fixed fixture
contracts. They do not establish live endpoint behavior.

## Integrated steward run

The frozen binary with SHA-256
`379329a99f85def002ffece63e3644ae9b18ec95ce49cd68974e5ceada3c7885`
passed **16/16** independent repair checks through `llm.selfware.design` using
`qwen38-flash-next`. It produced 16 distinct task and audit-session identities,
16 valid terminal activity records, and measured peak overlap of 16 actual
processes. The binary hash stayed unchanged. Recorded usage was 758,697 tokens;
individual sessions lasted 65.510–150.786 seconds.

The final build, formatting, and strict all-target Clippy checks passed.
The updated state/expression/steward suite passed 84 tests, activity integration
passed 21 tests (including 14 Chromium fixture cases), and six existing workspace
browser regressions passed. Identity tests retain the earlier assertions and
add the session/task fields required by the stronger exact-task contract.

Artifacts for this version are in `target/phi-validation-20260913/integrated/`:
`build-receipt.json`, `agent-run-summary.json`, and `auth-live-evidence.json`.
The live service rejects missing or incorrect session tokens with HTTP 401.

## Recorded live validation — 2026-09-13

The first endpoint run used 16 isolated agent processes and reached a measured
peak of 16 overlapping processes. The original report is retained at
`target/phi-agents-live-20260913/report.json`. Fourteen agents completed their
repairs; two exhausted their token budgets. The initial validator incorrectly
rejected the CLI's `REAL_EDIT` success diagnostic and the file writer's default
backup. The corrected contract recognizes that documented success variant and
permits only a regular `subject.py.bak` containing the original source bytes.
Mismatching backups, unrelated files, changed tests, and incomplete runs remain
failures. Reassessment yields 14/16; it does not change the original report.

The real browser rendered all 16 captures from that first run: 14 completed and
2 partial, with incomplete evidence on 15 captures. Desktop and 390px mobile
checks passed with no horizontal overflow, page exceptions, or console errors.
Phi retained its guarded pose instead of treating completed tasks as verified.
One live Prepare request completed against `llm.selfware.design`: two claims,
one recommendation, and three citation buttons. Its source excerpt was checked
against disk; trust remained `structural`, and the UI explicitly stated that no
code tests ran for the reading. No model responses or activity rows were mocked.

Browser evidence and screenshots are in `target/phi-validation-20260913/`,
including `browser-live-evidence.json` and `live-reading-result.png`. These
verify the UI and reading flow separately from agent task success. The frozen
binary and its build receipt are retained alongside them.

The second run, `target/phi-agents-live-20260913-r2/report.json`, passed **16/16**
under the corrected validator with a measured peak of 16 processes and 16 valid,
distinct runtime captures. It used the documented 120,000-token/600-second caps
and explicit tool-schema/checklist instructions. All original independent tests
passed, and the binary hash stayed unchanged. Recorded agent usage totalled
767,020 tokens; individual session durations were 85.113–129.217 seconds. This
run predates the subsequent directory-race and exceptional-exit hardening.

### Final live browser and reading verification

The hardened frozen binary served the final workspace on local port 7790.
The real authenticated activity API and shipped browser rendered all 16 final
agent captures as completed. Fourteen captures still reported incomplete
execution evidence; the remaining two were available. These distinctions and
the captured run/coverage counts remained visible, and Phi stayed guarded.
Task completion was not presented as comprehensive verification.

Desktop (1440 × 1080) and mobile (390 × 844) checks passed, including the first
and last roster entries, with no horizontal overflow, page exceptions, or
console errors. No activity rows or model responses were mocked. Screenshots
were visually inspected after capture.

Exactly one additional live Prepare request was submitted for
`agents/phi-agent-01/subject.py`; job
`f4c8f4d7-d08d-492d-8357-0e305b9146a7` completed without a replacement request.
It produced three claims, one recommendation, and four citation buttons.
Its single source excerpt was validated against the saved file, and a citation
was opened in the editor. Trust remained `structural`, with the explicit label
“no code tests ran for this reading.” This validates source-reference binding
and the UI flow, not semantic correctness of every model explanation.

Final browser evidence is preserved separately in
`target/phi-validation-20260913/final/browser-live-evidence.json`, with
`live-desktop-agents-first.png`, `live-desktop-agents-last.png`,
`live-mobile-agents-first.png`, `live-mobile-agents-last.png`, and
`live-reading-result.png`. Earlier run artifacts remain unchanged.

The final hardened binary also passed **16/16**, recorded at
`target/phi-agents-live-20260913-final/report.json`. All 16 task IDs and audit
session IDs were distinct, all 16 runtime snapshots were valid, and measured
peak process overlap was 16. Original assertions, fixture scope, and unchanged
Git heads passed for every worker. Recorded usage totalled 765,124 tokens;
individual sessions took 78.528–144.714 seconds. The frozen binary SHA-256 is
`0425ffff7de326289508727a9033701baebe127c2293c61a5ee50cd26db78ebf`, unchanged
throughout the run; source hashes are in `build-receipt-hardened.json`.

The final build, `cargo fmt --check`, and
`cargo clippy --all-targets -- -D warnings` passed. Focused Rust checks passed
7 receipt-writer tests and 5 observer/lifecycle tests; prior backend checks
passed 6 tests. The validator's 13 offline tests also passed. Individual observed
check outcomes are summarized in `target/phi-validation-20260913/checks.json`.
The final service rejected both absent and incorrect activity-session tokens
with HTTP 401; see `final/auth-live-evidence.json` in the validation directory.

### Integrated steward, local inspection, and targeted reading

The integrated build on port 7791 was observed during the real sixteen-worker
run at `target/phi-agents-live-20260913-steward`. While agents were running,
the idle-steward probe returned no suggestion and made no speech interruption.
That running capture and screenshot were retained before waiting for the same
run to finish. Terminal browser inspection rendered all sixteen completed
captures, preserving thirteen incomplete evidence states and three available
states. The actual **Ask Phi** response produced four evidence-linked activity
inspection proposals and no suggestion that the workspace was clear for new
work. Inspection selected the exact agent/session/task tuple, retained focus
across the next real poll, and made no model request.

Expression sounds were initially off; explicit on/off clicks used the existing
sound engine without changing narration, source, or making a model request.
This checks the control and its connection, not audible sound quality. Desktop
and 390px mobile geometry had no horizontal overflow, with no page exceptions
or console errors. Visual review also identified that remaining suggestion
cards could obscure the inspected roster on mobile; this observation is
separate from the successful identity/focus checks.

The targeted-reading test used an explicitly constructed proposal card as test
input, not a claimed model-generated insight. With file B initially selected,
its **Prepare reading** action named the real file A,
`agents/phi-agent-01/subject.py`. Exactly one request was posted for A; job
`0f08eda7-5b98-426c-94ad-99f21ac025dc` completed with two claims and a
recommendation. The saved source excerpt was checked against A, and the
displayed trust remained `structural`, explicitly stating that no code tests
ran for this reading. Neither activity nor model responses were mocked.

Evidence is under `target/phi-validation-20260913/integrated/`, including
`integrated-browser-evidence.json`, `actual-running-agents.png`, terminal
desktop/mobile screenshots, and `targeted-reading-result.png`. An initial
test-harness locator matched four proposals sharing a title and stopped before
any reading request. Its evidence is retained separately as
`integrated-browser-first-attempt.json`. The locator was corrected to the
proposal's identity and terminal checks resumed in a new browser against the
same agent run; the running-phase evidence belongs to the first browser. No
agent rerun or replacement reading request was used to obtain this proof.

### Final proposal overlay verification

The mobile overlay findings were fixed and checked against the same sixteen
real captures, without rerunning agents or submitting any model request.
Successful inspection now hides the proposal stack while retaining the other
proposal IDs for the next **Ask Phi**. The stack has a bounded scrolling
viewport and renders above the mascot, speech bubble, and laser. A first live
attempt exposed that last stacking issue; its failed evidence and screenshot
remain preserved as `overlay-final/live-overlay-first-attempt.json` and
`overlay-final/first-attempt-mascot-occlusion.png`.

The final UI binary is `selfware-ui-final2`, SHA-256
`19c5a83eeca9379c702ffc308d66f92db3d3c69e9cbf15d085340e712da439e5`.
It served the same workspace on port 7791. The final check passed at
1440 × 1080 and 390 × 844: all sixteen captures remained visible, actual
**Ask Phi** proposals retained the agent/session/task identity, first and last
Inspect buttons were reachable, the inspected row was not covered by the
proposal stack, focus survived the next real poll, and other proposals returned
on the next Ask. By this check the captures were stale; Phi correctly proposed
historical inspection and stated that current task status was unknown. The
check made zero model requests and recorded no page exceptions, console errors,
or horizontal overflow. All four final screenshots were visually inspected.

Final evidence, including served asset hashes, is
`target/phi-validation-20260913/integrated/overlay-final/live-overlay-evidence.json`.
That directory also contains `desktop-proposals.png`, `mobile-proposals.png`,
`desktop-inspected-roster.png`, and `mobile-inspected-roster.png`. All 24 focused
activity tests passed, including real mascot/bubble overlap during Ask Phi and
scrolling through four long proposals. Earlier running-agent and targeted
reading proofs belong to the preceding integrated asset version; this final
UI-only proof does not claim a new agent run or reading.
