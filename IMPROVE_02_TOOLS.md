# Selfware Tools Audit Report

> Scope: `src/tools/mod.rs`, `src/tools/file.rs`, `src/tools/file_read/mod.rs`, `src/tools/shell.rs`, `src/tools/grep_search/mod.rs`, `src/tools/search.rs`, `src/tools/cargo.rs`, `src/tools/git.rs`, `src/tools/lsp_tools.rs`, `src/tools/fim.rs`, `src/tools/context.rs`, `src/tools/codemap.rs`, `src/tools/tool_search.rs`, `src/tool_parser.rs`

---

## 1. BUGS & LOGIC ERRORS

### 1.1 `FileRead` is defined twice — stale-guard is broken
- **File:** `src/tools/file.rs` (lines 147–357) and `src/tools/file_read/mod.rs` (lines 24–121)
- **Issue:** Two separate `struct FileRead` types exist. `mod.rs` registers `file_read::FileRead` (line 70), but the stale-guard snapshot logic (`record_file_snapshot`) only lives in `file::FileRead` (line 334). The registered `file_read::FileRead` never records snapshots, so `is_file_stale()` always returns `None` after a read, rendering the stale-guard useless for the critical read path.
- **Fix:** Delete the `FileRead` implementation from `file.rs` (keep the snapshot helper functions). Move `record_file_snapshot` into `file_read/mod.rs` and call it after every successful read. Unify `resolve_safety_config` / `validate_tool_path` / `SAFETY_CONFIG` into a shared module to eliminate duplication.

### 1.2 `GrepSearch` is defined twice
- **File:** `src/tools/grep_search/mod.rs` (entire file) and `src/tools/search.rs` (lines 65–348)
- **Issue:** `grep_search/mod.rs` is a near-exact duplicate of the first half of `search.rs`. The registry imports `grep_search::GrepSearch` (mod.rs line 73), leaving `search::GrepSearch` as dead code. Both define competing `REGEX_CACHE` statics, wasting memory.
- **Fix:** Delete `src/tools/grep_search/mod.rs` and its module declaration in `mod.rs` (line 39). Re-export `GrepSearch` from `search.rs` if needed for backward compatibility.

### 1.3 `tool_search` placeholder is never upgraded to the real registry
- **File:** `src/tools/tool_search.rs` (lines 52–63) and `src/tools/mod.rs` (line 366)
- **Issue:** `ToolRegistry::new()` registers `ToolSearchTool::placeholder()`, whose inner `DummySearchable` always returns an empty `Vec`. There is no setter or replacement mechanism visible in the tools layer, so `tool_search` returns nothing unless the agent dispatch layer manually intercepts the call.
- **Fix:** Add a `set_registry()` method on `ToolSearchTool` (taking an `Arc<RwLock<dyn ToolSearchable>>`) and call it from `ToolRegistry::new()` after construction, passing `&self` wrapped appropriately.

### 1.4 `normalize_malformed_xml` corrupts JSON inside `<arguments>`
- **File:** `src/tool_parser.rs` (lines 186–195)
- **Issue:** The regex `(?m)(^|[^<\w/])(tool_call|arguments|parameter|function|tool|name)>` will rewrite valid JSON string values that happen to contain these words followed by `>`. Example:
  ```json
  {"message": "Pass arguments> here"}
  ```
  becomes:
  ```json
  {"message": "Pass </arguments> here"}
  ```
  which is invalid JSON and causes parse failure.
- **Fix:** Only run normalization on text that is *outside* of JSON object boundaries, or restrict the regex to known XML contexts (e.g., only when preceded by whitespace/newline and not inside `{}`).

### 1.5 `cargo_clippy` emits an illegal double `--` separator
- **File:** `src/tools/cargo.rs` (lines 411–425)
- **Issue:** When `deny_warnings` is true, the code adds `-- -D warnings`. It then unconditionally adds `-- -D clippy::unwrap_used -D clippy::expect_used`. Cargo passes everything after the *first* `--` to rustc; a second `--` becomes a literal `--` argument to rustc, which errors or is ignored.
- **Fix:** Build the lint arguments in a single `Vec` and append them after a single `--`:
  ```rust
  let mut lints = vec!["--"];
  if deny_warnings { lints.extend(["-D", "warnings"]); }
  lints.extend(["-D", "clippy::unwrap_used", "-D", "clippy::expect_used"]);
  cmd.args(lints);
  ```

### 1.6 Panic location extraction in `parse_test_output` is completely wrong
- **File:** `src/tools/cargo.rs` (lines 606–611)
- **Issue:** The code finds the first `'` and last `'` in the panic line:
  ```rust
  if let Some(loc_start) = panic_line.find('\'') {
      if let Some(loc_end) = panic_line.rfind('\'') {
          failure.location = Some(panic_line[loc_start + 1..loc_end].to_string());
      }
  }
  ```
  For `thread 'tests::foo' panicked at 'assertion failed', src/lib.rs:10:5`, this extracts `tests::foo' panicked at 'assertion failed` instead of `src/lib.rs:10:5`.
- **Fix:** Parse the comma-separated tail of the panic line with a regex such as `r"panicked at .*?, ([^,]+:\d+:\d+)"`.

### 1.7 `GitDiff` schema lies about the `path` parameter
- **File:** `src/tools/git.rs` (lines 365–374, 376–398)
- **Issue:** The schema says `path: "Specific file or directory"`, but the implementation uses it as the *repo* path (`cmd.arg("-C").arg(repo_path).arg("diff")`). The actual file to diff is never passed to `git diff`. Small LLMs that pass a file path get the entire repo diff instead.
- **Fix:** Accept a separate `repo_path` parameter (default `.`) and pass the user-supplied `path` as a trailing pathspec to `git diff`.

### 1.8 `GlobFind` does not support `**` recursion as claimed
- **File:** `src/tools/search.rs` (lines 360–381, 398–445)
- **Issue:** The tool description says `"Glob pattern (e.g., *.rs, src/**/*.ts)"`, but the code uses `glob::Pattern`, which only supports `*` in a single path segment (`**` is treated as two literal stars). `src/**/*.ts` will never match `src/a/b/c.ts`.
- **Fix:** Use `globset::Glob` (which supports `**`) or document that only single-segment wildcards work and recommend `grep_search` with `include` for recursive patterns.

### 1.9 `FileFimEdit` is missing from the registry
- **File:** `src/tools/fim.rs` (lines 95–272) and `src/tools/mod.rs`
- **Issue:** `FileFimEdit` is implemented but never registered in `ToolRegistry::new()`. It is unreachable by the LLM.
- **Fix:** Register it as a deferred tool in `ToolRegistry::new()`:
  ```rust
  registry.register_deferred(FileFimEdit::new(Arc::clone(&api_client)));
  ```
  (Requires passing the `ApiClient` into the registry constructor or using a lazy singleton.)

---

## 2. TODO / FIXME COMMENTS

No actionable `TODO`/`FIXME` markers were found in the audited source files. The only occurrences are inside prompt-string literals in `grep_search/prompt.rs` (examples for the LLM).

---

## 3. MISSING TOOL FEATURES (SWE-bench Impact)

### 3.1 `file_read` lacks pagination — large files blow up the context window
- **File:** `src/tools/file_read/mod.rs` (schema, lines 57–75)
- **Issue:** Unlike `shell_exec` (which has `output_offset`/`output_limit`), `file_read` returns the entire file content in one JSON blob. Reading a 10 MB log file or generated test output will OOM the agent loop.
- **Fix:** Add `offset` (char offset) and `limit` (max chars) parameters to the schema, and use `super::truncate_with_pagination()` (already in `mod.rs`) before returning.

### 3.2 `file_read` does not prefix line numbers
- **File:** `src/tools/file_read/mod.rs` (execute, lines 100–119)
- **Issue:** The raw content returned has no line numbers. When the LLM later calls `file_edit`, it must copy exact strings. Line numbers in the output would reduce mismatches.
- **Fix:** Add an optional `show_line_numbers: bool` parameter (default `false`). When true, prepend `"  1 | "`, `"  2 | "`, etc.

### 3.3 No language-agnostic test runner
- **File:** `src/tools/cargo.rs`
- **Issue:** SWE-bench includes Python, JS, and Go repositories. There is no `pytest`, `npm test`, or `go test` tool. The LLM is forced to use `shell_exec`, losing structured output (pass/fail counts, failure locations).
- **Fix:** Add `python_test`, `npm_test`, and `go_test` deferred tools with minimal structured parsing (exit code + stdout/stderr + regex-based failure extraction).

### 3.4 `symbol_search` is Rust-only
- **File:** `src/tools/search.rs` (lines 464–600)
- **Issue:** `SymbolSearch` only walks `.rs` files and uses Rust-specific regexes. It is useless for Python SWE-bench tasks.
- **Fix:** Detect the project language from file extensions (`.py`, `.go`, `.ts`) and switch regex sets accordingly, or use a lightweight tree-sitter backend.

### 3.5 `directory_tree` returns a flat list, not a tree
- **File:** `src/tools/file.rs` (lines 796–919)
- **Issue:** The output is a single `entries` array. For deep hierarchies the LLM loses parent/child relationships.
- **Fix:** Return a nested JSON tree (`children: [...]`) or indent a text tree in a `tree` field while keeping `entries` for backward compatibility.

### 3.6 `git_log` is referenced in the parser but not implemented
- **File:** `src/tool_parser.rs` (line 744) and `src/tools/mod.rs`
- **Issue:** `KNOWN_TOOLS` in the fallback parser lists `git_log`, but no such tool exists in the registry.
- **Fix:** Implement a lightweight `git_log` tool (last N commits, optional file filter) or remove it from `KNOWN_TOOLS` to avoid parser hallucinations.

---

## 4. SAFETY GAPS

### 4.1 `shell_exec` does not validate `cwd` against `SafetyConfig`
- **File:** `src/tools/shell.rs` (lines 93–103)
- **Issue:** The tool checks that `cwd` is absolute and contains no `..`, but it never calls `validate_tool_path(cwd, &safety)`. A compromised LLM can set `cwd` to `/etc` and run `cat passwd`.
- **Fix:** Call `crate::tools::file::validate_tool_path(cwd, &resolve_safety_config(...))` before executing.

### 4.2 `find_dangerous_shell_pattern` is trivially bypassed
- **File:** `src/tools/mod.rs` (lines 125–143)
- **Issue:** The pattern list is tiny (`/dev/tcp/`, `| bash -i`, `mkfifo /tmp`). It does not catch `curl … | sh`, `nc -e`, `python -c 'import socket,subprocess;…'`, or simple `rm -rf /`.
- **Fix:** Expand the deny-list to include `| sh`, `| bash`, `curl *|*`, `wget *|*`, `nc -e`, `rm -rf /`, `> /etc/passwd`, etc. Consider migrating to a block-list regex set instead of substring checks.

### 4.3 LSP tools bypass path safety checks
- **File:** `src/tools/lsp_tools.rs` (lines 116–148, 191–216, 251–272, 315–343)
- **Issue:** None of the four LSP tools call `validate_tool_path` on the `file` argument. They read arbitrary paths from disk and open them in the LSP client.
- **Fix:** Add `validate_tool_path(&args.file, &resolve_safety_config(None))?` at the top of each `execute` method.

### 4.4 `FileFimEdit` uses non-atomic write
- **File:** `src/tools/fim.rs` (line 263)
- **Issue:** All other file write tools use `write_atomic` (tempfile + rename). `FileFimEdit` uses `fs::write`, leaving the file half-written if the process crashes.
- **Fix:** Replace `fs::write(path, &final_content)` with `crate::tools::file::write_atomic(Path::new(path), &final_content).await?`.

### 4.5 `GitPush` allows `--force` at the tool level
- **File:** `src/tools/git.rs` (lines 525–604)
- **Issue:** The schema says "Force push (blocked by safety checker)" but the tool happily adds `--force` to the command. If the safety checker is disabled or misconfigured, data loss is one tool call away.
- **Fix:** Reject `force: true` inside the tool itself with a clear error: `anyhow::bail!("Force push is not allowed.");`.

### 4.6 `GitCommit` path validation is overly broad and under-validated
- **File:** `src/tools/git.rs` (lines 459–463)
- **Issue:** `f.contains("..")` rejects legitimate filenames like `foo..bar.rs`. It also does not call `validate_tool_path` on individual files.
- **Fix:** Use `validate_tool_path(f, &safety)` for each file, and only reject actual path-traversal components via `Path::components()`.

---

## 5. PERFORMANCE ISSUES

### 5.1 `grep_search` / `search.rs` allocates a `Vec<&str>` for every file
- **File:** `src/tools/search.rs` (line 248)
- **Issue:** `let lines: Vec<&str> = content.lines().collect();` allocates a vector of line references for every file searched. For a 1 MB file this is ~20,000 allocations.
- **Fix:** Use `content.lines().enumerate()` directly in the match loop without collecting.

### 5.2 `sanitize_fim_instruction` recompiles 15+ regexes on every call
- **File:** `src/tools/fim.rs` (lines 33–90)
- **Issue:** Each FIM edit compiles fresh `Regex` objects for static token lists and injection patterns. Regex compilation is expensive.
- **Fix:** Pre-compile the patterns in a `Lazy<Regex>` set:
  ```rust
  static FIM_TOKEN_RE: Lazy<Regex> = Lazy::new(|| {
      Regex::new(r"(?i)<\|fim_prefix\|>|\|fim_suffix\|>|...").unwrap()
  });
  ```
  Then apply a single `replace_all` pass.

### 5.3 `parse_test_output` allocates a 32 MB string for large cargo output
- **File:** `src/tools/cargo.rs` (line 545)
- **Issue:** `let combined = format!("{}\n{}", stdout, stderr);` clones both 16 MB buffers into a single 32+ MB `String`.
- **Fix:** Iterate over `stdout.lines().chain(stderr.lines())` instead of concatenating.

### 5.4 `ToolRegistry::search` builds many temporary lowercase strings
- **File:** `src/tools/mod.rs` (lines 665–687)
- **Issue:** `info.tool.name().to_lowercase()` and `info.tool.description().to_lowercase()` are called for *every* tool on every search, allocating twice per tool.
- **Fix:** Pre-compute lowercase name + description inside `ToolInfo` at registration time and store them as `String` fields.

### 5.5 `file.rs` `FileWrite` reads the existing file twice
- **File:** `src/tools/file.rs` (lines 415–430)
- **Issue:** The tool reads the file once to detect line endings and again to detect no-op writes.
- **Fix:** Read once, reuse the `existing` string for both checks.

---

## 6. SCHEMA / DESCRIPTION QUALITY ISSUES (Small-LLM Confusion)

### 6.1 `shell_exec` description encourages using it for builds/tests
- **File:** `src/tools/shell.rs` (line 30)
- **Current:** `"Execute shell command. Use for builds, tests, and system operations."`
- **Problem:** Small LLMs frequently call `shell_exec("cargo test")` instead of the dedicated `cargo_test` tool, losing structured failure parsing.
- **Fix:** `"Execute arbitrary shell commands. **Prefer dedicated tools** (cargo_test, cargo_check, grep_search) when available. Use this only for operations that have no dedicated tool."`

### 6.2 `file_read` schema omits pagination that `shell_exec` has
- **File:** `src/tools/file_read/mod.rs` (lines 57–75)
- **Problem:** LLMs learn that pagination exists for shell output; they may try to pass `offset`/`limit` to `file_read` and get schema-validation errors.
- **Fix:** Add `offset` and `limit` to the schema (see 3.1) so the interface is consistent.

### 6.3 `file_edit` description is intimidating but lacks actionable guidance
- **File:** `src/tools/file.rs` (line 462)
- **Current:** `"The old_str must match EXACTLY once. Include enough context to ensure unique match."`
- **Problem:** Small LLMs often forget that `old_str` must include surrounding whitespace/indentation exactly.
- **Fix:** Add concrete guidance: `"old_str must match the file content byte-for-byte, including indentation and trailing newlines. If the string appears more than once, make it unique by including 2–3 lines of surrounding context."`

### 6.4 LSP tools use zero-based coordinates without contrasting `file_read`
- **File:** `src/tools/lsp_tools.rs` (lines 89–93, 165–168, etc.)
- **Problem:** `file_read` uses 1-based line numbers. Small LLMs routinely pass 1-based lines to `lsp_goto_definition` and miss the target.
- **Fix:** Add a contrast sentence: `"Note: line and column are ZERO-based, unlike file_read which uses 1-based line numbers."`

### 6.5 `code_map` and `context_action` are silently Rust-only
- **File:** `src/tools/codemap.rs` (lines 375–380, 542–546)
- **Current descriptions:** Mention module paths like `tools::codemap` and file paths like `src/tools/codemap.rs`.
- **Problem:** In a Python repo, the LLM will pass `tools::codemap` and get an empty result with no explanation.
- **Fix:** Update descriptions to state: `"Currently optimized for Rust projects (scans src/ and .rs files). For Python/JS, use directory_tree and grep_search instead."`

### 6.6 `cargo_test` schema does not explain `test_name` substring matching
- **File:** `src/tools/cargo.rs` (lines 163–173)
- **Current:** `"Specific test to run (substring match)"`
- **Problem:** LLMs may pass a fully qualified name with module path and be surprised when cargo runs multiple tests matching the substring.
- **Fix:** `"Filter tests by substring (e.g., 'my_test' matches 'module::my_test' and 'module::my_test_extra'). To run exactly one test, use the full path."`

### 6.7 `tool_parser` `KNOWN_TOOLS` list is stale
- **File:** `src/tool_parser.rs` (lines 726–753)
- **Issue:** The fallback plain-function parser knows about `git_log` (which doesn't exist) but not about `file_fim_edit`, `lsp_goto_definition`, `patch_apply`, or `context_budget`.
- **Fix:** Synchronize the list with `ToolRegistry` (or generate it from the registry at compile time via a build script / macro).

### 6.8 `directory_tree` schema lacks explanation of `include_hidden` side effects
- **File:** `src/tools/file.rs` (lines 806–816)
- **Issue:** When `include_hidden` is `false` (default), hidden *directories* are not descended into. The schema description does not mention this.
- **Fix:** Change description to: `"If false, skips hidden files AND does not descend into hidden directories (e.g., .git, .github)."`

---

## Summary Table

| Priority | Issue | File | Line(s) | Type |
|----------|-------|------|---------|------|
| 🔴 High | Duplicate `FileRead` breaks stale-guard | `file.rs` / `file_read/mod.rs` | 147 / 24 | Bug |
| 🔴 High | `tool_search` placeholder never upgraded | `tool_search.rs` | 52–63 | Bug |
| 🔴 High | `normalize_malformed_xml` corrupts JSON | `tool_parser.rs` | 186–195 | Bug |
| 🔴 High | `cargo_clippy` double `--` separator | `cargo.rs` | 411–425 | Bug |
| 🔴 High | `FileFimEdit` not registered | `fim.rs` / `mod.rs` | 95 / — | Bug |
| 🟡 Med | `GrepSearch` duplicated | `grep_search/mod.rs` / `search.rs` | entire | Bug |
| 🟡 Med | Panic location extraction broken | `cargo.rs` | 606–611 | Bug |
| 🟡 Med | `GitDiff` schema vs implementation mismatch | `git.rs` | 365–398 | Bug |
| 🟡 Med | `GlobFind` `**` recursion claim is false | `search.rs` | 360–445 | Bug |
| 🟡 Med | `shell_exec` missing `SafetyConfig` cwd check | `shell.rs` | 93–103 | Safety |
| 🟡 Med | LSP tools missing path validation | `lsp_tools.rs` | 116–343 | Safety |
| 🟡 Med | `FileFimEdit` non-atomic write | `fim.rs` | 263 | Safety |
| 🟡 Med | `GitPush` allows force push | `git.rs` | 575–578 | Safety |
| 🟢 Low | `file_read` missing pagination | `file_read/mod.rs` | 57–75 | Feature |
| 🟢 Low | `file_read` missing line numbers | `file_read/mod.rs` | 100–119 | Feature |
| 🟢 Low | No Python/JS test runner | `cargo.rs` | — | Feature |
| 🟢 Low | `symbol_search` Rust-only | `search.rs` | 464–600 | Feature |
| 🟢 Low | `sanitize_fim_instruction` regex perf | `fim.rs` | 33–90 | Perf |
| 🟢 Low | `parse_test_output` 32 MB alloc | `cargo.rs` | 545 | Perf |
| 🟢 Low | Schema description inconsistencies | multiple | — | Schema |
