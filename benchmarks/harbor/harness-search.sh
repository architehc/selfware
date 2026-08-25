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
  python3 - "$ARCHIVE" "$id" "$parent" "$cfg" "$jobdir" <<'PYEOF'
import json, sys, pathlib, hashlib
archive, cid, parent, cfg, jobdir = sys.argv[1:6]
jobdir = pathlib.Path(jobdir)
rewards, traces, cost = {}, {}, 0.0
for trial in sorted(jobdir.glob("*/")):
    task = trial.name.split("__")[0]
    rw = trial / "verifier" / "reward.txt"
    rewards[task] = float(rw.read_text().strip()) if rw.exists() else None
    agent_log = trial / "agent" / "selfware.txt"
    ver_out = trial / "verifier" / "test-stdout.txt"
    traces[task] = [str(agent_log), str(ver_out)]
result = jobdir / "result.json"
if result.exists():
    cost = json.loads(result.read_text()).get("total_cost_usd") or 0.0
rec = {
    "id": cid,
    "parent": parent,
    "config_sha256": hashlib.sha256(pathlib.Path(cfg).read_bytes()).hexdigest()[:16],
    "rewards": rewards,
    "mean_reward": sum(r for r in rewards.values() if r is not None) / max(1, len(rewards)),
    "total_cost_usd": cost,
    "trace_paths": traces,
}
with open(pathlib.Path(archive) / "candidates.jsonl", "a") as f:
    f.write(json.dumps(rec) + "\n")
print(f"recorded {cid}: mean_reward={rec['mean_reward']:.3f} cost=${cost:.2f}")
PYEOF
}

evaluate() {
  # $1=id  $2=config -> prints the harbor job dir on stdout (last line).
  # Harbor needs the docker group: wrap in `sg docker -c`. Job dir is detected
  # by diffing the jobs/ listing, so a harbor failure can never return a stale dir.
  local id="$1" cfg="$2"
  local includes=()
  for t in $SEARCH_TASKS; do includes+=(-i "$t"); done
  # Concurrency = task count (NOT the array length, which counts the -i flags
  # too — that bug ran n=16 and collapsed env builds under load).
  local n_tasks=0
  for _ in $SEARCH_TASKS; do n_tasks=$((n_tasks + 1)); done
  local n_conc=$(( n_tasks < 4 ? n_tasks : 4 ))
  local before after
  before=$(ls /home/rig/harbor-agents/jobs/ 2>/dev/null)
  sg docker -c "cd /home/rig/harbor-agents && \
    SELFWARE_HARBOR_CONFIG='$cfg' PYTHONPATH=/home/rig/harbor-agents \
    SELFWARE_BINARY='${SELFWARE_BINARY:-/home/rig/harbor-agents/dist/selfware-bullseye}' \
    SELFWARE_API_KEY='$SELFWARE_API_KEY' \
    '$HARBOR_BIN' run -d terminal-bench/terminal-bench@latest \
      --agent selfware_agent:SelfwareAgent -k 1 -n $n_conc --env docker \
      ${includes[*]@Q} -q" || echo "harbor evaluation of $id failed"
  after=$(ls /home/rig/harbor-agents/jobs/ 2>/dev/null)
  comm -13 <(echo "$before") <(echo "$after") | head -1 | sed 's|^|/home/rig/harbor-agents/jobs/|'
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
import json
recs = [json.loads(l) for l in open('$ARCHIVE/candidates.jsonl')]
best = max(recs, key=lambda r: r['mean_reward'])
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
recs.sort(key=lambda r: -r['mean_reward'])
for r in recs:
    print(f\"{r['id']} parent={r['parent'] or '-':5} mean={r['mean_reward']:.3f} cost=\${r['total_cost_usd']:.2f}\")
"
