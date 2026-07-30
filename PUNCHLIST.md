# PUNCHLIST — open items register

> Living register of verified-open issues. Each entry: severity, status, owner
> note. Process rules (from the closing review, adopted):
> - Every invariant-restoring fix ships with a regression test.
> - Every fix sweeps the bug class (grep all call sites) — AGENTS.md rule 5.

## 🔴 Do first

| Item | Status | Note |
|---|---|---|
| `/api/workspace` hands out the session token unauthenticated | [open — design decision needed] | Localhost-trust model (Host guard added 2026-07-26). Real fix is OS-level identity: unix socket + peer-credentials, or 0600 token file + bootstrap. Browser UI constraint. |
| computer/ spawns (36) unsanitized — child processes inherit SELFWARE_API_KEY etc. | [open — in progress] | Must preserve DISPLAY/WAYLAND_DISPLAY/XDG_RUNTIME_DIR for xdotool/osascript. |

## 🟠 Sibling misses / regressions

| Item | Status | Note |
|---|---|---|
| browser.rs redirect-SSRF (pinned first hop only) | [open] | Pin redirect hops or disable redirects; DNS pin per hop. |
| trust-gate coverage: RAG index, CLAUDE.md/AGENTS.md loading, memory/lessons paths | [open] | Loop gate covers tool results; these ingest paths predate it. Lessons are sanitized at injection (2026-07-26) but not scanned. |
| vision/screen_capture URL validation against PinnedDnsResolver | [open] | Egress classification fixed; endpoint itself still unverified. |

## 🟡 Minor / hardening

| Item | Status | Note |
|---|---|---|
| No-default-features doc: what silently vanishes without `self-improvement` | [open] | Document in configuration.md. |
| `edit_history` unwired API (blanket) | [open] | Tested, partially wired — wire timeline UI or prune. |
| Daemon quality: verification asymmetry, token-budget-only scoring | [open] | Feature-gated daemon items. |
| qwen3-vl-embedding / gemma-4-12b availability | [blocked — external] | Probes in docs/superpowers/plans/2026-07-25-visual-embeddings.md. |
| keyring-rs#341 (macOS set/get inconsistency) | [blocked — upstream] | Filed 2026-07-26. |
| truncate helpers ×7 (different semantics) | [open] | Deliberately kept separate; named-helper standardization. |
| Benchmark config dup (Cluster 10) | [open] | Field-set comparison needed. |

## ✅ Closed this arc (for the record)

Credential sweep (cargo/git/package/git_worktree/browser/cli), trust gate (review path + agent loop), apply isolation + protected paths + symlinks, /undo + /restore integrity, exit-code cause chains, expand Auxiliary reachability, redaction unification + re-match flaw, telemetry pipeline, staged banner, expansion API + validator + 580-example catalog, trust_state honesty everywhere, dedup waves (66→31 pairs), debloat (953k Full < 1M), 29-module recommendation library.
