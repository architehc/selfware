#!/bin/sh
# Install the stop-the-line pre-commit gate (AGENTS.md Rule 1).
# The hook is intentionally unversioned in git, so this script is the
# canonical way to install it on a fresh clone.
set -eu
HOOK="$(git rev-parse --git-dir)/hooks/pre-commit"
cat > "$HOOK" <<'HOOK_EOF'
#!/bin/sh
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
HOOK_EOF
chmod +x "$HOOK"
echo "installed stop-the-line gate at $HOOK"
