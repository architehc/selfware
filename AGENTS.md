# AGENTS.md — selfware working agreements

These rules bind any agent working in this repository. They exist because of
specific observed failures; each rule names its failure mode.

## 5. Sweep the bug class, not the file

When a review finding is fixed, the fix commit is not done until the same
mistake has been grepped for everywhere it could recur. Pattern: a finding in
`cargo.rs` means check `git.rs`/`package.rs`/every other Command spawn; a
hardened regex means check its sibling patterns; a trust gate means check
every path that reaches the model, not just the one the review named.
Failure mode it prevents: credential scrubbing landed in cargo.rs/git.rs but
not package.rs; protected-path enforcement landed in the daemon but not in
apply.

## 1. Stop-the-line: CI red means stop

`cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` must be green
before every commit. A `.git/hooks/pre-commit` gate enforces this locally (also
mirrored in `.pre-commit-config.yaml` for pre-commit-framework users).

Failure mode it prevents: main sat red on a fmt check while docs commits kept
stacking on top (fixed in d9c5874d). Red CI is a stop signal, never background
noise.

## 2. Review-gate: sign-off for subtractions

Any change that (a) deletes a user-selected goal, preset, or feature, or
(b) weakens a test assertion (removed assertions, lowered thresholds, dropped
cases) requires explicit human sign-off **called out in the commit message or
the change review** — not bundled silently inside a larger change.

Mechanical necessities are fine (a deleted module forces its test to change),
but the edit must be justified in the report/commit, not hidden.

## 3. Honest status over optimistic success

Structured outcomes only: typed errors over untyped 500s, telemetry retained on
failure, and success badges must name what was actually verified (see
`trust_state` in the grounded review path: `structural` means schema+citations,
never more). Never render green for checks that were not performed.

## 4. Measured, not estimated

Token accounting goes through `crate::token_count::estimate_content_tokens`;
context sizing uses measured projections (`evolve::context_fit::TierMeasurer`,
`evolve::envelope`), not fractional heuristics. The old 0.18 signature fraction
overstated Lite by 42% — heuristics survive only as per-node fallbacks.
