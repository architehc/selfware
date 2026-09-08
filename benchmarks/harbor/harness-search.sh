#!/bin/bash
# harness-search.sh — Meta-Harness-style outer loop over selfware harness
# profiles on Terminal-Bench 3.0 (arXiv 2603.28052; review in
# docs/2026-08-24-meta-harness-review.md).
#
# Loop per iteration:
#   1. proposer (selfware run, GLM-5.3) reads the archive: raw traces + scores
#      of ALL prior candidates, writes proposal.diff + proposal.md
#   2. apply the diff to the parent profile -> new candidate config
#   3. evaluate on the fixed search slice via Harbor (docker env)
#   4. append the candidate record to candidates.jsonl (scores, cost, traces)
#
# Usage: benchmarks/harbor/harness-search.sh <iterations>
# Env:   SELFWARE_API_KEY (OpenRouter), HARNESS_ARCHIVE (default
#        /home/rig/harbor-agents/archive), SEARCH_TASKS (space-separated
#        terminal-bench/ names), HARBOR_BIN (default harbor on PATH).
set -euo pipefail

ITERATIONS="${1:-5}"
ARCHIVE="${HARNESS_ARCHIVE:-/home/rig/harbor-agents/archive}"
SEARCH_TASKS="${SEARCH_TASKS:-terminal-bench/cargo-flight-dispatch terminal-bench/bun-sourcemap-leak terminal-bench/cli-2ph-simplex terminal-bench/data-anonymization terminal-bench/html-js-filter terminal-bench/distributed-dedup terminal-bench/embedding-drift-monitor terminal-bench/cumulative-layout-shift}"
SEED_CONFIG="${SEED_CONFIG:-/home/rig/selfware/benchmarks/harbor/selfware-harbor-medium.toml}"
HARBOR_BIN="${HARBOR_BIN:-$HOME/.local/bin/harbor}"
SELFWARE="/home/rig/selfware/target/release/selfware"

mkdir -p "$ARCHIVE/candidates"

next_id() {
  local n
  n=$(ls "$ARCHIVE/candidates" 2>/dev/null | grep -c '^c[0-9]*$' || true)
  printf 'c%03d' "$n"
}

record_candidate() {
  # $1=id  $2=parent_id  $3=config path  $4=harbor job dir
  local id="$1" parent="$2" cfg="$3" jobdir="$4"
  if [ -z "$jobdir" ] || [ ! -d "$jobdir" ]; then
    echo "ERROR: no job dir for $id — evaluation did not produce trials; refusing to record" >&2
    return 1
  fi
  python3 - "$ARCHIVE" "$id" "$parent" "$cfg" "$jobdir" "$SEARCH_TASKS" <<'PYEOF'
import json, sys, pathlib, hashlib, subprocess
archive, cid, parent, cfg, jobdir, planned = sys.argv[1:7]
jobdir = pathlib.Path(jobdir)
# Basenames: trial dirs cannot contain the "terminal-bench/" prefix.
planned = [t.split("/")[-1] for t in planned.split()]
rewards, traces, cost = {}, {}, None
for trial in sorted(jobdir.glob("*/")):
    task = trial.name.split("__")[0]
    rw = trial / "verifier" / "reward.txt"
    r = float(rw.read_text().strip()) if rw.exists() else None
    # Keep every replicate: two trials of one task must NOT collapse into
    # the last-seen reward (external review finding 10).
    rewards.setdefault(task, []).append(r)
    agent_log = trial / "agent" / "selfware.txt"
    ver_out = trial / "verifier" / "test-stdout.txt"
    traces.setdefault(task, []).append([str(agent_log), str(ver_out)])
result = jobdir / "result.json"
if result.exists():
    # Preserve "unknown": a missing cost is NOT $0.00 — recording 0.0 would
    # make un-metered runs indistinguishable from free ones and silently
    # improve every cost comparison (external review of 6e231e2e, #7).
    raw = json.loads(result.read_text()).get("total_cost_usd")
    cost = float(raw) if raw is not None else None
try:
    rev = subprocess.run(
        ["git", "rev-parse", "--short", "HEAD"],
        capture_output=True, text=True, check=True,
    ).stdout.strip()
except Exception:
    rev = "unknown"
# A candidate is complete only if every PLANNED task produced a reward;
# one observed success out of 8 planned must not read as mean 1.000.
missing = [t for t in planned
           if not any(r is not None for r in rewards.get(t, []))]
complete = not missing
flat = [r for rs in rewards.values() for r in rs if r is not None]
mean = sum(flat) / len(flat) if flat else None
rec = {
    "id": cid,
    "parent": parent,
    "config_sha256": hashlib.sha256(pathlib.Path(cfg).read_bytes()).hexdigest()[:16],
    "selfware_rev": rev,
    "rewards": rewards,
    "mean_reward": mean,
    "complete": complete,
    "missing_tasks": missing,
    "total_cost_usd": cost,
    "trace_paths": traces,
}
with open(pathlib.Path(archive) / "candidates.jsonl", "a") as f:
    f.write(json.dumps(rec) + "\n")
cost_str = f"${cost:.2f}" if cost is not None else "unknown"
mean_str = f"{mean:.3f}" if mean is not None else "unknown"
flag = "" if complete else f" INCOMPLETE (missing: {', '.join(missing)})"
print(f"recorded {cid}: mean_reward={mean_str} cost={cost_str}{flag}")
PYEOF
}

evaluate() {
  # $1=id  $2=config -> prints the harbor job dir on stdout (last line).
  # Harbor needs the docker group: wrap in `sg docker -c`. Job dir is found
  # by unique --job-name (jobs/-listing diff only as fallback), so a harbor
  # failure can never return a stale dir.
  local id="$1" cfg="$2"
  local includes=()
  for t in $SEARCH_TASKS; do includes+=(-i "$t"); done
  # Concurrency = task count (NOT the array length, which counts the -i flags
  # too — that bug ran n=16 and collapsed env builds under load).
  local n_tasks=0
  for _ in $SEARCH_TASKS; do n_tasks=$((n_tasks + 1)); done
  local n_conc=$(( n_tasks < 4 ? n_tasks : 4 ))
  local before after
  # Unique job name: harbor supports --job-name, so attribution no longer
  # depends on diffing the shared jobs/ dir (racy under concurrent launches).
  local jobtag="hs-${id}-$(date +%Y%m%d-%H%M%S)-$$"
  local jobpath="/home/rig/harbor-agents/jobs/$jobtag"
  before=$(ls /home/rig/harbor-agents/jobs/ 2>/dev/null)
  sg docker -c "cd /home/rig/harbor-agents && \
    SELFWARE_HARBOR_CONFIG='$cfg' PYTHONPATH=/home/rig/harbor-agents \
    SELFWARE_BINARY='${SELFWARE_BINARY:-/home/rig/harbor-agents/dist/selfware-bullseye}' \
    SELFWARE_API_KEY='$SELFWARE_API_KEY' \
    '$HARBOR_BIN' run -d terminal-bench/terminal-bench@latest \
      --agent selfware_agent:SelfwareAgent -k 1 -n $n_conc --env docker \
      --job-name '$jobtag' \
      ${includes[*]@Q} -q" || echo "harbor evaluation of $id failed"
  if [ -d "$jobpath" ]; then
    echo "$jobpath"
    return 0
  fi
  # Fallback: name not honored — diff the jobs/ listing as before.
  after=$(ls /home/rig/harbor-agents/jobs/ 2>/dev/null)
  comm -13 <(echo "$before") <(echo "$after") | grep '^hs-' | head -1 | sed 's|^|/home/rig/harbor-agents/jobs/|'
}

# --- iteration 0: seed the archive with the current profile ---
seed_id="c000"
if [ ! -s "$ARCHIVE/candidates.jsonl" ]; then
  mkdir -p "$ARCHIVE/candidates/$seed_id"
  cp "$SEED_CONFIG" "$ARCHIVE/candidates/$seed_id/config.toml"
  echo "evaluating seed $seed_id..."
  jobdir=$(evaluate "$seed_id" "$SEED_CONFIG" | tail -1)
  record_candidate "$seed_id" "" "$SEED_CONFIG" "$jobdir"
fi

for i in $(seq 1 "$ITERATIONS"); do
  parent_id=$(python3 -c "
import json, sys
recs = [json.loads(l) for l in open('$ARCHIVE/candidates.jsonl')]
# Incomplete candidates stay in the archive for diagnosis but must never
# be promoted to parent (missing planned tasks would inflate their mean).
eligible = [r for r in recs if r.get('complete', True) and r['mean_reward'] is not None]
if not eligible:
    sys.exit('no complete candidate in archive — cannot select a parent')
best = max(eligible, key=lambda r: r['mean_reward'])
print(best['id'])")
  parent_cfg="$ARCHIVE/candidates/$parent_id/config.toml"
  cid=$(next_id)
  work="$ARCHIVE/candidates/$cid"
  mkdir -p "$work"
  cp "$parent_cfg" "$work/config.toml"

  echo "=== iteration $i: proposer on top of $parent_id -> $cid ==="
  (
    cd "$work"
    timeout 900 "$SELFWARE" run -m yolo -c /home/rig/selfware/benchmarks/harbor/proposer.toml \
      "You are the harness proposer. Read the skill at /home/rig/selfware/benchmarks/harbor/proposer-skill.md and follow it exactly. The archive is at $ARCHIVE. The parent candidate is $parent_id (config: $parent_cfg). Write proposal.diff and proposal.md here. IMPORTANT: only write those two files — never run patch, never edit config.toml yourself; the harness applies your diff." \
      || echo "proposer failed/timed out — keeping parent config unchanged"
  )

  if [ -s "$work/proposal.diff" ]; then
    (cd "$work" && patch -p0 --fuzz=3 config.toml < proposal.diff) || {
      echo "patch failed; trying tolerant apply"
      python3 - "$work" <<'PYEOF'
import pathlib, sys
work = pathlib.Path(sys.argv[1])
cfg = work / "config.toml"
diff = (work / "proposal.diff").read_text()
text = cfg.read_text()
removed = [l[1:] for l in diff.splitlines() if l.startswith("-") and not l.startswith("---")]
added = [l[1:] for l in diff.splitlines() if l.startswith("+") and not l.startswith("+++")]
if len(removed) == len(added):
    for old, new in zip(removed, added):
        if old in text:
            text = text.replace(old, new, 1)
    cfg.write_text(text)
    print("tolerant apply done")
else:
    print("tolerant apply skipped (unbalanced diff)")
PYEOF
    }
  else
    echo "no proposal.diff — candidate $cid keeps parent config"
  fi

  jobdir=$(evaluate "$cid" "$work/config.toml" | tail -1)
  record_candidate "$cid" "$parent_id" "$work/config.toml" "$jobdir"
done

echo "=== archive state ==="
cat "$ARCHIVE/candidates.jsonl" | python3 -c "
import json, sys
recs = [json.loads(l) for l in sys.stdin]
# None sorts below all numbers: unknown means unknown, never $0.00 / 0.000.
recs.sort(key=lambda r: (r['mean_reward'] is None, -(r['mean_reward'] or 0)))
for r in recs:
    mean = r['mean_reward']
    mean_str = f'{mean:.3f}' if mean is not None else 'unknown'
    cost = r['total_cost_usd']
    cost_str = f'\${cost:.2f}' if cost is not None else 'unknown'
    flag = '' if r.get('complete', True) else ' INCOMPLETE'
    print(f\"{r['id']} parent={r['parent'] or '-':5} mean={mean_str} cost={cost_str}{flag}\")
"
