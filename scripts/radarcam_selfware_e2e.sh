#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${SELFWARE_BIN:-$ROOT_DIR/target/debug/selfware}"
CONFIG="${SELFWARE_CONFIG:-$ROOT_DIR/selfware-radarcam.toml}"
WORKDIR="${SELFWARE_WORKDIR:-/home/ivo/radarcam}"
VISION_IMAGE="${SELFWARE_VISION_IMAGE:-$WORKDIR/samples/sample_3_00000693.jpg}"
HEADLESS_FLAGS="${SELFWARE_HEADLESS_FLAGS:---yolo --ascii --compact}"
SESSION_LOG_DIR="${SELFWARE_SESSION_LOG_DIR:-$HOME/.local/share/selfware/session_logs}"
TMP_DIR="$(mktemp -d)"
TEXT_PROMPT="What is the default full validation CLI command for this workspace? Answer in one line."
TEXT_ANSWER_REGEX='python validate\.py'
VISION_PROMPT="Use vision_analyze on $VISION_IMAGE and answer with exactly one lowercase word naming the main subject from this set: aircraft, airplane, plane, jet, bird, insect, unknown."
VISION_ANSWER_REGEX='^(aircraft|airplane|plane|jet)[.!]?$'

cleanup() {
  rm -rf "$TMP_DIR"
}

trap cleanup EXIT

read -r -a HEADLESS_ARGV <<<"$HEADLESS_FLAGS"

strip_ansi_and_normalize() {
  perl -pe 's/\e\[[0-9;?]*[ -\/]*[@-~]//g; s/\r/\n/g' "$1"
}

session_log_for_prompt() {
  local marker="$1"
  local prompt="$2"

  while read -r _ path; do
    if rg -F -q "\"cwd\":\"$WORKDIR\"" "$path" \
      && rg -F -q "\"event_type\":\"task_start\"" "$path" \
      && rg -F -q "\"input\":\"$prompt\"" "$path"; then
      printf '%s\n' "$path"
      return 0
    fi
  done < <(find "$SESSION_LOG_DIR" -maxdepth 1 -type f -newer "$marker" -printf '%T@ %p\n' | sort -nr)
  return 1
}

extract_final_answer() {
  awk '
    NF {
      if ($0 == "[100% Done]") {
        print prev
        exit
      }
      prev = $0
    }
  ' "$1"
}

is_capability_disclaimer() {
  local lower="${1,,}"
  local markers=(
    "execute external tools"
    "execute system commands"
    "access local file system"
    "access local file systems"
    "access files on your local system"
    "run tools"
    "view images directly"
    "call tools"
    "only generate text responses"
    "only process and respond to the text"
    "information provided to me"
    "information provided directly"
  )
  local count=0
  local marker

  for marker in "${markers[@]}"; do
    if [[ "$lower" == *"$marker"* ]]; then
      ((count += 1))
    fi
  done

  ((count >= 2))
}

llm_doctor_warning_allowed() {
  case "$1" in
    *"Tool calling: not working or unsupported"*|*"Tool calling did not produce tool_calls"*|*"High latency — consider a faster backend or smaller model"*)
      return 0
      ;;
  esac

  return 1
}

check_llm_doctor_output() {
  local clean_output="$1"
  local warning

  while IFS= read -r warning; do
    [[ -z "$warning" ]] && continue
    if ! llm_doctor_warning_allowed "$warning"; then
      echo "Unexpected llm-doctor warning: $warning" >&2
      exit 1
    fi
  done < <(rg '!!' "$clean_output" || true)
}

run_headless_check() {
  local label="$1"
  local prompt="$2"
  local answer_regex="$3"
  local required_tool="${4:-}"
  local forbidden_regex="${5:-}"
  local marker output clean_output session_log answer

  marker="$(mktemp "$TMP_DIR/session_marker.XXXXXX")"
  output="$(mktemp "$TMP_DIR/${label}.stdout.XXXXXX")"
  clean_output="$(mktemp "$TMP_DIR/${label}.clean.XXXXXX")"

  "$BIN" "${HEADLESS_ARGV[@]}" -C "$WORKDIR" --config "$CONFIG" -p "$prompt" | tee "$output"

  strip_ansi_and_normalize "$output" >"$clean_output"

  session_log="$(session_log_for_prompt "$marker" "$prompt")"
  if [[ -z "${session_log:-}" ]]; then
    echo "Could not locate the session log for prompt: $prompt" >&2
    exit 1
  fi

  if ! rg -q '"event_type":"task_end".*"success":true' "$session_log"; then
    echo "Expected a successful task_end event in $session_log" >&2
    exit 1
  fi

  if [[ -n "$required_tool" ]] && ! rg -q "\"event_type\":\"tool_call\".*\"tool_name\":\"$required_tool\".*\"success\":true" "$session_log"; then
    echo "Expected a successful $required_tool tool call in $session_log" >&2
    exit 1
  fi

  answer="$(extract_final_answer "$clean_output")"
  if [[ -z "${answer// }" ]]; then
    echo "Could not extract a final answer for $label from $clean_output" >&2
    exit 1
  fi

  if is_capability_disclaimer "$answer"; then
    echo "Final answer for $label was a capability disclaimer: $answer" >&2
    exit 1
  fi

  if ! printf '%s\n' "$answer" | rg -qi "$answer_regex"; then
    echo "Final answer for $label did not match /$answer_regex/: $answer" >&2
    exit 1
  fi

  if [[ -n "$forbidden_regex" ]] && printf '%s\n' "$answer" | rg -qi "$forbidden_regex"; then
    echo "Final answer for $label matched forbidden /$forbidden_regex/: $answer" >&2
    exit 1
  fi

  echo "Verified $label in $session_log"
  echo "Final answer: $answer"
}

cargo build --bin selfware --manifest-path "$ROOT_DIR/Cargo.toml"

if [[ ! -f "$VISION_IMAGE" ]]; then
  echo "Vision sample image not found: $VISION_IMAGE" >&2
  exit 1
fi

echo "[1/5] validating config"
"$BIN" --config "$CONFIG" --validate-config

echo "[2/5] running Rust regression checks"
cargo test --manifest-path "$ROOT_DIR/Cargo.toml" --lib explicit_tool
cargo test --manifest-path "$ROOT_DIR/Cargo.toml" --lib capability_disclaimer
cargo test --manifest-path "$ROOT_DIR/Cargo.toml" --lib extract_mentioned_path_finds_absolute_markdown_file

echo "[3/5] diagnosing endpoint"
LLM_DOCTOR_OUTPUT="$(mktemp "$TMP_DIR/llm_doctor.stdout.XXXXXX")"
LLM_DOCTOR_CLEAN="$(mktemp "$TMP_DIR/llm_doctor.clean.XXXXXX")"
"$BIN" --config "$CONFIG" llm-doctor | tee "$LLM_DOCTOR_OUTPUT"
strip_ansi_and_normalize "$LLM_DOCTOR_OUTPUT" >"$LLM_DOCTOR_CLEAN"
check_llm_doctor_output "$LLM_DOCTOR_CLEAN"

echo "[4/5] validating live text task"
run_headless_check "text_task" "$TEXT_PROMPT" "$TEXT_ANSWER_REGEX"

echo "[5/5] validating live vision task"
run_headless_check "vision_task" "$VISION_PROMPT" "$VISION_ANSWER_REGEX" "vision_analyze"
