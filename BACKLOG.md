# Selfware Backlog - Remaining Tasks

**Generated:** 2026-04-12

---

## 🔴 CRITICAL (Before Production Release)

### 1. Git Hygiene Cleanup
| Item | Status | Action |
|------|--------|--------|
| `codegraph.json` | ⚠️ PENDING | `git rm --cached codegraph.json` (4.6MB) |
| `tarpaulin-report.html` | ⚠️ PENDING | `git rm --cached tarpaulin-report.html` (9.9MB) |
| `hello` binary | ⚠️ PENDING | Check if exists, `git rm` if found |
| `hello_test` binary | ⚠️ PENDING | Check if exists, `git rm` if found |

### 2. Test Infrastructure Cleanup (93GB Potential Savings)
| Item | Status | Size | Action |
|------|--------|------|--------|
| 4-day incomplete test | ⚠️ PENDING | 74GB | Delete `4day_results_20260405_094416/` |
| v5 test old cycles | ⚠️ PENDING | 15GB | Compress/archive old cycles |
| Marathon old tests | ⚠️ PENDING | 1GB | Delete `marathon_20260402_230101/` |
| Early longrun dirs | ⚠️ PENDING | ~3GB | Delete Apr 1-2 directories |

### 3. Documentation Updates
| Item | Status | Action |
|------|--------|--------|
| README.md line count | ⚠️ PENDING | Update "~197k lines" to "~273k lines" |
| README.md test count | ⚠️ PENDING | Update "~6,400" to "7,391" |
| ARCHITECTURE_SUMMARY.md | ⚠️ PENDING | Update stale stats (line count, tool count) |

---

## 🟡 HIGH PRIORITY (This Sprint)

### 4. Configuration Consolidation
| Item | Status | Action |
|------|--------|--------|
| Too many TOML files | ⚠️ PENDING | 7 files remain, consolidate to 3-4 |
| Config validation tool | ⚠️ PENDING | Add `selfware --validate-config` |
| Auto-discovery | ⚠️ PENDING | Query endpoint for model/context |

### 5. Code Quality - Unwrap Reduction
| File | Unwrap Count | Priority |
|------|--------------|----------|
| `src/session/checkpoint.rs` | 150 | High |
| `src/agent/execution.rs` | 148 | High |
| `src/tools/file.rs` | 136 | High |
| `src/agent/task_runner.rs` | 101 | Medium |
| **Total production unwraps** | **~2,932** | Target: < 250 |

### 6. Supervisor Implementation
| Method | Status | Location |
|--------|--------|----------|
| `restart_child()` | ⚠️ STUB | `src/supervision/mod.rs:183` |
| `escalate()` | ⚠️ STUB | `src/supervision/mod.rs:187` |

---

## 🟢 MEDIUM PRIORITY (Next 2 Sprints)

### 7. Feature Completion
| Feature | Status | Action |
|---------|--------|--------|
| Browser automation | ❌ STUB | Implement or remove from docs |
| SWE-bench integration | ❌ STUB | Implement or remove from docs |
| Computer control (macOS) | ⚠️ PARTIAL | Complete window operations |
| Dream consolidation | ⚠️ STUB | Wire up LLM call or remove |
| Visual validation | ⚠️ STUB | Implement actual vision model |
| Batch mode | ⚠️ STUB | Implement or remove |

### 8. Error Handling Improvements
| Item | Status | Action |
|------|--------|--------|
| JSON Logic evaluation | ✅ DONE | Already implemented |
| Proper error types | ⚠️ PENDING | Reduce anyhow, use thiserror |
| Fuzz testing | ⚠️ PENDING | Add for Workflow DSL parser |

### 9. Dependency Audit
| Dependency | Status | Action |
|------------|--------|--------|
| `zstd` | ⚠️ VERIFY | Check if actually used |
| `opentelemetry-otlp` | ⚠️ VERIFY | Check if fully implemented |
| `libloading` | ✅ OK | Feature-gated for hot-reload |

---

## 🔵 LOW PRIORITY (This Quarter)

### 10. Architecture Improvements
| Item | Status | Action |
|------|--------|--------|
| Module size reduction | ⚠️ PENDING | Split `agent/mod.rs` (1000+ lines) |
| Feature flag audit | ⚠️ PENDING | CI matrix for feature combinations |
| Integration tests | ⚠️ PENDING | Add for feature-gated modules |

### 11. Documentation Cleanup
| Item | Status | Count | Action |
|------|--------|-------|--------|
| Stale markdown files | ⚠️ PENDING | ~40 | Archive to `docs/archive/` |
| Duplicate test scripts | ⚠️ PENDING | 5 | Consolidate |
| Branch cleanup policy | ⚠️ PENDING | - | Document in CONTRIBUTING.md |

### 12. Development Experience
| Item | Status | Action |
|------|--------|--------|
| Log rotation | ⚠️ PENDING | Auto-cleanup for long-running tests |
| Unified test orchestrator | ⚠️ PENDING | Consolidate test scripts |
| Config profiles | ⚠️ PENDING | Single config with profiles |

---

## ✅ COMPLETED (During This Session)

| Item | Date | Notes |
|------|------|-------|
| Endpoint verification | 2026-04-12 | 16 concurrent streams confirmed |
| selfware.toml updated | 2026-04-12 | Model path, context length fixed |
| Obsolete configs archived | 2026-04-12 | 3 configs moved to configs/obsolete/ |
| Stub documentation | 2026-04-12 | All placeholders clearly marked |
| Version bump | 2026-04-12 | 0.3.0-beta.1 |
| Changelog update | 2026-04-12 | Release notes added |
| Build fixes | 2026-04-12 | Clippy warnings resolved |
| Tests passing | 2026-04-12 | 7,391 tests pass |

---

## 📊 Summary Statistics

| Category | Count |
|----------|-------|
| 🔴 Critical | 3 items |
| 🟡 High Priority | 3 items |
| 🟢 Medium Priority | 3 items |
| 🔵 Low Priority | 3 items |
| ✅ Completed | 8 items |

---

## 🎯 Next Steps Recommendation

1. **Immediate (Today):** Clean up git-tracked binaries and large files
2. **This Week:** Reduce unwrap count in top 3 files, update README.md
3. **This Sprint:** Archive old test data (93GB savings), consolidate configs
4. **Next Sprint:** Complete supervisor implementation, reduce unwrap count to <250
