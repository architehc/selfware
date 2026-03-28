# Dependency Audit Report

**Date:** 2026-03-27  
**Project:** selfware v0.3.0  
**Tool:** cargo-audit + cargo-outdated

## Executive Summary

The dependency audit identified **5 security warnings** (all allowed) and **2 outdated direct dependencies** that should be updated. No critical vulnerabilities were found.

---

## Security Vulnerabilities (cargo-audit)

### Allowed Warnings (5 total)

#### 1. **bincode** - Unmaintained ⚠️
- **Versions affected:** 1.3.3, 2.0.1
- **RUSTSEC ID:** RUSTSEC-2025-0141
- **Status:** ALLOWED
- **Affected by:**
  - `selfware` directly uses 2.0.1
  - `syntect` and `hnsw_rs` use 1.3.3
- **Recommendation:** Upgrade to bincode 3.0.0 (already available, breaking API changes)
- **Impact:** Low - bincode is still functional, just not actively maintained

#### 2. **number_prefix** - Unmaintained
- **Version:** 0.4.0
- **RUSTSEC ID:** RUSTSEC-2025-0119
- **Status:** ALLOWED
- **Dependency chain:** `indicatif` → `hf-hub` → `tokenizers` → `selfware`
- **Recommendation:** Wait for `indicatif` to update dependencies
- **Impact:** Minimal - transitive dependency, no known security issues

#### 3. **paste** - Unmaintained
- **Version:** 1.0.15
- **RUSTSEC ID:** RUSTSEC-2024-0436
- **Status:** ALLOWED
- **Dependency chain:** Used by `tokenizers` and `macro_rules_attribute`
- **Recommendation:** Monitor for updates in `tokenizers` crate
- **Impact:** Minimal - procedural macro, no runtime security implications

#### 4. **yaml-rust** - Unmaintained
- **Version:** 0.4.5
- **RUSTSEC ID:** RUSTSEC-2024-0320
- **Status:** ALLOWED
- **Dependency chain:** `syntect` → `selfware`
- **Recommendation:** Consider replacing `syntect` with alternative syntax highlighting
- **Impact:** Low - syntect is also unmaintained but functional

#### 5. **bincode (again)** - Already listed above
- Note: Listed twice because it's used by multiple direct dependencies

---

## Outdated Dependencies (cargo-outdated)

### Direct Dependencies to Update

#### 1. **bincode** ⚠️ HIGH PRIORITY
- **Current:** 2.0.1
- **Latest:** 3.0.0
- **Status:** Outdated
- **Action Required:** Upgrade with breaking changes
- **Migration:** See bincode 3.0 migration guide - API changes include:
  - `Bincode` trait replaced with `Encode`/`Decode`
  - Configuration API restructured
  - `serde` feature flag removed (serde integration is now default)

#### 2. **metrics** ⚠️ MEDIUM PRIORITY
- **Current:** 0.21.0
- **Latest:** 0.24.3
- **Status:** Outdated
- **Action Required:** Upgrade with minor API changes
- **Migration:** Check metrics changelog for breaking changes between 0.21 → 0.24

---

## Dependency Health Metrics

| Category | Count | Status |
|----------|-------|--------|
| Total Dependencies | 648 | ✅ Scanned |
| Critical Vulnerabilities | 0 | ✅ None |
| High Vulnerabilities | 0 | ✅ None |
| Medium Vulnerabilities | 0 | ✅ None |
| Low Vulnerabilities | 0 | ✅ None |
| Unmaintained Warnings | 5 | ⚠️ Allowed |
| Outdated Direct Deps | 2 | ⚠️ Action needed |

---

## Recommended Actions

### Immediate (High Priority)

1. **Upgrade bincode to 3.0.0**
   ```bash
   # Update Cargo.toml
   bincode = { version = "3.0", features = ["serde"] }
   
   # Then update code:
   # - Replace Bincode trait with Encode/Decode
   # - Update configuration API
   # - Remove serde feature flag (now default)
   ```

2. **Upgrade metrics to 0.24.x**
   ```bash
   # Update Cargo.toml
   metrics = "0.24"
   metrics-exporter-prometheus = "0.12"  # Check compatibility
   
   # Review API changes in metrics crate
   ```

### Medium Priority

3. **Monitor unmaintained dependencies**
   - `syntect`: Consider alternatives like `syntect-next` or `comrak` for markdown
   - `tokenizers`: Monitor for paste macro updates
   - `indicatif`: Monitor for number_prefix updates

### Long-term Considerations

4. **Evaluate syntax highlighting alternatives**
   - `syntect` is unmaintained (uses yaml-rust)
   - Consider: `syntect-next`, `tree-sitter` based highlighters
   - Impact: Affects TUI syntax highlighting in code display

---

## Security Configuration

### Current .cargo/config.toml or Cargo.toml ignores
No explicit RUSTSEC ignores found in project configuration. All warnings are allowed by default.

### Recommended: Add explicit ignores (optional)
If you want to silence specific warnings:

```toml
[audit]
ignore = [
    "RUSTSEC-2025-0141",  # bincode unmaintained
    "RUSTSEC-2025-0119",  # number_prefix unmaintained
    "RUSTSEC-2024-0436",  # paste unmaintained
    "RUSTSEC-2024-0320",  # yaml-rust unmaintained
]
```

---

## Verification Commands

```bash
# Run security audit
cargo audit

# Check for outdated dependencies
cargo outdated -R

# Check specific package
cargo outdated bincode -R
cargo outdated metrics -R

# Update dependencies
cargo update bincode
cargo update metrics

# Verify no new vulnerabilities
cargo audit
```

---

## Conclusion

The project has a **healthy dependency posture** with:
- ✅ No critical/high security vulnerabilities
- ✅ All warnings are for unmaintained (not vulnerable) crates
- ⚠️ 2 direct dependencies need updating (bincode, metrics)
- ⚠️ Some transitive dependencies use unmaintained crates (low risk)

**Priority:** Upgrade bincode to 3.0.0 first (breaking changes), then metrics to 0.24.x (minor changes).

---

## Appendix: Full Dependency Tree (Key Paths)

```
selfware 0.3.0
├── bincode 2.0.1 (OUTDATED → 3.0.0)
├── metrics 0.21.0 (OUTDATED → 0.24.3)
├── syntect 5.3.0
│   ├── bincode 1.3.3 (UNMAINTAINED)
│   └── yaml-rust 0.4.5 (UNMAINTAINED)
├── hnsw_rs 0.3.4
│   └── bincode 1.3.3 (UNMAINTAINED)
├── tokenizers 0.22.2
│   ├── hf-hub 0.4.3
│   │   └── indicatif 0.17.11
│   │       └── number_prefix 0.4.0 (UNMAINTAINED)
│   ├── paste 1.0.15 (UNMAINTAINED)
│   └── macro_rules_attribute 0.2.2
│       └── paste 1.0.15 (UNMAINTAINED)
└── [644 more dependencies...]
```

---

**Report generated:** 2026-03-27  
**Next audit recommended:** After upgrading bincode and metrics