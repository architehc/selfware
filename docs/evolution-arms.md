# Multi-arm evolution in quarantine

`selfware evolve --workflow arms` compares several candidate edits ("arms")
with the unmodified code (the baseline). Each arm is built, tested and
benchmarked in its own quarantined snapshot. The command only produces a
report: nothing is applied to your repository. The default `selfware evolve`
workflow is unchanged, and none of this runs unless you ask for it.

```sh
selfware evolve --workflow arms \
  --arms-file arms.json \
  --bench-cmd "cargo run --release --quiet --example bench" \
  --consensus 2 --replicates 3 --parallel 2
```

`arms.json` is a list of full-file replacements:

```json
[
  {"arm_id": "unroll", "name": "Loop unrolling",
   "target_file": "src/algo.rs", "proposed_source": "…the whole new file…"}
]
```

(`proposed_patch` is accepted as an alias of `proposed_source`.)

## What happens to each arm

1. The target has to be a project-relative `.rs` path that is not protected
   (`evolution::PROTECTED_PATHS`: `src/safety/`, `src/evolve/`, `tests/`,
   `Cargo.toml`, …). Otherwise the arm is rejected before anything runs.
2. The source is syntax-checked. A syntax error is reported with its line
   and column when they can be located.
3. The arm gets a snapshot of `HEAD`, with the file replaced.
   `cargo check --all-targets --message-format=json` runs, and rustc's
   `MachineApplicable` suggestions are applied for at most 2 repair rounds.
   The number of rounds and fixes is recorded, and the resulting diff is
   kept in the report.
4. `cargo test --no-run` decides whether the arm compiled, and `cargo test`
   counts passes and failures.
5. The benchmark command runs once as a warm-up, then `--replicates` times.
   The wall time of each measured run is one sample. Benchmarks run one at
   a time across all arms and the baseline, so parallel builds do not skew
   the timings.

The baseline goes through steps 3–5 (without any edit or repair) in its own
quarantine, so every comparison is between numbers measured the same way.

## Verdict

An arm **passes** when all of these hold:

- it compiled;
- it has **0 test regressions** against the baseline: no more failures and
  no fewer passing tests;
- its median sample is below the baseline median;
- **every** one of its samples is faster than **every** baseline sample, so
  a better median inside the noise does not count.

The fitness delta is `(baseline median − arm median) / baseline median`.
The number of tests does not enter the score. If you give no `--bench-cmd`,
no fitness is measured and no arm can pass; the report says so.

**Consensus** needs at least `--consensus` passing arms. The winner is the
passing arm with the largest delta. This follows `formal/EvolutionBounds.lean`
E4 (`consensus_requires_passing_arms`), and a conformance test pins it.

The report is printed as a PR description and saved as JSON under
`.selfware/evolve-arms/`.

## The quarantine

Building and testing model-written code runs arbitrary code: `build.rs`,
proc macros and tests all execute. Every arm process runs with:

| Aspect | Setting |
| --- | --- |
| Source | `git archive` snapshot of the base commit. It has no `.git` and is **not** a linked worktree, because a linked worktree shares `.git/config` and hooks with your repository. |
| HOME, XDG dirs, TMPDIR | private, inside the arm |
| CARGO_HOME | private. `config.toml` is copied without registry tokens, credential providers, `[env]` or `build.rustc-wrapper`. `credentials.toml` is never copied. `registry/` and `git/` are copy-on-write clones (APFS `cp -c`, btrfs/xfs reflink); where cloning is not possible they start empty and cargo downloads. |
| Toolchain | Called directly from a sysroot `bin/`, cloned once per run where possible. The arm gets a private, empty `RUSTUP_HOME`, so no rustup proxy runs and nothing installs into `~/.rustup`. |
| CARGO_TARGET_DIR | Inside the arm, seeded from the baseline build where it can be cloned. |
| PATH | Toolchain `bin` plus `/usr/bin:/bin:/usr/sbin:/sbin`. Nothing else from your PATH. |
| Environment | The shared sanitized allowlist (`safety::process_env`), so no API keys or tokens. |
| git | `GIT_CONFIG_*` overrides turn off fsmonitor, hooks, pager and external diff. `GIT_ATTR_SOURCE` points at the empty tree, so `.gitattributes` filters and textconv do not run (git 2.40+). System and global config are off. `GIT_CEILING_DIRECTORIES` is set at the arm root. |
| Processes | Own process group. The whole group is SIGKILLed on timeout, cancel, or descendants left running. |

Each report includes this as `isolation`.

### What is NOT isolated

- **Filesystem and network (default):** the arm runs as your user. Code that
  writes to an absolute path outside the arm (for example your home
  directory, found through `/etc/passwd`) or opens a socket can still do so.
  The quarantine moves the *default* locations (HOME, cargo and rustup
  homes, git config, secrets in the environment) away from the arm. It is
  not a boundary against code written to escape it. A test pins this
  limitation so the documentation stays true.
- A process that calls `setsid()` leaves the process group and survives the
  kill.
- Arms of one run share a parent directory, so one arm can write into
  another's directory.
- Where copy-on-write cloning is not available, arm code can modify the
  host sysroot. The report records this.
- git: per-driver keys selected from another repository's own
  `.git/info/attributes` are not neutralised. git older than 2.40 ignores
  `GIT_ATTR_SOURCE`.

### Opt-in sandbox (`--arm-sandbox`, macOS)

Every arm process runs under `sandbox-exec`, with a profile that:

- denies writes outside the arm (except `/dev` nodes);
- denies reads of your home directory;
- denies all network access. cargo runs offline, so dependencies must be in
  the cloned registry.

Linux has no equivalent in selfware yet (`bwrap` would be the route).
Asking for the sandbox there is an error, not a silent no-op.
