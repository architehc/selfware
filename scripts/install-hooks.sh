#!/bin/sh
# Install the stop-the-line pre-commit gate (AGENTS.md Rule 1).
# The hook is intentionally unversioned in git, so this script is the
# canonical way to install it on a fresh clone. Keep it in sync with
# scripts/check_rule1_gate.sh and .pre-commit-config.yaml.
set -eu
HOOK="$(git rev-parse --git-common-dir)/hooks/pre-commit"
cat > "$HOOK" <<'HOOK_EOF'
#!/bin/sh
# selfware stop-the-line gate (AGENTS.md Rule 1): fmt, clippy, rustdoc and
# lib tests must be green before every commit.

# Unset git hook environment variables so child tests and git commands don't
# inherit them.
unset GIT_INDEX_FILE GIT_DIR GIT_WORK_TREE

echo "pre-commit: cargo fmt --check..."
if ! cargo fmt -- --check >/dev/null 2>&1; then
    echo "STOP-THE-LINE: cargo fmt --check failed. Run 'cargo fmt' and re-stage."
    exit 1
fi

echo "pre-commit: cargo clippy..."
if ! cargo clippy --all-targets -- -D warnings >/dev/null 2>&1; then
    echo "STOP-THE-LINE: clippy failed. Fix warnings before committing."
    exit 1
fi

# Same command as the CI Documentation job. Private/broken intra-doc links
# otherwise surface only in CI or check_ci_parity.sh (they did on ~7 merges).
echo "pre-commit: cargo doc (-D warnings)..."
if ! RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --features extras >/dev/null 2>&1; then
    echo "STOP-THE-LINE: cargo doc failed (often a private intra-doc link: use a code span)."
    echo "Rerun: RUSTDOCFLAGS=\"-D warnings\" cargo doc --no-deps --features extras"
    exit 1
fi

echo "pre-commit: cargo test --lib..."
if ! cargo test --lib; then
    echo "STOP-THE-LINE: cargo test --lib failed. Fix tests before committing."
    exit 1
fi
HOOK_EOF
chmod +x "$HOOK"
echo "installed stop-the-line gate at $HOOK"
