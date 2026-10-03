#!/usr/bin/env bash
# =============================================================================
# Selfware Docker Validation — Full User Journey Test
# =============================================================================
# Tests the complete selfware user experience from installation through
# feature validation. Designed to run inside the validation Docker container.
#
# Usage:
#   docker build -f tests/Dockerfile.validation -t selfware-validation .
#   docker run --rm selfware-validation
#
# Exit codes:
#   0 — all tests passed
#   1 — one or more tests failed
# =============================================================================

set -euo pipefail

# ---------------------------------------------------------------------------
# Globals
# ---------------------------------------------------------------------------
PASS_COUNT=0
FAIL_COUNT=0
SKIP_COUNT=0
RESULTS=()
VALIDATION_DIR="/home/validator/validation-workspace"
SELFWARE_VERSION=""
START_TIME=$(date +%s)

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

record_pass() {
    local name="$1"
    local detail="${2:-}"
    PASS_COUNT=$((PASS_COUNT + 1))
    if [ -n "$detail" ]; then
        RESULTS+=("[PASS] ${name} — ${detail}")
    else
        RESULTS+=("[PASS] ${name}")
    fi
    echo "  [PASS] ${name} ${detail:+— $detail}"
}

record_fail() {
    local name="$1"
    local detail="${2:-}"
    FAIL_COUNT=$((FAIL_COUNT + 1))
    if [ -n "$detail" ]; then
        RESULTS+=("[FAIL] ${name} — ${detail}")
    else
        RESULTS+=("[FAIL] ${name}")
    fi
    echo "  [FAIL] ${name} ${detail:+— $detail}"
}

record_skip() {
    local name="$1"
    local detail="${2:-}"
    SKIP_COUNT=$((SKIP_COUNT + 1))
    if [ -n "$detail" ]; then
        RESULTS+=("[SKIP] ${name} — ${detail}")
    else
        RESULTS+=("[SKIP] ${name}")
    fi
    echo "  [SKIP] ${name} ${detail:+— $detail}"
}

separator() {
    echo "-----------------------------------------------------------"
}

# ---------------------------------------------------------------------------
# 1. Version check
# ---------------------------------------------------------------------------
test_version() {
    echo ""
    echo "1. Version Check"
    separator

    if output=$(selfware --version 2>&1); then
        SELFWARE_VERSION=$(echo "$output" | head -1)
        if [ -n "${SELFWARE_EXPECTED_GIT_SHA:-}" ] && \
           [ "${SELFWARE_EXPECTED_GIT_SHA}" != "unknown" ] && \
           [[ "$SELFWARE_VERSION" != *"+g${SELFWARE_EXPECTED_GIT_SHA}"* ]]; then
            record_fail "selfware --version" \
                "missing expected provenance +g${SELFWARE_EXPECTED_GIT_SHA}: ${SELFWARE_VERSION}"
        else
            record_pass "selfware --version" "$SELFWARE_VERSION"
        fi
    else
        record_fail "selfware --version" "exit code $?"
    fi
}

# ---------------------------------------------------------------------------
# 2. Help output
# ---------------------------------------------------------------------------
test_help() {
    echo ""
    echo "2. Help Output"
    separator

    if output=$(selfware --help 2>&1); then
        # Verify key subcommands appear in help text
        if echo "$output" | grep -q "doctor" && echo "$output" | grep -q "init" && echo "$output" | grep -q "run"; then
            record_pass "selfware --help" "lists doctor, init, run subcommands"
        else
            record_fail "selfware --help" "missing expected subcommands in output"
        fi
    else
        record_fail "selfware --help" "exit code $?"
    fi
}

# ---------------------------------------------------------------------------
# 3. Doctor mode
# ---------------------------------------------------------------------------
test_doctor() {
    echo ""
    echo "3. Doctor Mode"
    separator

    # `doctor` verifies both the local toolchain and the configured model. Keep
    # that model check deterministic and hermetic: a tiny local server exercises
    # the same OpenAI-compatible routes without depending on a public endpoint.
    local doctor_dir="$VALIDATION_DIR/doctor-test"
    local port_file="$doctor_dir/mock-port"
    local mock_log="$doctor_dir/mock.log"
    local mock_pid=""
    local mock_port=""
    local doctor_exit=0
    mkdir -p "$doctor_dir"

    cat > "$doctor_dir/mock_llm.py" << 'PYEOF'
import json
import sys
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer


MODEL = "docker-doctor-text-model"


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def log_message(self, format, *args):
        # Keep a request audit trail so the journey can prove it reached the
        # completion route, rather than passing on system checks alone.
        sys.stderr.write("%s\n" % (format % args))
        sys.stderr.flush()

    def send_body(self, body, content_type):
        encoded = body.encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(encoded)))
        self.send_header("X-vLLM-Version", "docker-validation-mock")
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self):
        if self.path == "/v1/models":
            self.send_body(
                json.dumps(
                    {
                        "object": "list",
                        "data": [
                            {
                                "id": MODEL,
                                "object": "model",
                                "owned_by": "vllm",
                                "max_model_len": 32768,
                            }
                        ],
                    }
                ),
                "application/json",
            )
            return
        self.send_error(404)

    def do_POST(self):
        if self.path != "/v1/chat/completions":
            self.send_error(404)
            return

        length = int(self.headers.get("Content-Length", "0"))
        request = json.loads(self.rfile.read(length) or b"{}")
        has_tools = bool(request.get("tools"))

        if request.get("stream"):
            if has_tools:
                event = {
                    "choices": [
                        {
                            "delta": {
                                "tool_calls": [
                                    {
                                        "index": 0,
                                        "id": "call_docker_validation",
                                        "type": "function",
                                        "function": {
                                            "name": "calculator",
                                            "arguments": '{"expression":"2+2"}',
                                        },
                                    }
                                ]
                            },
                            "finish_reason": None,
                        }
                    ]
                }
            else:
                event = {
                    "choices": [
                        {"delta": {"content": "hi"}, "finish_reason": None}
                    ]
                }
            self.send_body(
                "data: " + json.dumps(event) + "\n\ndata: [DONE]\n\n",
                "text/event-stream",
            )
            return

        message = {"role": "assistant", "content": "hello"}
        if has_tools:
            message["content"] = None
            message["tool_calls"] = [
                {
                    "id": "call_docker_validation",
                    "type": "function",
                    "function": {
                        "name": "calculator",
                        "arguments": '{"expression":"2+2"}',
                    },
                }
            ]
        self.send_body(
            json.dumps(
                {
                    "choices": [{"message": message, "finish_reason": "stop"}],
                    "usage": {"completion_tokens": 1},
                }
            ),
            "application/json",
        )


server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
with open(sys.argv[1], "w", encoding="utf-8") as port_output:
    port_output.write(str(server.server_port))
server.serve_forever()
PYEOF

    python3 "$doctor_dir/mock_llm.py" "$port_file" >"$mock_log" 2>&1 &
    mock_pid=$!

    # Wait for both the selected port and the listening socket. A dynamic port
    # avoids collisions with a developer's service when this runs locally.
    for _ in $(seq 1 50); do
        if [ -s "$port_file" ]; then
            mock_port=$(cat "$port_file")
            if curl --silent --fail "http://127.0.0.1:${mock_port}/v1/models" >/dev/null; then
                break
            fi
        fi
        sleep 0.1
    done

    if [ -z "$mock_port" ] || ! kill -0 "$mock_pid" 2>/dev/null || \
       ! curl --silent --fail "http://127.0.0.1:${mock_port}/v1/models" >/dev/null; then
        kill "$mock_pid" 2>/dev/null || true
        wait "$mock_pid" 2>/dev/null || true
        record_fail "selfware doctor" "local model fixture failed to start"
        return
    fi

    cat > "$doctor_dir/selfware.toml" << TOMLEOF
endpoint = "http://127.0.0.1:${mock_port}/v1"
model = "docker-doctor-text-model"
max_tokens = 4096
context_length = 32768
temperature = 0.0

[safety]
allowed_paths = ["/home/validator"]

[agent]
native_function_calling = true
TOMLEOF

    if output=$(npm_config_offline=true selfware --no-color --ascii \
        --config "$doctor_dir/selfware.toml" doctor 2>&1); then
        kill "$mock_pid" 2>/dev/null || true
        wait "$mock_pid" 2>/dev/null || true

        # A zero exit alone is insufficient: require evidence from both halves
        # of the command and from the mock's completion-route audit trail.
        if echo "$output" | grep -Fq "[PASS] rustc" && \
           echo "$output" | grep -Fq "[PASS] cargo" && \
           echo "$output" | grep -Fq "[PASS] endpoint reachable" && \
           echo "$output" | grep -Fq "[PASS] configured model available" && \
           grep -Fq 'POST /v1/chat/completions' "$mock_log"; then
            pass_lines=$(echo "$output" | grep -c "\[PASS\]" || true)
            record_pass "selfware doctor" \
                "${pass_lines} checks passed, including a local model round-trip"
        else
            record_fail "selfware doctor" \
                "command exited 0 without complete system and model-probe evidence"
        fi
    else
        doctor_exit=$?
        kill "$mock_pid" 2>/dev/null || true
        wait "$mock_pid" 2>/dev/null || true
        record_fail "selfware doctor" "exit code ${doctor_exit}"
    fi
}

# ---------------------------------------------------------------------------
# 4. Config creation (manual selfware.toml)
# ---------------------------------------------------------------------------
test_config_creation() {
    echo ""
    echo "4. Config Creation"
    separator

    mkdir -p "$VALIDATION_DIR/config-test"
    cat > "$VALIDATION_DIR/config-test/selfware.toml" << 'TOMLEOF'
endpoint = "http://127.0.0.1:8000/v1"
model = "txn545/Qwen3.5-122B-A10B-NVFP4"
max_tokens = 4096
temperature = 0.7

[agent]
max_iterations = 20
native_function_calling = false
TOMLEOF

    if [ ! -f "$VALIDATION_DIR/config-test/selfware.toml" ]; then
        record_fail "Config creation" "selfware.toml not created"
        return
    fi

    # Exercise Selfware's real loader and invariant checks. Grepping key names
    # cannot prove that the generated file is valid TOML or usable config.
    if selfware --config "$VALIDATION_DIR/config-test/selfware.toml" \
        --validate-config >/dev/null 2>&1; then
        record_pass "Config creation" "selfware.toml loaded and validated"
    else
        record_fail "Config creation" "selfware.toml failed config validation"
    fi
}

# ---------------------------------------------------------------------------
# 5. Template scaffolding
# ---------------------------------------------------------------------------
test_template_scaffold() {
    echo ""
    echo "5. Template Scaffolding"
    separator

    # --- Rust template ---
    local rust_dir="$VALIDATION_DIR/scaffold-rust"
    mkdir -p "$rust_dir"
    if (cd "$rust_dir" && selfware init --template rust 2>&1); then
        if [ -f "$rust_dir/selfware.toml" ]; then
            record_pass "Template scaffold (Rust)" "selfware.toml created"
        else
            record_fail "Template scaffold (Rust)" "selfware.toml missing after init"
        fi
    else
        record_fail "Template scaffold (Rust)" "selfware init --template rust failed"
    fi

    # --- Python template ---
    local py_dir="$VALIDATION_DIR/scaffold-python"
    mkdir -p "$py_dir"
    if (cd "$py_dir" && selfware init --template python 2>&1); then
        if [ -f "$py_dir/selfware.toml" ]; then
            record_pass "Template scaffold (Python)" "selfware.toml created"
        else
            record_fail "Template scaffold (Python)" "selfware.toml missing after init"
        fi
    else
        record_fail "Template scaffold (Python)" "selfware init --template python failed"
    fi

    # --- Node.js template ---
    local node_dir="$VALIDATION_DIR/scaffold-node"
    mkdir -p "$node_dir"
    if command -v node >/dev/null 2>&1; then
        if (cd "$node_dir" && selfware init --template node 2>&1); then
            if [ -f "$node_dir/selfware.toml" ]; then
                record_pass "Template scaffold (Node.js)" "selfware.toml created"
            else
                record_fail "Template scaffold (Node.js)" "selfware.toml missing after init"
            fi
        else
            record_fail "Template scaffold (Node.js)" "selfware init --template node failed"
        fi
    else
        record_fail "Template scaffold (Node.js)" \
            "node missing from validation image"
    fi

    # --- Minimal template ---
    local min_dir="$VALIDATION_DIR/scaffold-minimal"
    mkdir -p "$min_dir"
    if (cd "$min_dir" && selfware init --template minimal 2>&1); then
        if [ -f "$min_dir/selfware.toml" ]; then
            record_pass "Template scaffold (Minimal)" "selfware.toml created"
        else
            record_fail "Template scaffold (Minimal)" "selfware.toml missing after init"
        fi
    else
        record_fail "Template scaffold (Minimal)" "selfware init --template minimal failed"
    fi
}

# ---------------------------------------------------------------------------
# 6. Binary capabilities (no LLM required)
# ---------------------------------------------------------------------------
test_binary_capabilities() {
    echo ""
    echo "6. Binary Capabilities (no LLM needed)"
    separator

    # --- Status command ---
    local status_dir="$VALIDATION_DIR/status-test"
    mkdir -p "$status_dir"
    cat > "$status_dir/selfware.toml" << 'TOMLEOF'
endpoint = "http://localhost:9999/v1"
model = "test-model"
max_tokens = 1024
TOMLEOF

    if (cd "$status_dir" && selfware status 2>&1); then
        record_pass "selfware status" "ran without error"
    else
        # Status may fail without an LLM, but the binary should at least parse args
        record_fail "selfware status" "command returned error"
    fi

    # --- Status JSON output ---
    local status_json
    if status_json=$(cd "$status_dir" && \
        selfware --no-color --ascii status --output-format json \
            2>"$status_dir/status-json.stderr"); then
        if STATUS_JSON="$status_json" python3 - << 'PYEOF'
import json
import os

status = json.loads(os.environ["STATUS_JSON"])
required = {
    "model",
    "endpoint",
    "is_local",
    "endpoint_reachable",
    "endpoint_status",
    "project_path",
    "execution_mode",
    "journal",
}
assert required <= status.keys()
assert status["model"] == "test-model"
assert status["endpoint"] == "http://localhost:9999/v1"
assert type(status["is_local"]) is bool
assert type(status["endpoint_reachable"]) is bool
assert isinstance(status["endpoint_status"], str)
assert isinstance(status["project_path"], str)
assert isinstance(status["execution_mode"], str)
assert isinstance(status["journal"], dict)
assert {"total", "completed", "in_progress"} <= status["journal"].keys()
assert all(
    type(status["journal"][key]) is int
    for key in ("total", "completed", "in_progress")
)
PYEOF
        then
            record_pass "selfware status --output-format json" \
                "valid status schema and selected config values"
        else
            record_fail "selfware status --output-format json" \
                "stdout was not the expected JSON status schema"
        fi
    else
        record_fail "selfware status --output-format json" "command failed"
    fi
}

# ---------------------------------------------------------------------------
# 7. Git integration
# ---------------------------------------------------------------------------
test_git_integration() {
    echo ""
    echo "7. Git Integration"
    separator

    local git_dir="$VALIDATION_DIR/git-test"
    mkdir -p "$git_dir"

    # Set up a minimal git repo
    if (cd "$git_dir" && git init && echo "hello" > hello.txt && git add -A && git commit -m "init" 2>&1); then
        record_pass "Git repo setup" "initialized and committed"
    else
        record_fail "Git repo setup" "could not create test repo"
        return
    fi

    # Verify git log works
    if (cd "$git_dir" && git log --oneline 2>&1 | grep -q "init"); then
        record_pass "Git log" "commit visible in log"
    else
        record_fail "Git log" "commit not found"
    fi

    # Verify git status succeeds and actually reports a clean working tree.
    local git_status
    if git_status=$(cd "$git_dir" && git status --porcelain 2>&1); then
        if [ -z "$git_status" ]; then
            record_pass "Git status" "clean working tree"
        else
            record_fail "Git status" "unexpected changes: ${git_status}"
        fi
    else
        record_fail "Git status" "git status failed"
    fi
}

# ---------------------------------------------------------------------------
# 8. Shell tools (basic system commands)
# ---------------------------------------------------------------------------
test_shell_tools() {
    echo ""
    echo "8. Shell Tools (system commands)"
    separator

    # Verify basic commands available in container
    local all_ok=true

    for cmd in bash git curl python3; do
        if command -v "$cmd" >/dev/null 2>&1; then
            record_pass "Shell tool: $cmd" "$(command -v "$cmd")"
        else
            record_fail "Shell tool: $cmd" "not found in PATH"
            all_ok=false
        fi
    done

    # Node is installed by tests/Dockerfile.validation and is part of the
    # validation runtime contract.
    if command -v node >/dev/null 2>&1; then
        record_pass "Shell tool: node" "$(node --version 2>&1)"
    else
        record_fail "Shell tool: node" "missing from validation image"
    fi
}

# ---------------------------------------------------------------------------
# 9. File operations
# ---------------------------------------------------------------------------
test_file_operations() {
    echo ""
    echo "9. File Operations"
    separator

    local file_dir="$VALIDATION_DIR/file-test"
    mkdir -p "$file_dir"

    # Write
    echo "hello world" > "$file_dir/hello.txt"
    if [ -f "$file_dir/hello.txt" ]; then
        record_pass "File write" "hello.txt created"
    else
        record_fail "File write" "hello.txt not created"
        return
    fi

    # Read
    content=$(cat "$file_dir/hello.txt")
    if [ "$content" = "hello world" ]; then
        record_pass "File read" "content matches"
    else
        record_fail "File read" "content mismatch: got '$content'"
    fi

    # Nested directory creation
    mkdir -p "$file_dir/a/b/c"
    echo "nested" > "$file_dir/a/b/c/deep.txt"
    if [ -f "$file_dir/a/b/c/deep.txt" ]; then
        record_pass "Nested file write" "a/b/c/deep.txt created"
    else
        record_fail "Nested file write" "nested file not created"
    fi

    # Glob-style file finding
    file_count=$(find "$file_dir" -name "*.txt" | wc -l | tr -d ' ')
    if [ "$file_count" -ge 2 ]; then
        record_pass "File glob find" "found $file_count .txt files"
    else
        record_fail "File glob find" "expected >=2 .txt files, found $file_count"
    fi
}

# ---------------------------------------------------------------------------
# 10. Search capabilities
# ---------------------------------------------------------------------------
test_search() {
    echo ""
    echo "10. Search Capabilities"
    separator

    local search_dir="$VALIDATION_DIR/search-test"
    mkdir -p "$search_dir/src"

    # Create test files
    cat > "$search_dir/src/main.rs" << 'RUSTEOF'
fn main() {
    println!("Hello, selfware!");
    let x = 42;
    process(x);
}

fn process(value: i32) -> i32 {
    value * 2
}
RUSTEOF

    cat > "$search_dir/src/lib.rs" << 'RUSTEOF'
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

pub fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
RUSTEOF

    # Grep search
    if grep -r "selfware" "$search_dir" >/dev/null 2>&1; then
        matches=$(grep -r "selfware" "$search_dir" | wc -l | tr -d ' ')
        record_pass "Grep search" "found $matches matches for 'selfware'"
    else
        record_fail "Grep search" "no matches found"
    fi

    # Find files by pattern
    rs_files=$(find "$search_dir" -name "*.rs" | wc -l | tr -d ' ')
    if [ "$rs_files" -ge 2 ]; then
        record_pass "Glob find (*.rs)" "found $rs_files Rust files"
    else
        record_fail "Glob find (*.rs)" "expected >=2, found $rs_files"
    fi

    # Regex search
    if grep -rE "fn\s+\w+\(" "$search_dir" >/dev/null 2>&1; then
        fn_count=$(grep -rE "fn\s+\w+\(" "$search_dir" | wc -l | tr -d ' ')
        record_pass "Regex search (fn declarations)" "found $fn_count function declarations"
    else
        record_fail "Regex search (fn declarations)" "no function declarations found"
    fi
}

# ---------------------------------------------------------------------------
# 11. Language toolchain validation
# ---------------------------------------------------------------------------
test_language_toolchains() {
    echo ""
    echo "11. Language Toolchain Validation"
    separator

    # Python
    if command -v python3 >/dev/null 2>&1; then
        py_version=$(python3 --version 2>&1)
        if python3 -c "print('selfware validation')" >/dev/null 2>&1; then
            record_pass "Python3 execution" "$py_version"
        else
            record_fail "Python3 execution" "could not run test script"
        fi
    else
        record_fail "Python3 execution" "python3 missing from validation image"
    fi

    # Node.js
    if command -v node >/dev/null 2>&1; then
        node_version=$(node --version 2>&1)
        if node -e "console.log('selfware validation')" >/dev/null 2>&1; then
            record_pass "Node.js execution" "$node_version"
        else
            record_fail "Node.js execution" "could not run test script"
        fi
    else
        record_fail "Node.js execution" "node missing from validation image"
    fi

    # npm
    if command -v npm >/dev/null 2>&1; then
        npm_version=$(npm --version 2>&1)
        record_pass "npm available" "v${npm_version}"
    else
        record_fail "npm available" "npm missing from validation image"
    fi
}

# ---------------------------------------------------------------------------
# 12. Config parsing validation
# ---------------------------------------------------------------------------
test_config_parsing() {
    echo ""
    echo "12. Config Parsing Validation"
    separator

    local cfg_dir="$VALIDATION_DIR/config-parse-test"
    mkdir -p "$cfg_dir"

    # Valid config — selfware should at least parse it without crashing
    cat > "$cfg_dir/selfware.toml" << 'TOMLEOF'
endpoint = "http://localhost:8080/v1"
model = "qwen3-coder"
max_tokens = 8192
temperature = 0.7

[safety]
allowed_paths = ["."]
denied_paths = ["**/.env", "**/.git/**"]
protected_branches = ["main"]
require_confirmation = []

[agent]
max_iterations = 50
native_function_calling = false

[qa]
profile = "standard"
auto_fix_iterations = 3
test_retry_iterations = 2

[[hooks]]
event = "PostToolUse"
match_tools = ["file_write"]
command = "true"
TOMLEOF

    # --validate-config runs after Config::load and invariant validation. `--help`
    # exits inside clap before the config is read and cannot test this path.
    if selfware --config "$cfg_dir/selfware.toml" --validate-config >/dev/null 2>&1; then
        record_pass "Config parsing (full config)" "loaded and validated"
    else
        record_fail "Config parsing (full config)" "selfware rejected the valid config"
    fi

    # Minimal config
    cat > "$cfg_dir/selfware.toml" << 'TOMLEOF'
endpoint = "http://localhost:8080/v1"
model = "test"
TOMLEOF

    if selfware --config "$cfg_dir/selfware.toml" --validate-config >/dev/null 2>&1; then
        record_pass "Config parsing (minimal config)" "loaded and validated"
    else
        record_fail "Config parsing (minimal config)" "selfware rejected the valid minimal config"
    fi

    # Negative control: this must fail, proving the command above did not take
    # an early-exit path that ignores the selected file.
    cat > "$cfg_dir/invalid.toml" << 'TOMLEOF'
endpoint = ["unterminated"
TOMLEOF

    if selfware --config "$cfg_dir/invalid.toml" --validate-config >/dev/null 2>&1; then
        record_fail "Config parsing (malformed config)" "invalid TOML was accepted"
    else
        record_pass "Config parsing (malformed config)" "invalid TOML rejected"
    fi
}

# ---------------------------------------------------------------------------
# 13. CLI flag combinations
# ---------------------------------------------------------------------------
test_cli_flags() {
    echo ""
    echo "13. CLI Flag Combinations"
    separator

    # --version with --quiet (should still print version)
    if selfware --version 2>&1 | grep -q "selfware"; then
        record_pass "Version output format" "contains 'selfware'"
    else
        record_fail "Version output format" "output does not contain 'selfware'"
    fi

    # --no-color flag accepted
    if selfware --no-color --help >/dev/null 2>&1; then
        record_pass "--no-color flag" "accepted without error"
    else
        record_fail "--no-color flag" "not accepted"
    fi

    # --ascii flag accepted
    if selfware --ascii --help >/dev/null 2>&1; then
        record_pass "--ascii flag" "accepted without error"
    else
        record_fail "--ascii flag" "not accepted"
    fi

    # --compact flag accepted
    if selfware --compact --help >/dev/null 2>&1; then
        record_pass "--compact flag" "accepted without error"
    else
        record_fail "--compact flag" "not accepted"
    fi

    # --theme flag variants
    for theme_name in amber ocean minimal high-contrast; do
        if selfware --theme "$theme_name" --help >/dev/null 2>&1; then
            record_pass "--theme $theme_name" "accepted"
        else
            record_fail "--theme $theme_name" "not accepted"
        fi
    done
}

# ---------------------------------------------------------------------------
# Report
# ---------------------------------------------------------------------------
print_report() {
    local end_time
    end_time=$(date +%s)
    local duration=$((end_time - START_TIME))
    local total=$((PASS_COUNT + FAIL_COUNT + SKIP_COUNT))

    echo ""
    echo ""
    echo "==========================================================="
    echo "          SELFWARE VALIDATION REPORT"
    echo "==========================================================="
    echo "  Version:  ${SELFWARE_VERSION:-unknown}"
    echo "  Date:     $(date '+%Y-%m-%d %H:%M:%S %Z')"
    echo "  Duration: ${duration}s"
    echo "==========================================================="
    echo ""

    for result in "${RESULTS[@]}"; do
        echo "  $result"
    done

    echo ""
    echo "-----------------------------------------------------------"
    echo "  Passed:  ${PASS_COUNT}"
    echo "  Failed:  ${FAIL_COUNT}"
    echo "  Skipped: ${SKIP_COUNT}"
    echo "  Total:   ${PASS_COUNT}/${total} passed"
    echo "-----------------------------------------------------------"

    if [ "$FAIL_COUNT" -gt 0 ]; then
        echo ""
        echo "  RESULT: FAIL (${FAIL_COUNT} failures)"
        echo ""
        return 1
    else
        echo ""
        echo "  RESULT: PASS (all ${PASS_COUNT} checks passed)"
        echo ""
        return 0
    fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    echo "==========================================================="
    echo "  Selfware Docker Validation Suite"
    echo "==========================================================="
    echo "  Starting at $(date '+%Y-%m-%d %H:%M:%S %Z')"
    echo "  Working directory: $(pwd)"

    # Create workspace
    mkdir -p "$VALIDATION_DIR"

    # Run all test sections
    test_version
    test_help
    test_doctor
    test_config_creation
    test_template_scaffold
    test_binary_capabilities
    test_git_integration
    test_shell_tools
    test_file_operations
    test_search
    test_language_toolchains
    test_config_parsing
    test_cli_flags

    # Print final report
    print_report
}

main "$@"
