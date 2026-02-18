# Selfware Long-Run Mega Project Plan

This folder is a dedicated harness to run multi-hour Selfware sessions, collect evidence, and report what improved or regressed.

## Objectives

- Stress-test Selfware over long coding sessions (4-8+ hours)
- Measure reliability, pace, and quality across many tasks
- Capture experience improvements between runs

## Folder Layout

- `selfware.longrun.toml`: long-run tuned Selfware config
- `tasks/mega_tasks.txt`: phased task backlog for long sessions
- `scripts/bootstrap_megaproject.sh`: creates a test Rust workspace
- `scripts/run_longrun.sh`: executes the long-run task sequence
- `scripts/summarize_results.sh`: generates run summary markdown
- `project/`: generated mega-project workspace (target under test)
- `report/experience_report_template.md`: qualitative report template

## Execution Plan (Per Run)

1. Phase 0 (10-15 min): bootstrap and baseline
2. Phase 1 (30-45 min): architecture + core scaffolding
3. Phase 2 (90-150 min): feature expansion and tests
4. Phase 3 (60-120 min): refactors + performance + docs
5. Phase 4 (30-60 min): failure/recovery scenarios
6. Phase 5 (15-30 min): final validation and review

## Quick Start

Run from repository root:

```bash
bash experiments/selfware-longrun-mega/scripts/bootstrap_megaproject.sh
bash experiments/selfware-longrun-mega/scripts/run_longrun.sh
```

Run summary will be written to:

- `experiments/selfware-longrun-mega/runs/<run_id>/summary.md`
- `experiments/selfware-longrun-mega/runs/<run_id>/results.csv`

## Improvement Reporting

After each run:

1. Copy `report/experience_report_template.md` into `runs/<run_id>/experience.md`
2. Fill quantitative metrics from `summary.md`
3. Fill qualitative observations (friction, wins, regressions)
4. Compare to previous run and list concrete changes for next iteration

## Notes

- Use `--yolo` in scripted long-runs to avoid interactive blocking.
- Keep run configs stable for apples-to-apples comparisons.
- Change one variable at a time when testing improvements.
