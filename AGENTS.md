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

## 6. Window placement: stay inside the visible region, touch only your own windows

When arranging desktop windows with `wmctrl`/`xdotool`:

- Only move windows your session owns (e.g. `sw-*` study terminals). Never
  reposition other windows (chat terminals, Firefox, System Monitor) — the
  user arranges those.
- The X screen is 7680x2928 (GNOME X11, 200% scale). The visible area of the
  main display is device coords **x 0–7680, y 768–2928** (the small VGA
  monitor sits above it at x 3840–4864, y 0–768).
- mutter doubles `wmctrl -e` position requests: request = target/2; sizes pass
  through 1:1. So a window meant for device (1920, 800) is requested as
  `(960, 400)`.
- Sanity-check after every placement with `wmctrl -lG`: every window must land
  at y >= 768 and x+width <= 7680 in the listing. If not, fix it immediately.

Failure mode it prevents: tiling loops requested coordinates like (0,0) or
(3840,1065), which doubled to device y=0 (above the visible top edge) or
x=7680 (past the right edge), repeatedly pushing study terminals and other
people's windows off-screen.
