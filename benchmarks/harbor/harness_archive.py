#!/usr/bin/env python3
"""Record and select benchmark candidates with explicit comparison identity."""

import hashlib
import json
import math
from pathlib import Path
import subprocess
import sys


def fingerprint(binary):
    digest = hashlib.sha256()
    with Path(binary).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def task_roster(planned):
    tasks = sorted(planned.split())
    names = [task.rsplit("/", 1)[-1] for task in tasks]
    if not tasks or len(set(names)) != len(tasks):
        raise ValueError("planned task roster must be nonempty with unique task basenames")
    return tasks


def select_candidate(records, planned, binary_sha256):
    roster = task_roster(planned)
    eligible = [record for record in records
                if record.get("complete") is True
                and record.get("planned_tasks") == roster
                and record.get("selfware_sha256") == binary_sha256
                and isinstance(record.get("mean_reward"), (int, float))
                and math.isfinite(record["mean_reward"])]
    return max(eligible, key=lambda record: record["mean_reward"]) if eligible else None


def record_candidate(archive, cid, parent, config, jobdir, planned, binary, expected_sha):
    binary_sha = fingerprint(binary)
    if binary_sha != expected_sha:
        raise ValueError("evaluated binary changed during the benchmark; refusing attribution")
    roster = task_roster(planned)
    names = [task.rsplit("/", 1)[-1] for task in roster]
    jobdir = Path(jobdir)
    if not jobdir.is_dir():
        raise ValueError("evaluation did not produce a job directory")
    rewards, traces = {}, {}
    for trial in sorted(jobdir.glob("*/")):
        task = trial.name.split("__")[0]
        reward_file = trial / "verifier/reward.txt"
        reward = float(reward_file.read_text().strip()) if reward_file.exists() else None
        if reward is not None and not math.isfinite(reward):
            reward = None
        rewards.setdefault(task, []).append(reward)
        traces.setdefault(task, []).append([str(trial / "agent/selfware.txt"),
                                           str(trial / "verifier/test-stdout.txt")])
    missing = [name for name in names if not rewards.get(name)
               or any(reward is None for reward in rewards[name])]
    unexpected = sorted(set(rewards) - set(names))
    complete = not missing and not unexpected
    means = [sum(rewards[name]) / len(rewards[name]) for name in names if name not in missing]
    mean = sum(means) / len(means) if means else None
    result = jobdir / "result.json"
    raw_cost = json.loads(result.read_text()).get("total_cost_usd") if result.exists() else None
    cost = float(raw_cost) if raw_cost is not None else None
    if cost is not None and not math.isfinite(cost):
        cost = None
    revision = subprocess.run(["git", "rev-parse", "HEAD"], capture_output=True, text=True)
    record = {"id": cid, "parent": parent,
              "config_sha256": hashlib.sha256(Path(config).read_bytes()).hexdigest(),
              "selfware_rev": revision.stdout.strip() if revision.returncode == 0 else "unknown",
              "selfware_sha256": binary_sha, "planned_tasks": roster,
              "rewards": rewards, "mean_reward": mean, "complete": complete,
              "missing_tasks": missing, "unexpected_tasks": unexpected,
              "total_cost_usd": cost, "trace_paths": traces}
    with (Path(archive) / "candidates.jsonl").open("a") as output:
        output.write(json.dumps(record) + "\n")
    return record


def main():
    command, *args = sys.argv[1:]
    if command == "fingerprint":
        print(fingerprint(*args))
    elif command == "select":
        archive, planned, binary = args
        path = Path(archive) / "candidates.jsonl"
        records = [json.loads(line) for line in path.read_text().splitlines() if line.strip()] if path.exists() else []
        best = select_candidate(records, planned, fingerprint(binary))
        if best is None:
            return 1
        print(best["id"])
    elif command == "record":
        record = record_candidate(*args)
        print(f"recorded {record['id']}: mean_reward={record['mean_reward']} "
              f"cost={record['total_cost_usd']} complete={record['complete']}")
    else:
        raise ValueError(f"unknown command: {command}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
