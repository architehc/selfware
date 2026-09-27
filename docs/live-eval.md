# Live evaluation harness (`scripts/live_eval`)

`scripts/live_eval/run.py` runs selfware against a real endpoint
(llm.selfware.design, model `qwen38-flash-next`) over a fixed set of
scenarios, records one JSONL line per run, and reports with statistics
whether a commit made selfware better or worse. It is the empirical layer
of the validation design: golden scenarios, N-run statistics, planted-bug
fixtures, intervention counts, release gating. Python stdlib only.

The per-commit CI stays hermetic. Live runs happen in three places: the
nightly `live-eval` job (`.github/workflows/live-endpoint.yml`, quick
scenarios whose fixtures are in the repo), `run.py gate` before a release,
and the long-running `run.py loop` on a developer machine.

## Scenarios

| name | what it measures | pass criteria (all must hold) |
|---|---|---|
| `review-planted` | review of a 16-file Python fixture with 8 planted bugs (hidden `answer_key.json`, never copied into the workspace) | completed, recall >= 0.5, <= 3 false findings, >= 3 verified citations, 0 wrong, coverage complete, no edits |
| `review-slugify` | review of python-slugify (clean clone of `$LIVE_EVAL_SLUGIFY_REPO`) | completed, coverage reported and >= 80 %, >= 3 verified citations, 0 wrong, no edits |
| `edit-tests` | add `max_words` + a test to slugify and run pytest | completed, >= 2 files changed, the harness's own pytest run passes, a behaviour probe passes, a test mentions `max_words` |
| `c24` | the 24k-window multi-step documentation task (`$LIVE_EVAL_C24_WS`, `cfg.toml`, `compact.prompt`) | completed, CONTEXT_NOTES.md bullet count equals the measured `pub fn` count, `context.rs` diff adds only `///` lines, every `pub fn` documented, 0 wrong citations |
| `qa-greeting` | "hi" | completed, no tool calls, <= 2 turns (0 = answered in the planning turn), short answer, and a text-mode twin run shows no `NO_CHANGES` noise |
| `interrupt` | SIGINT to the process group 4 s into a streaming answer | exit 130, outcome `interrupted`, exit within 30 s, nothing left in `selfware resources --json` |
| `review-core-long` | "can you review the selfware core do not code" on a snapshot of the binary's own commit, 4 h cap | completed, coverage reported and >= 50 %, 0 wrong citations, no edits; reports files/hour |

Every run: isolated `HOME` (no response cache, checkpoint or global config
carries over), an explicit `-c <cfg>` copied from the tracked
`selfware-llm-selfware-design.toml` at the binary's commit (c24 uses its own
24k config), `--output-format stream-json`, `--max-wall-secs`. The record
holds the commit, `selfware --version`, endpoint, model, config sha256,
every metric and one boolean per criterion. Interventions are counted from
the stream's `turn_decision` events: nudges (`nudge_injected`), refusals
(`refused`, `rejected_tools`, `stopped_before_dispatch`, `retry_suppressed`),
gate blocks (citation correction rounds, `cap_completion_gate`,
`*_accept_draft`); `no_tool_call` is counted apart because every plain
final answer emits it.

An unreachable endpoint, a fixture that cannot be set up, a harness timeout
or a missing result object is a FAIL record with a reason, never a skip
(AGENTS.md rule 3). A run the loop stops on shutdown is `abandoned`: counted,
never a pass.

## Running

```sh
export LIVE_EVAL_RESULTS_DIR=~/selfware-live-eval   # never inside the repo
export LIVE_EVAL_FIXTURES=/path/to/fixtures         # holds ux092/projects/slugify, c24_int3/
# or point at each fixture: LIVE_EVAL_SLUGIFY_REPO, LIVE_EVAL_C24_WS, LIVE_EVAL_C24_CFG, LIVE_EVAL_C24_PROMPT

scripts/live_eval/run.py list
scripts/live_eval/run.py run --binary target/release/selfware --scenarios review-planted --samples 3
scripts/live_eval/run.py run --source . --rev HEAD --target-dir /path/to/target   # build, then run all quick
scripts/live_eval/run.py report                      # stats + comparison vs the previous commit
scripts/live_eval/run.py gate --rev v0.9.4-rc1 --samples 3 --target-dir /path/to/target
```

Builds never touch the source worktree: a `git clone --shared` at
`$LIVE_EVAL_RESULTS_DIR/build-src` checks out the wanted commit and
`cargo build --profile release-fast` runs there with the given target dir;
binaries are kept as `bin/selfware-<sha12>` (last 3).

## Report and gate

`report` groups runs by scenario and commit. Per scenario: pass rate with a
95 % Wilson interval, outages, abandoned runs, p50/p90 wall, p50 tokens and
turns, coverage, planted-bug recall, false findings, wrong/verified
citations, interventions per turn, files/hour, and which criteria failed.
It then compares the latest commit with the one before it using
`scripts/live_eval/thresholds.json` (every threshold carries its
rationale). A change beyond a threshold is `REGRESSION` when both sides
have >= 3 runs (pass rate: also one-sided Fisher exact p < 0.1), otherwise
`WATCH`. Pass rates are compared over endpoint-reachable runs; outage runs
stay FAIL in the raw pass rate and mark the commit `UNCERTIFIED`.

`gate` builds `--rev`, runs the quick scenarios `--samples` times, and
exits 1 on any REGRESSION, any scenario below its absolute floor
(`floor_pass_rate`), or any outage/setup failure in its own runs.

## Long-running loop

```sh
nohup scripts/live_eval/run.py loop \
    --source /path/to/integration-worktree --target-dir /path/to/target \
    > "$LIVE_EVAL_RESULTS_DIR/loop.out" 2>&1 &

scripts/live_eval/run.py status          # heartbeat + liveness
tail -f "$LIVE_EVAL_RESULTS_DIR/loop.log"
scripts/live_eval/run.py report
scripts/live_eval/run.py stop --finish   # start nothing new, exit after current runs
scripts/live_eval/run.py stop            # SIGTERM: current runs recorded as abandoned
```

The loop rotates the quick scenarios round-robin with at most two runs at
once (the endpoint serves 8 slots; the rest stay usable), starts
`review-core-long` at most every `--long-every-hours` (default 6) and never
two at once, checks the source HEAD every `--poll-secs` (default 300) and
rebuilds in the background when it moves (a failed build keeps the previous
binary and is shown in the heartbeat). On an endpoint outage it records the
FAIL and backs off exponentially (60 s doubling to 30 min). It writes
`heartbeat.json` every 5 s, keeps the newest `--keep-runs` (300) run
artifact dirs under `--max-artifact-mb` (2048), caps each stream at 1 MB
(head + tail, gzipped), and deletes the agents' shared cargo target dir
past `--child-target-gb` (20). `results.jsonl` is never rotated.

### launchd (macOS) example — not installed

Save as `~/Library/LaunchAgents/design.selfware.live-eval.plist`, adjust the
paths, then `launchctl load` it. `KeepAlive` restarts the loop if it exits;
`launchctl unload` sends SIGTERM (current runs are recorded as abandoned).

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>design.selfware.live-eval</string>
  <key>ProgramArguments</key>
  <array>
    <string>/usr/bin/python3</string>
    <string>/Users/you/selfware/scripts/live_eval/run.py</string>
    <string>loop</string>
    <string>--source</string><string>/Users/you/selfware-integration</string>
    <string>--target-dir</string><string>/Users/you/selfware-live-eval/target</string>
  </array>
  <key>EnvironmentVariables</key>
  <dict>
    <key>LIVE_EVAL_RESULTS_DIR</key><string>/Users/you/selfware-live-eval</string>
    <key>LIVE_EVAL_FIXTURES</key><string>/Users/you/selfware-fixtures</string>
    <key>PATH</key><string>/Users/you/.cargo/bin:/usr/local/bin:/usr/bin:/bin</string>
  </dict>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>ExitTimeOutSecs</key><integer>120</integer>
  <key>StandardOutPath</key><string>/Users/you/selfware-live-eval/loop.out</string>
  <key>StandardErrorPath</key><string>/Users/you/selfware-live-eval/loop.out</string>
</dict>
</plist>
```
