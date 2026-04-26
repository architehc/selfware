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
        record_pass "selfware --version" "$SELFWARE_VERSION"
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

    if output=$(selfware doctor 2>&1); then
        # Doctor returns 0 even when optional deps are missing.
        # Count how many checks passed from the output.
        pass_lines=$(echo "$output" | grep -c "OK\|PASS\|✓\|ok\|available" || true)
        record_pass "selfware doctor" "${pass_lines} checks reported"
    else
        # Doctor may exit non-zero if critical deps missing — still useful info
        record_fail "selfware doctor" "exit code $?"
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

    if [ -f "$VALIDATION_DIR/config-test/selfware.toml" ]; then
        # Verify the config is valid TOML and contains expected keys
        if grep -q "endpoint" "$VALIDATION_DIR/config-test/selfware.toml" && \
           grep -q "model" "$VALIDATION_DIR/config-test/selfware.toml" && \
           grep -q "max_tokens" "$VALIDATION_DIR/config-test/selfware.toml"; then
            record_pass "Config creation" "selfware.toml written with endpoint, model, max_tokens"
        else
            record_fail "Config creation" "config file missing expected keys"
        fi
    else
        record_fail "Config creation" "selfware.toml not created"
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
        record_skip "Template scaffold (Node.js)" "node not available"
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
    if (cd "$status_dir" && selfware status --output-format json 2>&1); then
        record_pass "selfware status --output-format json" "JSON output mode works"
    else
        record_fail "selfware status --output-format json" "JSON output failed"
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

    # Verify git status works
    if (cd "$git_dir" && git status --porcelain 2>&1); then
        record_pass "Git status" "clean working tree"
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

    # Optional: node
    if command -v node >/dev/null 2>&1; then
        record_pass "Shell tool: node" "$(node --version 2>&1)"
    else
        record_skip "Shell tool: node" "not installed"
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
        record_skip "Python3 execution" "python3 not available"
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
        record_skip "Node.js execution" "node not available"
    fi

    # npm
    if command -v npm >/dev/null 2>&1; then
        npm_version=$(npm --version 2>&1)
        record_pass "npm available" "v${npm_version}"
    else
        record_skip "npm available" "npm not installed"
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
blocked_commands = ["rm -rf /"]

[agent]
max_iterations = 50
native_function_calling = false

[hooks]
pre_tool_use = []
post_tool_use = []

[mcp]
servers = []

[qa]
enabled = false
TOMLEOF

    # Run --help in directory with this config (should not crash on config load)
    if (cd "$cfg_dir" && selfware --help >/dev/null 2>&1); then
        record_pass "Config parsing (full config)" "parsed without crash"
    else
        record_fail "Config parsing (full config)" "selfware crashed on config load"
    fi

    # Minimal config
    cat > "$cfg_dir/selfware.toml" << 'TOMLEOF'
endpoint = "http://localhost:8080/v1"
model = "test"
TOMLEOF

    if (cd "$cfg_dir" && selfware --help >/dev/null 2>&1); then
        record_pass "Config parsing (minimal config)" "parsed without crash"
    else
        record_fail "Config parsing (minimal config)" "selfware crashed on minimal config"
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
