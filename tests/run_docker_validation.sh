#!/usr/bin/env bash
# =============================================================================
# Run Selfware Docker Validation
# =============================================================================
# Builds the validation image from the repo root and runs the full test suite.
#
# Usage:
#   ./tests/run_docker_validation.sh          # build + run
#   ./tests/run_docker_validation.sh --no-cache  # force full rebuild
# =============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
IMAGE_NAME="selfware-validation"
DOCKERFILE="tests/Dockerfile.validation"

echo "=== Selfware Docker Validation ==="
echo "  Repo root:  $REPO_ROOT"
echo "  Dockerfile: $DOCKERFILE"
echo "  Image:      $IMAGE_NAME"
echo ""

# Collect extra docker build args (e.g., --no-cache)
BUILD_ARGS=()
for arg in "$@"; do
    BUILD_ARGS+=("$arg")
done

# Build
echo "--- Building validation image ---"
docker build \
    -f "$REPO_ROOT/$DOCKERFILE" \
    -t "$IMAGE_NAME" \
    "${BUILD_ARGS[@]+"${BUILD_ARGS[@]}"}" \
    "$REPO_ROOT"

echo ""
echo "--- Running validation suite ---"
echo ""

# Run and propagate exit code
docker run --rm "$IMAGE_NAME"
