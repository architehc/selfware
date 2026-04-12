# Documentation & Repository Hygiene Audit Report

**Agent:** Agent 4 — Documentation & Repo Hygiene  
**Date:** 2026-04-09  
**Branch:** agent-20260405-145152  
**Repository:** https://github.com/architehc/selfware

---

## Executive Summary

This repository has **significant documentation sprawl** and **git hygiene issues**. There are **62 markdown files** in the root directory (many auto-generated reports), **101GB of test artifacts**, and **111 dirty files/submodules** pending. The `.gitignore` is reasonably comprehensive but several generated files and test artifacts remain tracked or untracked in the working directory.

**Critical Findings:**
- 728 commits mention "checkpoint/test/agent/wip/temp" suggesting heavy WIP commit usage
- 101GB repository size (16GB is `target/`, 101GB is `long_run_tests/`)
- 111 dirty files/submodules in git status
- Multiple stale architecture docs contradicting actual codebase

---

## 1. Documentation Inventory

### Root Directory Markdown Files (62 total)

| # | Filename | Last Modified | Category | Status | Action Needed |
|---|----------|---------------|----------|--------|---------------|
| 1 | README.md | 2026-03-31 | Core Doc | ✅ Current | Keep updated |
| 2 | CHANGELOG.md | 2026-02-16 | Core Doc | ⚠️ STALE (1.5mo) | UPDATE |
| 3 | ARCHITECTURE_SUMMARY.md | 2026-03-20 | Architecture | ⚠️ STALE (3w) | UPDATE |
| 4 | ARCHITECTURE_ASSESSMENT.md | 2026-03-28 | Architecture | ⚠️ STALE (2w) | UPDATE |
| 5 | ARCHITECTURE_FOR_SMALLER_MODELS.md | 2026-03-20 | Architecture | ⚠️ STALE | UPDATE |
| 6 | CONTRIBUTING.md | 2026-02-27 | Core Doc | ✅ Current | Keep |
| 7 | SECURITY.md | 2026-03-09 | Core Doc | ✅ Current | Keep |
| 8 | PRIVACY.md | 2026-03-09 | Core Doc | ✅ Current | Keep |
| 9 | docs/architecture.md | — | Architecture | ✅ Current | Keep |
| 10 | HNSW_SUMMARY.md | 2026-03-26 | Generated Report | ⚠️ STALE | ARCHIVE |
| 11 | HNSW_IMPLEMENTATION.md | 2026-03-26 | Technical | ⚠️ STALE | REVIEW |
| 12 | CACHE_INTEGRATION_COMPLETE.md | 2026-03-19 | Status Report | ⚠️ STALE | ARCHIVE |
| 13 | KNOWLEDGE_GRAPH_SUMMARY.md | 2026-03-19 | Status Report | ⚠️ STALE | ARCHIVE |
| 14 | CLEANUP_PLAN.md | 2026-03-22 | Meta | ⚠️ STALE | ARCHIVE |
| 15 | COMPARISON_FRAMEWORK.md | 2026-03-24 | Technical | ⚠️ STALE | REVIEW |
| 16 | CODEBASE_HEALTH_ASSESSMENT.md | 2026-03-19 | Status Report | ⚠️ STALE | ARCHIVE |
| 17 | COMMAND_CENTER_ARCH.md | 2026-03-28 | Architecture | ⚠️ STALE | MERGE/ARCHIVE |
| 18 | COMPRESSION_IMPLEMENTATION.md | 2026-03-31 | Technical | ✅ Recent | Keep |
| 19 | CONFIG_COMPARISON.md | 2026-03-25 | Reference | ⚠️ STALE | ARCHIVE |
| 20 | CONSOLIDATED_BROKEN_IMPLEMENTATIONS_REPORT.md | 2026-03-14 | Status Report | ⚠️ STALE | ARCHIVE |
| 21 | COORDINATOR_MODE_SUMMARY.md | 2026-03-31 | Status Report | ✅ Recent | Keep |
| 22 | DEAD_CODE_TODOS_REPORT.md | 2026-03-19 | Status Report | ⚠️ STALE | ARCHIVE |
| 23 | DEPENDENCY_AUDIT_REPORT.md | 2026-03-28 | Status Report | ⚠️ STALE | ARCHIVE |
| 24 | DOCKER_GUIDE.md | 2026-03-24 | User Guide | ⚠️ STALE | UPDATE |
| 25 | DOCKER_SETUP_SUMMARY.md | 2026-03-24 | User Guide | ⚠️ STALE | MERGE |
| 26 | ENDPOINT_CAPABILITIES.md | 2026-03-24 | Reference | ⚠️ STALE | REVIEW |
| 27 | ENDPOINT_OPTIMIZATION_GUIDE.md | 2026-03-26 | User Guide | ⚠️ STALE | UPDATE |
| 28 | EVAL_REPORT.md | 2026-03-03 | Status Report | 🚫 OBSOLETE (1mo+) | DELETE |
| 29 | EXPERIMENTS_PRE_MERGE.md | 2026-03-23 | Status Report | ⚠️ STALE | ARCHIVE |
| 30 | FINAL_ASSESSMENT_SUMMARY.md | 2026-03-28 | Status Report | ⚠️ STALE | ARCHIVE |
| 31 | FINAL_IMPLEMENTATION_SUMMARY.md | 2026-03-24 | Status Report | ⚠️ STALE | ARCHIVE |
| 32 | FIXES_STATUS_REPORT.md | 2026-03-19 | Status Report | ⚠️ STALE | ARCHIVE |
| 33 | GEMINI_RECOMMENDATIONS.md | 2026-03-06 | External Ref | 🚫 OBSOLETE | DELETE |
| 34 | IMPLEMENTATION_REVIEW_REPORT.md | 2026-03-28 | Status Report | ⚠️ STALE | ARCHIVE |
| 35 | MIGRATION_PLAN.md | 2026-03-28 | Planning | ⚠️ STALE | ARCHIVE |
| 36 | MULTIAGENT_122B_REVIEW_REPORT.md | 2026-03-28 | Test Report | ⚠️ STALE | ARCHIVE |
| 37 | MULTIMODAL_WORKFLOWS_GUIDE.md | 2026-03-24 | User Guide | ⚠️ STALE | UPDATE |
| 38 | NEXT_STEPS_RECOMMENDATIONS.md | 2026-03-24 | Planning | ⚠️ STALE | ARCHIVE |
| 39 | POST_TEST_CHECKLIST.md | 2026-03-25 | Checklist | ⚠️ STALE | ARCHIVE |
| 40 | PROJECT_REVIEW_2026_03_06.md | 2026-03-20 | Status Report | ⚠️ STALE | ARCHIVE |
| 41 | QUICK_FIXES.md | 2026-03-20 | Planning | ⚠️ STALE | ARCHIVE |
| 42 | REVIEW.md | 2026-03-26 | Generic Review | ⚠️ STALE | ARCHIVE |
| 43 | SELFWARE_ENDPOINT_ANALYSIS.md | 2026-03-24 | Technical | ⚠️ STALE | ARCHIVE |
| 44 | SELFWARE_IMPROVEMENTS.md | 2026-03-24 | Planning | ⚠️ STALE | ARCHIVE |
| 45 | SELFWARE_IMPROVEMENTS_IMPLEMENTED.md | 2026-03-24 | Status Report | ⚠️ STALE | ARCHIVE |
| 46 | SWARM_DISPATCH_INVESTIGATION.md | 2026-03-23 | Investigation | ⚠️ STALE | ARCHIVE |
| 47 | SWL_DESIGN.md | 2026-03-28 | Technical | ⚠️ STALE | REVIEW |
| 48 | SWL_IMPLEMENTATION_ROADMAP.md | 2026-03-28 | Planning | ⚠️ STALE | ARCHIVE |
| 49 | SWL_LIVE_EXECUTION_SUMMARY.md | 2026-03-28 | Status Report | ⚠️ STALE | ARCHIVE |
| 50 | SYSTEM_TEST_RESULTS.md | 2026-03-24 | Test Report | ⚠️ STALE | ARCHIVE |
| 51 | TASKS.md | 2026-03-19 | Planning | ⚠️ STALE | UPDATE |
| 52 | TOOL_EXECUTION_FLOW_ANALYSIS.md | 2026-03-28 | Technical | ⚠️ STALE | ARCHIVE |
| 53 | TOOL_GUARDRAIL_INTEGRATION_REVIEW.md | 2026-03-29 | Technical | ✅ Recent | Keep |
| 54 | TUI_FIX_SUMMARY.md | 2026-03-24 | Status Report | ⚠️ STALE | ARCHIVE |
| 55 | VISUAL_VALIDATION_WORKFLOW.md | 2026-03-24 | Technical | ⚠️ STALE | REVIEW |
| 56 | 32_CONCURRENT_TEST_REPORT.md | 2026-03-24 | Test Report | ⚠️ STALE | ARCHIVE |
| 57 | BENCHMARK_SUMMARY.md | 2026-03-26 | Test Report | ⚠️ STALE | ARCHIVE |
| 58 | ANNOUNCEMENT_PREP.md | 2026-03-23 | Meta | ⚠️ STALE | ARCHIVE |
| 59 | CI_CD_REVIEW_REPORT.md | 2026-03-28 | Status Report | ⚠️ STALE | ARCHIVE |
| 60 | COMPREHENSIVE_PROJECT_REVIEW.md | 2026-03-05 | Status Report | 🚫 OBSOLETE | ARCHIVE |
| 61 | benchmark_final_report_20260327.md | 2026-03-27 | Test Report | ⚠️ STALE | ARCHIVE |
| 62 | complex_test_scenario.md | 2026-03-24 | Test Case | ⚠️ STALE | ARCHIVE |

### Docs/ Directory (28 files, 404KB)

| Category | Files | Status |
|----------|-------|--------|
| User Guides | getting-started.md, configuration.md, tools.md, hooks.md, mcp.md, doctor.md, interactive-commands.md, limitations.md, zed-extension.md | ✅ Good |
| Architecture | architecture.md | ✅ Good |
| SWL/Workflows | MICRO_EVOLUTION_GUIDE.md, TUI_GARDEN_GUIDE.md, AGENT_SWARM_UI_SUMMARY.md | ✅ Good |
| SWE-bench | SWE_BENCH_EVALUATION_STRATEGY.md, SWE_BENCH_QUICKSTART.md | ✅ Good |
| Reviews | COMPREHENSIVE_REVIEW.md, DEEP_DIVE_REVIEW.md | ⚠️ May be stale |
| Hermes Setup | HERMES_SETUP_*.md (4 files) | ⚠️ Check relevance |
| Other | LM_STUDIO_MAC_GUIDE.md, LONG_RUNNING_TEST_PLAN.md, MEGA_TEST_PLAN_SUMMARY.md, PERMISSION_SYSTEM.md, QWEN_CODE_CLI_UI.md, UX_RECOMMENDATIONS.md, agent_swarm_ui_guide.md, selfware-qa-specification.md | Mixed |

---

## 2. Doc Accuracy Issues

### README.md Accuracy Check

| Feature | README Claims | Actual Status | Issue |
|---------|---------------|---------------|-------|
| Lines of code | "~197k lines" | **215,813** lines (src/) | ⚠️ Undercount by ~18k |
| Built-in tools | "70+ tools" | Needs verification | Check actual count |
| Test count | "~6,400 unit tests" | Needs verification | May be inflated |
| Coverage | "~82% line coverage" | Check `tarpaulin-report.html` | Verify |
| SAB scenarios | "12 scenarios" | Check `system_tests/` | Verify |

### ARCHITECTURE_SUMMARY.md Issues

| Item | Document Claims | Actual Status | Issue |
|------|-----------------|---------------|-------|
| Code size | "~197k lines" | **215,813** lines | ❌ **WRONG** |
| Tool count | "54 built-in tools" | Likely 70+ | ⚠️ Undercount |
| Test count | "~6,400 unit tests" | Needs verification | ⚠️ Check |
| Coverage | "~82% line coverage" | Needs verification | ⚠️ Check |
| Feature status | "self-improvement: Stubbed" | Check actual status | May be outdated |
| Critical issues table | Lists 2 blocking I/O, 1 security bypass | Check if fixed | May be resolved |

### CHANGELOG.md Issues

- **Last entry:** 2026-02-16 (1.5 months stale)
- Missing entries for:
  - TUI enhancements (March)
  - 4-day continuous test harness (April)
  - 457 new unit tests (April 5)
  - Self-healing improvements
  - Checkpointing enhancements
  - Multi-agent improvements

### Architecture Doc Drift

The `ARCHITECTURE_SUMMARY.md` describes modules that may not exist or have been renamed:
- `cognitive/rsi_orchestrator.rs` — check if still stubbed
- `safety/yolo.rs` — verify existence
- `safety/threat_modeling.rs` — verify existence
- Path validation mentions "O_NOFOLLOW atomic open" — verify implementation

---

## 3. Git Hygiene Issues

### 3.1 Dirty Files (111 total)

**Type breakdown:**
- Modified submodules: 14 (bench_results/swebench/repos/*)
- Modified worktree files: 46 (long_run_tests/*/agent_*)
- Untracked directories: 51 (test result directories)

**Submodule Issues:**
```
M bench_results/swebench/repos/astropy-astropy
M bench_results/swebench/repos/pylint-dev-pylint
M bench_results/swebench/repos/pytest-dev-pytest
M bench_results/swebench/repos/sphinx-doc-sphinx
M bench_results/swebench/workdirs/django-django-11133
```

These appear to be **nested git repos, not submodules**, causing git to see them as dirty.

### 3.2 Files That SHOULD Be in .gitignore

| File/Pattern | Current Status | Should Be Ignored? | Size |
|--------------|----------------|-------------------|------|
| `.selfware/tool_results/*.json` | Untracked | ✅ YES | ~3MB |
| `hello`, `hello_test` | Tracked | ✅ YES (binaries) | 7.6MB |
| `codegraph.json` | In .gitignore but exists | DELETE | 4.6MB |
| `knowledge_graph.json` | Tracked | ✅ YES (generated) | 96KB |
| `tarpaulin-report.html` | In .gitignore but exists | DELETE | 9.9MB |
| `.evolution-log.jsonl` | Tracked | ⚠️ Maybe (118 entries) | — |
| `checkpointing.rs` (root) | Tracked | ✅ YES (should be in src/) | 76KB |
| `*.bak` files | Untracked in test dirs | Add to .gitignore | — |
| `vscode-selfware/node_modules/` | Partially ignored | Verify | — |
| `long_run_tests/*` | Partially ignored | Many patterns missing | 101GB |

### 3.3 Test Artifacts Not Fully Ignored

**Missing from .gitignore:**
```
/live_test_*/
/test_output_*/
/test_real_*/
/long_run_tests/*day_results_*/
/long_run_tests/marathon_*/
/long_run_tests/system_test_8hr_*/
/long_run_tests/e2e_*/
```

### 3.4 Large Files Analysis

| File | Size | In Repo? | Action |
|------|------|----------|--------|
| `codegraph.json` | 4.6MB | Yes (ignored but tracked) | `git rm --cached` |
| `knowledge_graph.json` | 96KB | Yes | Move to generated/ |
| `hello` | 3.8MB | Yes | `git rm` (build artifact) |
| `hello_test` | 3.8MB | Yes | `git rm` (build artifact) |
| `tarpaulin-report.html` | 9.9MB | Yes (ignored but tracked) | `git rm --cached` |
| `screenshots/*.png` | ~9MB total | Yes | Keep or move to LFS |
| `screenshots/*.gif` | 2.0MB | Yes | Keep or move to LFS |

---

## 4. Cleanup Plan

### DELETE (Free ~30MB + reduce clutter)

| File/Dir | Reason | Estimated Savings |
|----------|--------|-------------------|
| `codegraph.json` | Generated file, already ignored | 4.6MB |
| `hello` | Binary build artifact | 3.8MB |
| `hello_test` | Binary build artifact | 3.8MB |
| `tarpaulin-report.html` | Coverage report, already ignored | 9.9MB |
| `EVAL_REPORT.md` | Obsolete (March 3) | — |
| `GEMINI_RECOMMENDATIONS.md` | External ref, obsolete | — |
| `CONSOLIDATED_BROKEN_IMPLEMENTATIONS_REPORT.md` | Superseded | — |
| Various stale *REPORT.md, *SUMMARY.md files | Superseded by newer audits | — |

### ARCHIVE (Move to `docs/archive/` or `archive/`)

Create `docs/archive/` for:

| Files to Archive | Count |
|------------------|-------|
| `*_REPORT.md` (test/audit reports) | ~15 |
| `*_SUMMARY.md` (status summaries) | ~10 |
| `*_ASSESSMENT.md` (old assessments) | ~5 |
| `CLEANUP_PLAN.md`, `MIGRATION_PLAN.md`, `NEXT_STEPS_*.md` | ~5 |
| `SWL_*_SUMMARY.md`, `*_LIVE_EXECUTION_*.md` | ~3 |
| `32_CONCURRENT_TEST_REPORT.md`, `benchmark_final_report_*.md` | ~2 |

**Total: ~40 markdown files → archive**

### UPDATE (Priority order)

| File | Updates Needed |
|------|----------------|
| `CHANGELOG.md` | Add all changes since Feb 16 |
| `README.md` | Fix line count (215k not 197k) |
| `ARCHITECTURE_SUMMARY.md` | Fix stats, update module list, verify critical issues |
| `TASKS.md` | Review if still relevant |
| `DOCKER_GUIDE.md` | Verify current |

### .gitignore Additions Needed

```gitignore
# Additional test artifacts
/live_test_*/
/test_output_*/
/test_real_*/
/diverse_e2e_*/
/focused_followup_*/
/greenfield_batch_*/
/marathon_*/
/e2e_*/

# Agent worktrees
/worktree-agent-*/

# Backup files in test directories
**/*.bak

# Selfware tool results
.selfware/tool_results/*.json

# Generated JSON files
knowledge_graph.json
codegraph.json

# Binary artifacts at root
/hello
/hello_test
```

### Estimated Size Savings

| Action | Savings |
|--------|---------|
| Delete tracked binaries (hello, hello_test) | 7.6MB |
| Delete cached ignored files | 14.5MB |
| Remove old test result dirs (if safe) | 101GB (long_run_tests) |
| **Total potential** | **~101GB** |

---

## 5. Branch Strategy Assessment

### Current State

- **Active branch:** `agent-20260405-145152`
- **Main branch:** `main` (remotes/origin/main)
- **Total local branches:** 75+
- **Worktree branches:** ~50 (`worktree-agent-*`)

### Branch Categories

| Pattern | Count | Purpose | Recommendation |
|---------|-------|---------|----------------|
| `agent-YYYYMMDD-*` | 60+ | Agent checkpoints | Clean up old ones |
| `worktree-agent-*` | 50 | Worktree branches | Clean up merged ones |
| `dependabot/*` | 8 | Dependency updates | Keep, auto-managed |
| `main`, `master` | 2 | Primary branches | Keep |

### Recommendations

1. **Immediate:**
   - Clean up worktree branches older than 2 weeks
   - Delete agent branches already merged to main

2. **Short-term:**
   - Establish branch naming convention
   - Set up automated branch cleanup for old agent branches
   - Consider using `--single-branch` clones for CI

3. **Submodule Fix:**
   - The `bench_results/swebench/repos/*` are not proper submodules
   - Either convert to submodules or add to `.gitignore`:
   ```gitignore
   /bench_results/swebench/repos/*/
   /bench_results/swebench/workdirs/*/
   ```

---

## Appendix A: Repository Statistics

| Metric | Value |
|--------|-------|
| Total repo size | 121GB |
| `.git` size | 1.4GB |
| `target/` size | 16GB |
| `long_run_tests/` size | 101GB |
| Source code (src/) | 9.7MB (215,813 lines) |
| Markdown files (root) | 62 files |
| Markdown files (docs/) | 28 files |
| Test directories | 50+ |
| Git commits (checkpoint/test/temp) | 728 |

---

## Appendix B: Quick Wins Checklist

- [ ] `git rm --cached codegraph.json tarpaulin-report.html`
- [ ] `git rm hello hello_test`
- [ ] Add `/hello` and `/hello_test` to `.gitignore`
- [ ] Add `**/*.bak` to `.gitignore`
- [ ] Add bench_results subdirs to `.gitignore`
- [ ] Archive 40+ stale markdown files
- [ ] Update CHANGELOG.md with recent changes
- [ ] Fix README.md line count
- [ ] Clean up old agent branches
- [ ] Document branch cleanup policy

---

*Report generated by Agent 4 — Documentation & Repo Hygiene Audit*
