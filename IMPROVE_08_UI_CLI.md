# IMPROVE-08 — UI / CLI Hardening & Headless-Mode Fixes

> **Scope:** `src/cli/*`, `src/ui/*`, `src/output/*`, `src/interview.rs`, `src/templates.rs`, `src/main.rs`, `src/bin/*`, `src/vlm_bench/*`  
> **Goal:** Fix CLI parsing bugs, TUI rendering crashes, input-handling flaws, output formatting that confuses LLMs/users, missing SWE-bench automation features, interview/template bugs, and headless (`--no-tui --quiet`) mode breakers.

---

## 1. CLI Parsing & Validation

### 1.1 `output_format` is an unvalidated `String`
- **Location:** `src/cli/args.rs:110`, `src/cli/mod.rs:565-566`
- **Problem:** `Cli.output_format` is declared as `String` with `default_value = "text"`. The `OutputFormat` enum (`Text` / `Json`) exists but is **never used** by the CLI struct. In `cli/mod.rs` the code does raw string comparison:
  ```rust
  let is_json = cli.output_format == "json";
  let is_stream_json = cli.output_format == "stream-json";
  ```
  Any typo (e.g. `--output-format jsob`) silently falls through to plain text instead of erroring.
- **Impact:** Silent mis-configuration in CI / automation scripts. LLM-driven tooling that generates flags may produce invalid values without noticing.
- **Fix:** Change `Cli.output_format` to `OutputFormat` and add a `stream_json` boolean flag, **or** keep the string but validate it explicitly in `cli::run()` and return an error for unknown values.

### 1.2 No test coverage for `--output-format` validation
- **Location:** `src/cli/tests.rs`
- **Problem:** The test file covers `chat -y`, `run -y`, etc., but there are zero tests for `--output-format json`, `--output-format stream-json`, or invalid values.
- **Impact:** Regressions in headless contract go undetected.
- **Fix:** Add `cli_output_format_json`, `cli_output_format_invalid`, and `cli_quiet_with_tui_conflict` tests.

### 1.3 `--quiet` does **not** block TUI spawning
- **Location:** `src/cli/mod.rs:622`
- **Problem:**
  ```rust
  let should_use_tui = cli.tui || (cli.command.is_none() && !cli.no_tui);
  ```
  `cli.quiet` is never part of the predicate. Running `selfware --quiet` (no subcommand) therefore enters the TUI dashboard, which immediately crashes because the terminal is not interactive.
- **Impact:** Headless / CI usage is broken when users naturally combine `--quiet` with no subcommand.
- **Fix:** Change the predicate to:
  ```rust
  let should_use_tui = !cli.quiet && (cli.tui || (cli.command.is_none() && !cli.no_tui));
  ```

---

## 2. Headless / Structured Output

### 2.1 `emit_event()` prints unlocked stdout
- **Location:** `src/cli/headless.rs:133`
- **Problem:** `emit_event` is marked `#[allow(dead_code)]` and uses unguarded `println!("{}", json)`. It does not acquire `output::OUTPUT_LOCK`, so JSON lines can interleave with spinner output or concurrent tool summaries.
- **Impact:** Corrupted JSONL stream when `--output-format stream-json` is used alongside progress emitters.
- **Fix:** Either remove `emit_event` (it appears unused) or make it acquire `OUTPUT_LOCK` and write to stdout with a trailing newline, consistent with `emit_result`.

### 2.2 Headless path still renders decorative header when not structured
- **Location:** `src/cli/mod.rs:569-576`
- **Problem:** When `-p` is provided and `!is_structured && !cli.quiet`, the code prints `render_header(&ctx)` and a "Headless Mode" subtitle. In compact/quiet automation contexts this decorative chrome is noise.
- **Impact:** Scripts that parse plain-text output get polluted with Unicode borders and emoji.
- **Fix:** Skip the decorative header entirely in headless mode; only emit the final answer / result. If a header is desired, gate it on `!compact && !quiet`.

### 2.3 `build_session_result` silently swallows `capture_patch` errors
- **Location:** `src/cli/mod.rs:708`
- **Problem:** `headless::capture_patch().unwrap_or_default()` hides git errors. In a CI sandbox without git, the patch is empty and the caller has no idea why.
- **Impact:** SWE-bench harness may report "no patch" even though the agent wrote files.
- **Fix:** Log the error (to stderr, not stdout) and include an `patch_error: Option<String>` field in `SessionResult` so the harness can surface it.

---

## 3. TUI Rendering & Layout

### 3.1 Hardcoded box width breaks on narrow / wide terminals
- **Location:** `src/ui/components.rs:78`
- **Problem:** `render_header()` hardcodes `width = 65`. On terminals narrower than 65 columns the border overflows; on wider terminals the box looks tiny and off-center.
- **Impact:** Visual glitches, line wrapping that breaks box-drawing alignment.
- **Fix:** Query `terminal::size()` and clamp to `min(max_width, term_width.saturating_sub(4))`.

### 3.2 `render_box()` uses byte-based `len()` instead of display width
- **Location:** `src/ui/components.rs` (internal `render_box` helper)
- **Problem:** Padding is computed with `line.len()` (byte length) rather than `line.chars().count()` or a proper Unicode display-width crate. Multi-byte characters (emoji, CJK) cause misaligned right borders.
- **Impact:** Broken borders on any non-ASCII content.
- **Fix:** Replace `len()` with `chars().count()` as a stop-gap, or adopt `unicode-width` for correctness.

### 3.3 Sticky bar emits ANSI sequences without capability checks
- **Location:** `src/ui/sticky_bar.rs:267-273`
- **Problem:** `StickyBar::update()` writes `\x1b7\x1b[{};1H...` unconditionally. If stdout is piped or the terminal does not support cursor-positioning sequences, the output is garbage.
- **Impact:** Pollutes logs and breaks piping (`selfware ... | tee log.txt`).
- **Fix:** Gate sticky-bar activation on `stdout().is_terminal() && supports_ansi()` (the same checks `TerminalSpinner` uses).

### 3.4 TUI dashboard does not handle terminal resize
- **Location:** `src/ui/tui.rs` (inferred from `components.rs` hardcoded width)
- **Problem:** Because `render_header` hardcodes 65, and the dashboard likely uses fixed-size layouts, resizing the terminal while the TUI is open will clip or wrap content.
- **Impact:** Broken UI on window resize.
- **Fix:** Listen for `crossterm::event::Event::Resize` and re-flow all dashboard panels.

---

## 4. Input Handling & Terminal State

### 4.1 Raw mode is not protected by RAII guard
- **Location:** `src/ui/selections.rs:886-888`, `src/interview.rs` (similar pattern)
- **Problem:**
  ```rust
  terminal::enable_raw_mode()?;
  let result = read_raw_line();
  let _ = terminal::disable_raw_mode();
  ```
  If `read_raw_line()` panics (e.g. from a downstream `unwrap`), `disable_raw_mode` never runs and the terminal stays in raw mode.
- **Impact:** User must run `reset` manually; CI jobs may hang.
- **Fix:** Use a small RAII struct:
  ```rust
  struct RawModeGuard;
  impl Drop for RawModeGuard { fn drop(&mut self) { let _ = terminal::disable_raw_mode(); } }
  ```

### 4.2 Interview enters raw mode even when stdin is not a TTY
- **Location:** `src/interview.rs` (implied from `selections.rs` pattern)
- **Problem:** `read_line_or_esc` in `selections.rs` checks `stdin.is_terminal()`, but the higher-level `run_interview` does not gate the entire interview. If an upstream caller invokes `run_interview` in a headless context, raw mode is entered and immediately fails or behaves oddly.
- **Impact:** Crash or unresponsive behavior in CI / Docker.
- **Fix:** Add an early `if !stdin.is_terminal() { return Ok(InterviewContext::default()); }` bail-out at the top of `run_interview`, or accept answers from env vars / CLI flags.

### 4.3 `SelfwareEditor` (reedline) is never used in headless mode but is compiled unconditionally
- **Location:** `src/input/mod.rs`
- **Problem:** The entire `reedline`-based input system (completer, highlighter, prompt) is compiled even when `--no-tui` is passed. This bloats the binary and introduces unused dependencies in headless builds.
- **Impact:** Slower compile times, larger binary.
- **Fix:** Gate `src/input` behind `#[cfg(feature = "tui")]` or a new `interactive` feature.

---

## 5. Output Formatting & Emoji / Unicode Fallbacks

### 5.1 `output/mod.rs` has no global `quiet` flag
- **Location:** `src/output/mod.rs`
- **Problem:** The output module tracks `compact`, `verbose`, and `show_tokens`, but **not** `quiet`. Consequently functions like `step_start`, `phase_transition`, `print_token_usage`, and `thinking` have no single flag to respect. Many UI helpers (`banners`, `mascot`, `task_display`) print directly to stdout without ever consulting the output module.
- **Impact:** `--quiet` is leaky; dozens of code paths still emit emoji, borders, and ANSI.
- **Fix:** Add `static QUIET_MODE: AtomicBool` and an `is_quiet()` helper. Make every public `output::*` function return early when `is_quiet()` is true.

### 5.2 `TerminalSpinner` ignores `--quiet`
- **Location:** `src/ui/spinner.rs:58`
- **Problem:** `start()` checks `is_tui_active`, `is_compact`, `is_terminal`, `supports_ansi`, but never `is_quiet`.
- **Impact:** Spinner lines appear in `--quiet` logs, confusing LLM parsers.
- **Fix:** Add `|| output::is_quiet()` to the early-return condition.

### 5.3 Garden seasons, prompt, mascot, banners, and swarm viz use hardcoded emoji
- **Locations:**
  - `src/ui/garden.rs:216` (`Season::glyph`)
  - `src/ui/prompt.rs:48` (`fox()`, `garden_glyph()`)
  - `src/ui/mascot.rs:148` (`render_inline_mascot`)
  - `src/ui/banners.rs` (all banner literals)
  - `src/ui/swarm_viz.rs:105` (`role_icon`)
  - `src/ui/task_display.rs` (`render_detailed_status` uses `🦊`, `📊`, etc.)
- **Problem:** These bypass the `Glyphs` ASCII-mode system in `style.rs`. When `--ascii` or `NO_COLOR=1` is set, emoji still appear.
- **Impact:** Broken rendering in restricted terminals (Windows CMD, serial consoles, CI logs). Some emoji are double-width and misalign borders.
- **Fix:** Convert all hardcoded emoji to `Glyphs::*` methods (adding new variants where needed) and respect `is_ascii_mode()`.

### 5.4 Diff viewer emits ANSI unconditionally
- **Location:** `src/ui/diff_viewer.rs`
- **Problem:** `show_diff`, `show_inline_diff`, `show_creation`, and `show_deletion` apply `.green()`, `.red()`, `.dimmed()` without checking `is_compact()`, `is_quiet()`, or terminal capability.
- **Impact:** Piped diff output contains escape sequences; quiet mode still emits colored text.
- **Fix:** Add a `no_color` parameter or read a global flag, and return plain text when disabled.

### 5.5 VLM benchmark report uses emoji unconditionally
- **Location:** `src/vlm_bench/report.rs:124-131`
- **Problem:** `rating_with_emoji` hardcodes `🌸`, `🌿`, `🥀`, `❄️`.
- **Impact:** Benchmark markdown reports are unreadable in plain-text environments.
- **Fix:** Use `Glyphs::*` or plain-text labels when `ASCII_MODE` is active.

---

## 6. Configuration & Init Wizard

### 6.1 Init wizard writes to the wrong config file name
- **Location:** `src/cli/init_wizard.rs:212`
- **Problem:** The wizard writes to `~/.config/selfware/config.toml`, but `resolve_config_path()` and the normal `Config::load` search for `selfware.toml` (in CWD or via `SELFWARE_CONFIG`).
- **Impact:** Users run `selfware init`, answer questions, then run `selfware` and the config is ignored.
- **Fix:** Make the wizard write to `selfware.toml` in the current working directory, or align `Config::load` with the XDG path `~/.config/selfware/config.toml`. The two must match.

### 6.2 Wizard prints directly to stdout without respecting `--quiet`
- **Location:** `src/cli/init_wizard.rs:218-229`
- **Problem:** `println!` and `print!` are used for prompts and confirmations. No check for `quiet` mode.
- **Impact:** Non-interactive / automated runs block waiting for stdin because the prompt is printed anyway.
- **Fix:** Return an error (`--quiet requires non-interactive setup`) or accept `--yes` to skip prompts.

### 6.3 Template scaffolding uses `include_str!` with fragile relative paths
- **Location:** `src/templates.rs`
- **Problem:** `include_str!("../templates/rust/Cargo.toml.template")` is evaluated at compile time. If the `templates/` directory is missing or moved, the build fails with an opaque rustc error.
- **Impact:** Contributors who clone the repo without submodules or symlinks get a cryptic build failure.
- **Fix:** Either (a) commit a `build.rs` that checks template presence and emits a helpful error, or (b) embed templates via `rust-embed` so they are included automatically and can be loaded at runtime with a graceful fallback.

---

## 7. SWE-bench / Benchmark Automation

### 7.1 `BenchCommand::SwebenchPro` exists but is marked experimental
- **Location:** `src/cli/args.rs`
- **Problem:** The `SWEBench` subcommand docstring says "Experimental SWE-bench entrypoint (currently disabled)". There is no clear path to enable it, and no integration tests.
- **Impact:** SWE-bench automation is not actually usable.
- **Fix:** Remove the "disabled" wording, add a feature flag (`swebench`) if needed, and wire it to `bench_harness::swebench_pro` with clear env-var documentation (`SELFWARE_RESULT_DIR`, etc.).

### 7.2 `vlm_bench` binary prints directly to stdout/stderr
- **Location:** `src/bin/vlm_bench_run.rs:81-89`
- **Problem:** The runner uses `println!` for its banner. It does not respect the unified output module.
- **Impact:** Inconsistent UX; JSON/structured consumers see plain text.
- **Fix:** Use `tracing::info!` or the output module's helpers.

### 7.3 VLM benchmark lacks `--quiet` / `--json` output mode
- **Location:** `src/bin/vlm_bench_run.rs`
- **Problem:** No machine-readable summary flag; the only structured output is files on disk.
- **Impact:** Hard to integrate into CI pipelines that expect exit codes and JSON on stdout.
- **Fix:** Add `--json` to print the report JSON to stdout and suppress the banner.

---

## 8. Shutdown & Process Lifecycle

### 8.1 Force-exit aborts natural cleanup
- **Location:** `src/main.rs:19`
- **Problem:** After `SHUTDOWN_GRACE_SECS` the signal handler calls `std::process::exit(1)`. This aborts the process without running `Drop` guards (e.g. raw-mode restore, temporary file cleanup, checkpoint flush).
- **Impact:** Terminal left in raw mode; temp files leaked; TUI state not restored.
- **Fix:** Instead of `exit(1)`, set an `AtomicBool` (`FORCE_EXIT`) and let `cli::run()` poll it. If the grace period expires, return an error that causes `main()` to exit with a non-zero `ExitCode`. This keeps the async runtime alive long enough for destructors to run.

### 8.2 `main.rs` does not propagate `ExitCode` on force-exit
- **Location:** `src/main.rs:33-39`
- **Problem:** The graceful-path returns `ExitCode`, but the force-exit path uses `std::process::exit(1)` which bypasses Rust's `ExitCode` abstraction.
- **Impact:** Inconsistent exit-code reporting on Windows.
- **Fix:** Remove `std::process::exit(1)` entirely; use `ExitCode::FAILURE`.

---

## 9. Minor / Cosmetic

| # | Location | Problem | Suggested Fix |
|---|----------|---------|---------------|
| 9.1 | `src/ui/selections.rs:854` | `compute_content_width` uses `.len()` (bytes) not display width | Use `chars().count()` |
| 9.2 | `src/ui/swarm_viz.rs:639` | `display_width` is a stub (`chars().count()`); emoji misalignment | Adopt `unicode-width` crate |
| 9.3 | `src/input/command_registry.rs` | `Navigation` category has zero commands | Either add a nav command or remove the variant |
| 9.4 | `src/ui/animations.rs` | `SPINNER_CLOCK`, `SPINNER_GARDEN`, `SPINNER_MOON` are emoji-only | Provide ASCII variants or fall back to `SPINNER_LINE` |
| 9.5 | `src/bin/codegraph.rs` | Assumes `src/` exists in CWD; no error context | Check `src_dir.exists()` with a helpful message |
| 9.6 | `src/bin/swarm_folder_analyzer.rs` | Overwrites `RECOMMENDED_FOLDERS.md` silently | Add `--output` flag and confirmation prompt |
| 9.7 | `src/vlm_bench/scoring.rs:104` | JSON numeric values `1` vs `1.0` are treated as mismatch | Use `serde_json::Number` equality or tolerant parsing |

---

## Quick-Fix Priority Matrix

| Priority | Item | Effort | Files |
|----------|------|--------|-------|
| **P0** | Validate `output_format` & wire it to the enum | Small | `args.rs`, `mod.rs`, `tests.rs` |
| **P0** | Make `--quiet` block TUI spawning | Tiny | `mod.rs` |
| **P0** | Add `quiet` flag to `output/mod.rs` and make all emitters respect it | Medium | `output/mod.rs`, `spinner.rs` |
| **P1** | Fix init-wizard config path mismatch | Small | `init_wizard.rs` |
| **P1** | Replace hardcoded emoji with `Glyphs` system | Medium | `garden.rs`, `prompt.rs`, `mascot.rs`, `swarm_viz.rs`, `task_display.rs` |
| **P1** | RAII guard for raw mode | Small | `selections.rs`, `interview.rs` |
| **P1** | Remove / lock `emit_event` in headless.rs | Tiny | `headless.rs` |
| **P2** | Dynamic terminal width in `components.rs` | Small | `components.rs` |
| **P2** | Graceful shutdown without `process::exit` | Small | `main.rs` |
| **P2** | Add `--json` / `--quiet` to VLM bench binary | Small | `vlm_bench_run.rs`, `report.rs` |
| **P3** | Template compile-time fragility (`include_str!`) | Medium | `templates.rs`, `build.rs` |
| **P3** | Sticky-bar capability checks | Small | `sticky_bar.rs` |
| **P3** | Diff viewer `NO_COLOR` support | Small | `diff_viewer.rs` |

---

## How to Verify Fixes

1. **Headless contract test:**
   ```bash
   cargo test --test cli_tests -- cli_headless_json
   ```
   (to be added)

2. **Quiet-mode smoke test:**
   ```bash
   cargo run -- --quiet -p "hello" 2>/dev/null | wc -c
   # Should be ~0 bytes (or only the final JSON).
   ```

3. **ASCII-mode rendering test:**
   ```bash
   cargo test --lib ui::style::test_ascii_mode_toggle
   # plus new tests for garden, prompt, mascot
   ```

4. **TUI + quiet conflict test:**
   ```bash
   cargo run -- --quiet --tui -p "hello"
   # Should NOT spawn dashboard; should exit cleanly.
   ```

5. **Init wizard path test:**
   ```bash
   cargo run -- init
   ls -la selfware.toml   # should exist, not ~/.config/selfware/config.toml
   ```

---

*Document generated from static analysis of ~25 source files. All line numbers refer to commit `HEAD` as of the analysis date.*
