#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RESULTS_DIR="${RESULTS_DIR_OVERRIDE:-$ROOT_DIR/long_run_tests/greenfield_batch_$(date +%Y%m%d_%H%M%S)}"
BINARY_PATH="$ROOT_DIR/target/debug/selfware"
CONFIG_PATH="$ROOT_DIR/selfware.toml"
PASS1_TIMEOUT="${PASS1_TIMEOUT:-150}"
PASS2_TIMEOUT="${PASS2_TIMEOUT:-120}"
PROJECTS_STRING="${PROJECTS:-slugger money_splitter bounded_queue}"

mkdir -p "$RESULTS_DIR"

strip_ansi() {
  sed -r 's/\x1B\[[0-9;]*[A-Za-z]//g'
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

prompt_for_project() {
  local project="$1"
  case "$project" in
    slugger)
      cat <<'EOF'
Build a Rust library in `src/lib.rs` that exposes:

- `pub fn slugify(input: &str) -> String`

Behavior:
- lowercase letters
- keep ASCII letters and digits
- convert runs of whitespace, `_`, or `-` into a single `-`
- strip punctuation
- trim leading/trailing separators

Write at least 5 unit tests in `src/lib.rs`.
Run `cargo test` until it passes.
EOF
      ;;
    money_splitter)
      cat <<'EOF'
Build a Rust library in `src/lib.rs` that exposes:

- `pub fn split_amount_cents(total: i64, people: usize) -> Result<Vec<i64>, &'static str>`

Behavior:
- reject negative totals
- reject zero people
- split cents as evenly as possible
- distribute any remainder by adding 1 cent to the earliest entries
- preserve the total exactly

Write at least 5 unit tests in `src/lib.rs`.
Run `cargo test` until it passes.
EOF
      ;;
    bounded_queue)
      cat <<'EOF'
Build a Rust library in `src/lib.rs` with:

- `pub struct BoundedQueue<T>`
- `new(capacity: usize) -> Self`
- `push(&mut self, value: T)`
- `pop(&mut self) -> Option<T>`
- `peek(&self) -> Option<&T>`
- `len(&self) -> usize`
- `is_empty(&self) -> bool`

Behavior:
- capacity 0 keeps nothing
- when full, `push` evicts the oldest item
- preserve FIFO order otherwise

Write at least 5 unit tests in `src/lib.rs`.
Run `cargo test` until it passes.
EOF
      ;;
    *)
      echo "Unknown project: $project" >&2
      return 1
      ;;
  esac
}

write_initial_prompt() {
  local project="$1"
  printf '%s\n' "You are working locally inside this project: $project."
  printf '\n'
  cat <<'EOF'
Use only these tools:
- `file_read`
- `file_edit`
- `file_write`
- `shell_exec`

Never call tool_search or context_bulk_read.

Read `Cargo.toml` and `src/lib.rs` once at the start.
Read `tests/e2e.rs` once at the start.
Before step 6, make a real file write.

Task:
EOF
  prompt_for_project "$project"
  printf '\n'
  printf '%s\n' 'Do not stop with advice. Make the edits locally.'
}

write_followup_prompt() {
  local project="$1"
  local cargo_output="$2"
  printf '%s\n' "Continue working locally inside this same project: $project."
  printf '\n'
  cat <<'EOF'
Use only:
- `file_read`
- `file_edit`
- `file_write`
- `shell_exec`

Never call tool_search or context_bulk_read.

Current `cargo test` output:
```text
EOF
  tail -n 100 "$cargo_output" | strip_ansi
  cat <<'EOF'
```

Next actions:
1. If needed, read `src/lib.rs` once.
2. If needed, read `tests/e2e.rs` once.
3. By step 3, make a real write to fix the failures.
4. Run `cargo test` again.
5. Do not finish until tests are green.

If `file_edit` fails because the target moved, switch to `file_write` with the full file content.
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
cat >"$summary_path" <<EOF
# Greenfield End-to-End Batch Summary

| Project | Passes Used | Final Cargo Exit | Tests Found | Src Changed | Max Step | Final Status |
|---|---:|---:|---:|---|---:|---|
EOF

for project in $PROJECTS_STRING; do
  project_dir="$RESULTS_DIR/$project"
  mkdir -p "$project_dir/src" "$project_dir/tests"

  cat >"$project_dir/Cargo.toml" <<EOF
[package]
name = "$project"
version = "0.1.0"
edition = "2021"
EOF

  cat >"$project_dir/src/lib.rs" <<'EOF'
// implement here
EOF

  case "$project" in
    slugger)
      cat >"$project_dir/tests/e2e.rs" <<'EOF'
use slugger::slugify;

#[test]
fn lowercases_ascii_letters() {
    assert_eq!(slugify("Hello World"), "hello-world");
}

#[test]
fn collapses_spaces_underscores_and_dashes() {
    assert_eq!(slugify("alpha___beta---gamma"), "alpha-beta-gamma");
}

#[test]
fn strips_punctuation() {
    assert_eq!(slugify("Rust, but fast!"), "rust-but-fast");
}

#[test]
fn trims_edge_separators() {
    assert_eq!(slugify("  --- spaced out --- "), "spaced-out");
}

#[test]
fn preserves_digits() {
    assert_eq!(slugify("Version 2 Release 5"), "version-2-release-5");
}
EOF
      ;;
    money_splitter)
      cat >"$project_dir/tests/e2e.rs" <<'EOF'
use money_splitter::split_amount_cents;

#[test]
fn splits_even_amounts() {
    assert_eq!(split_amount_cents(100, 4).unwrap(), vec![25, 25, 25, 25]);
}

#[test]
fn distributes_remainder_to_earliest_entries() {
    assert_eq!(split_amount_cents(10, 3).unwrap(), vec![4, 3, 3]);
}

#[test]
fn preserves_total_exactly() {
    let split = split_amount_cents(101, 4).unwrap();
    assert_eq!(split.iter().sum::<i64>(), 101);
}

#[test]
fn rejects_zero_people() {
    assert!(split_amount_cents(100, 0).is_err());
}

#[test]
fn rejects_negative_totals() {
    assert!(split_amount_cents(-1, 3).is_err());
}
EOF
      ;;
    bounded_queue)
      cat >"$project_dir/tests/e2e.rs" <<'EOF'
use bounded_queue::BoundedQueue;

#[test]
fn empty_queue_has_no_items() {
    let queue: BoundedQueue<i32> = BoundedQueue::new(3);
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
    assert_eq!(queue.peek(), None);
}

#[test]
fn preserves_fifo_order() {
    let mut queue = BoundedQueue::new(3);
    queue.push(1);
    queue.push(2);
    queue.push(3);
    assert_eq!(queue.pop(), Some(1));
    assert_eq!(queue.pop(), Some(2));
    assert_eq!(queue.pop(), Some(3));
    assert_eq!(queue.pop(), None);
}

#[test]
fn evicts_oldest_when_full() {
    let mut queue = BoundedQueue::new(2);
    queue.push(10);
    queue.push(20);
    queue.push(30);
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop(), Some(20));
    assert_eq!(queue.pop(), Some(30));
}

#[test]
fn capacity_zero_discards_everything() {
    let mut queue = BoundedQueue::new(0);
    queue.push(7);
    assert!(queue.is_empty());
    assert_eq!(queue.pop(), None);
}

#[test]
fn peek_reads_front_without_removing_it() {
    let mut queue = BoundedQueue::new(2);
    queue.push(5);
    queue.push(6);
    assert_eq!(queue.peek(), Some(&5));
    assert_eq!(queue.len(), 2);
    assert_eq!(queue.pop(), Some(5));
}
EOF
      ;;
  esac

  initial_src_hash="$(sha256sum "$project_dir/src/lib.rs" | awk '{print $1}')"

  write_initial_prompt "$project" >"$project_dir/pass1.prompt.md"
  run_pass "$project_dir" "$project_dir/pass1.prompt.md" "$project_dir/pass1.run.log" "$PASS1_TIMEOUT"
  run_cargo_test "$project_dir" "$project_dir/pass1.cargo_test.txt"

  passes_used=1
  final_cargo_output="$project_dir/pass1.cargo_test.txt"
  final_log_path="$project_dir/pass1.run.log"
  cargo_status="$(extract_status "$final_cargo_output")"

  if [[ "$cargo_status" != "0" ]]; then
    write_followup_prompt "$project" "$final_cargo_output" >"$project_dir/pass2.prompt.md"
    run_pass "$project_dir" "$project_dir/pass2.prompt.md" "$project_dir/pass2.run.log" "$PASS2_TIMEOUT"
    run_cargo_test "$project_dir" "$project_dir/pass2.cargo_test.txt"

    passes_used=2
    final_cargo_output="$project_dir/pass2.cargo_test.txt"
    final_log_path="$project_dir/pass2.run.log"
    cargo_status="$(extract_status "$final_cargo_output")"
  fi

  src_changed="no"
  if [[ "$(sha256sum "$project_dir/src/lib.rs" | awk '{print $1}')" != "$initial_src_hash" ]]; then
    src_changed="yes"
  fi

  tests_found="$(count_all_tests "$project_dir")"
  max_step="$(max_step_in_log "$final_log_path")"
  final_status="needs_work"
  if [[ "$cargo_status" == "0" && "$src_changed" == "yes" ]]; then
    final_status="complete"
  elif [[ "$src_changed" == "yes" ]]; then
    final_status="edited_but_failing"
  fi

  printf '| %s | %s | %s | %s | %s | %s | %s |\n' \
    "$project" \
    "$passes_used" \
    "$cargo_status" \
    "$tests_found" \
    "$src_changed" \
    "${max_step:-0}" \
    "$final_status" \
    >>"$summary_path"
done

printf '%s\n' "$RESULTS_DIR"
