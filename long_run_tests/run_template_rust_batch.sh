#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULTS_DIR="${RESULTS_DIR_OVERRIDE:-$ROOT_DIR/long_run_tests/template_batch_$(date +%Y%m%d_%H%M%S)}"
BINARY_PATH="$ROOT_DIR/target/debug/selfware"
CONFIG_PATH="$ROOT_DIR/selfware.toml"
TEMPLATES_ROOT="${TEMPLATES_ROOT_OVERRIDE:-$ROOT_DIR/system_tests/projecte2e/templates}"
PASS1_TIMEOUT="${PASS1_TIMEOUT:-720}"
PASS2_TIMEOUT="${PASS2_TIMEOUT:-480}"
TEMPLATE_PROJECTS_STRING="${TEMPLATE_PROJECTS:-viz_ascii_table hard_scheduler hard_event_bus viz_svg_chart viz_maze_gen}"

mkdir -p "$RESULTS_DIR"

strip_ansi() {
  sed -r 's/\x1B\[[0-9;]*[A-Za-z]//g'
}

dir_fingerprint() {
  local dir="$1"
  if [[ ! -d "$dir" ]]; then
    echo "missing"
    return
  fi

  local tmp
  tmp="$(mktemp)"
  (
    cd "$dir"
    find . -type f -print0 \
      | sort -z \
      | xargs -0 sha256sum
  ) >"$tmp"
  sha256sum "$tmp" | awk '{print $1}'
  rm -f "$tmp"
}

run_cargo_test() {
  local project_dir="$1"
  local output_path="$2"
  (
    set +e
    cd "$project_dir" && cargo test --quiet >"$output_path" 2>&1
    printf '%s\n' "$?" >"${output_path}.exit_code"
  )
}

extract_status() {
  local output_path="$1"
  if [[ -f "${output_path}.exit_code" ]]; then
    cat "${output_path}.exit_code"
  else
    echo 999
  fi
}

count_all_tests() {
  local project_dir="$1"
  rg -n '^\s*#\[test\]' "$project_dir/src" "$project_dir/tests" 2>/dev/null | wc -l | tr -d ' '
}

max_step_in_log() {
  local log_path="$1"
  if [[ ! -f "$log_path" ]]; then
    echo 0
    return
  fi
  grep -o 'Step [0-9]\+' "$log_path" | awk '{print $2}' | tail -1
}

copy_template() {
  local template_name="$1"
  local dest_dir="$2"
  mkdir -p "$dest_dir"
  rsync -a \
    --exclude target \
    --exclude .git \
    "$TEMPLATES_ROOT/$template_name/" \
    "$dest_dir/"
}

write_initial_prompt() {
  local template_name="$1"
  printf '%s\n' "You are working locally inside this template project: $template_name."
  printf '\n'
  cat <<'EOF'
Use only these tools:
- `file_read`
- `file_edit`
- `file_write`
- `shell_exec`

Never call `tool_search` or `context_bulk_read`.

Read `Cargo.toml`, the relevant files under `src/`, and the relevant files under `tests/` once at the start.
Before step 6, make a real write under `src/`.

Task:
- Fix the implementation so `cargo test` passes.
- Prefer editing only files under `src/`.
- Do not modify files under `tests/` unless the prompt explicitly says the tests are wrong. These tests are the specification.
- Only write short relative paths like `src/lib.rs` or `src/scheduler.rs`.
- Never create files under `tests/`, `target/`, or any path that repeats the workspace directory name.
- After changing code, run `cargo test`.
- Do not stop with advice. Make the edits locally.
EOF
}

write_followup_prompt() {
  local template_name="$1"
  local cargo_output="$2"
  printf '%s\n' "Continue working locally inside this same template project: $template_name."
  printf '\n'
  cat <<'EOF'
Use only:
- `file_read`
- `file_edit`
- `file_write`
- `shell_exec`

Never call `tool_search` or `context_bulk_read`.

Current `cargo test` output:
```text
EOF
  tail -n 120 "$cargo_output" | strip_ansi
  cat <<'EOF'
```

Next actions:
1. If needed, reread the relevant file under `src/` once.
2. If needed, reread the relevant file under `tests/` once.
3. By step 3, make a real write under `src/` to fix the failures.
4. Run `cargo test` again.
5. Do not finish until tests are green.

Rules:
- Do not rewrite the tests to make them easier.
- Only write short relative paths under `src/`.
- Never create files under `tests/`, `target/`, or any path that repeats the workspace directory name.
- If `file_edit` fails because the target moved, switch to `file_write` with the full file content.
EOF
}

run_pass() {
  local project_dir="$1"
  local prompt_path="$2"
  local log_path="$3"
  local timeout_secs="$4"
  local prompt

  prompt="$(cat "$prompt_path")"
  (
    set +e
    timeout "$timeout_secs" \
      "$BINARY_PATH" \
      --config "$CONFIG_PATH" \
      -C "$project_dir" \
      --daemon \
      --ascii \
      --no-color \
      -p "$prompt" \
      >"$log_path" 2>&1
    printf '%s\n' "$?" >"${log_path}.exit_code"
  )
}

summary_path="$RESULTS_DIR/SUMMARY.md"
cat >"$summary_path" <<'EOF'
# Template Rust Batch Summary

| Project | Passes Used | Final Cargo Exit | Tests Found | Src Changed | Tests Changed | Max Step | Final Status |
|---|---:|---:|---:|---|---|---:|---|
EOF

for template_name in $TEMPLATE_PROJECTS_STRING; do
  project_dir="$RESULTS_DIR/$template_name"
  copy_template "$template_name" "$project_dir"

  initial_src_hash="$(dir_fingerprint "$project_dir/src")"
  initial_tests_hash="$(dir_fingerprint "$project_dir/tests")"

  write_initial_prompt "$template_name" >"$project_dir/pass1.prompt.md"
  run_pass "$project_dir" "$project_dir/pass1.prompt.md" "$project_dir/pass1.run.log" "$PASS1_TIMEOUT"
  run_cargo_test "$project_dir" "$project_dir/pass1.cargo_test.txt"

  passes_used=1
  final_cargo_output="$project_dir/pass1.cargo_test.txt"
  final_log_path="$project_dir/pass1.run.log"
  cargo_status="$(extract_status "$final_cargo_output")"

  if [[ "$cargo_status" != "0" ]]; then
    write_followup_prompt "$template_name" "$final_cargo_output" >"$project_dir/pass2.prompt.md"
    run_pass "$project_dir" "$project_dir/pass2.prompt.md" "$project_dir/pass2.run.log" "$PASS2_TIMEOUT"
    run_cargo_test "$project_dir" "$project_dir/pass2.cargo_test.txt"

    passes_used=2
    final_cargo_output="$project_dir/pass2.cargo_test.txt"
    final_log_path="$project_dir/pass2.run.log"
    cargo_status="$(extract_status "$final_cargo_output")"
  fi

  src_changed="no"
  if [[ "$(dir_fingerprint "$project_dir/src")" != "$initial_src_hash" ]]; then
    src_changed="yes"
  fi

  tests_changed="no"
  if [[ "$(dir_fingerprint "$project_dir/tests")" != "$initial_tests_hash" ]]; then
    tests_changed="yes"
  fi

  tests_found="$(count_all_tests "$project_dir")"
  max_step="$(max_step_in_log "$final_log_path")"
  final_status="needs_work"
  if [[ "$cargo_status" == "0" && "$src_changed" == "yes" && "$tests_changed" == "no" ]]; then
    final_status="complete"
  elif [[ "$cargo_status" == "0" && "$tests_changed" == "yes" ]]; then
    final_status="green_but_test_tampered"
  elif [[ "$src_changed" == "yes" ]]; then
    final_status="edited_but_failing"
  fi

  printf '| %s | %s | %s | %s | %s | %s | %s | %s |\n' \
    "$template_name" \
    "$passes_used" \
    "$cargo_status" \
    "$tests_found" \
    "$src_changed" \
    "$tests_changed" \
    "${max_step:-0}" \
    "$final_status" \
    >>"$summary_path"
done

printf '%s\n' "$RESULTS_DIR"
