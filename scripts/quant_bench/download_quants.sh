#!/usr/bin/env bash
# Download the Qwen3.6-27B GGUF quants used by the quant benchmark.
#
# Pulls the smallest (IQ2_M, ~10 GB) and biggest (Q8_K_P, ~32 GB) non-FP16
# quants plus the f16 mmproj from
# `HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive`.
#
# Defaults:
#   --dir   ~/models/qwen36-quants
#
# Idempotent: re-running with files already present skips download as long as
# the on-disk size matches what huggingface reports.  When the HF API exposes
# an `lfs.sha256`, that is verified after download.

set -euo pipefail

REPO="HauhauCS/Qwen3.6-27B-Uncensored-HauhauCS-Aggressive"
DEFAULT_DIR="${HOME}/models/qwen36-quants"
TARGET_DIR="${DEFAULT_DIR}"

FILES=(
    "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-IQ2_M.gguf"
    "Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf"
    "mmproj-Qwen3.6-27B-Uncensored-HauhauCS-Aggressive-f16.gguf"
)

usage() {
    cat <<EOF
Usage: $(basename "$0") [--dir PATH] [--repo REPO] [--help]

  --dir PATH    Download directory (default: ${DEFAULT_DIR})
  --repo REPO   HF repo (default: ${REPO})
  --help        Show this message

Files fetched:
$(printf '  - %s\n' "${FILES[@]}")
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dir)
            TARGET_DIR="$2"
            shift 2
            ;;
        --repo)
            REPO="$2"
            shift 2
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unknown argument: $1" >&2
            usage
            exit 2
            ;;
    esac
done

mkdir -p "${TARGET_DIR}"
cd "${TARGET_DIR}"

log() {
    printf '[download_quants] %s\n' "$*" >&2
}

# ── HF metadata helpers ────────────────────────────────────────────────────────

# `huggingface-cli` is preferred; we only call `wget`/`curl` as a fallback.
have_hf_cli=0
if command -v huggingface-cli >/dev/null 2>&1; then
    have_hf_cli=1
fi
if command -v curl >/dev/null 2>&1; then
    have_curl=1
else
    have_curl=0
fi

# Returns "<size>\t<sha256>" for a single file, or empty on failure.  sha256
# is empty if HF didn't expose it.
hf_metadata() {
    local file="$1"
    local url="https://huggingface.co/api/models/${REPO}/tree/main?path=&recursive=false"
    if [[ "${have_curl}" -ne 1 ]]; then
        echo ""
        return 0
    fi
    # Best-effort metadata lookup; missing fields produce an empty result.
    curl -sSfL "${url}" 2>/dev/null \
        | python3 -c '
import json, sys
try:
    data = json.load(sys.stdin)
except Exception:
    sys.exit(0)
target = sys.argv[1]
for entry in data:
    if entry.get("path") == target and entry.get("type") == "file":
        size = entry.get("size") or (entry.get("lfs") or {}).get("size") or ""
        sha = (entry.get("lfs") or {}).get("oid", "")
        # HF stores sha256 as the lfs oid for LFS-tracked files.
        print(f"{size}\t{sha}")
        break
' "${file}" 2>/dev/null || true
}

verify_size() {
    local file="$1"
    local expected="$2"
    if [[ -z "${expected}" ]]; then
        log "no expected size for ${file}, skipping size check"
        return 0
    fi
    local actual
    actual=$(stat --printf='%s' "${file}" 2>/dev/null || stat -f%z "${file}" 2>/dev/null || echo "")
    if [[ "${actual}" == "${expected}" ]]; then
        log "size OK for ${file} (${actual} bytes)"
        return 0
    fi
    log "size MISMATCH for ${file}: have=${actual} expected=${expected}"
    return 1
}

verify_sha256() {
    local file="$1"
    local expected="$2"
    if [[ -z "${expected}" ]]; then
        return 0
    fi
    local actual
    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "${file}" | awk '{print $1}')
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum -a 256 "${file}" | awk '{print $1}')
    else
        log "no sha256 tool available, skipping checksum verification"
        return 0
    fi
    if [[ "${actual}" == "${expected}" ]]; then
        log "sha256 OK for ${file}"
        return 0
    fi
    log "sha256 MISMATCH for ${file}: have=${actual} expected=${expected}"
    return 1
}

download_file() {
    local file="$1"
    local meta size sha
    meta=$(hf_metadata "${file}")
    size="${meta%%	*}"
    sha="${meta#*	}"
    if [[ "${meta}" == "${size}" ]]; then
        sha=""  # no tab in meta
    fi

    # Skip if already present and size matches.
    if [[ -f "${file}" ]] && verify_size "${file}" "${size}"; then
        if verify_sha256 "${file}" "${sha}"; then
            log "${file} already present and verified, skipping"
            return 0
        fi
        log "${file} present but failed verification, re-downloading"
        rm -f "${file}"
    fi

    if [[ "${have_hf_cli}" -eq 1 ]]; then
        log "fetching ${file} via huggingface-cli"
        # `huggingface-cli download` writes into ./, downloading only the requested file.
        huggingface-cli download "${REPO}" "${file}" \
            --local-dir "${TARGET_DIR}" \
            --local-dir-use-symlinks False
    else
        local url="https://huggingface.co/${REPO}/resolve/main/${file}?download=true"
        if command -v wget >/dev/null 2>&1; then
            log "fetching ${file} via wget"
            wget --continue --output-document "${file}" "${url}"
        elif [[ "${have_curl}" -eq 1 ]]; then
            log "fetching ${file} via curl"
            curl -L --fail --retry 3 --continue-at - --output "${file}" "${url}"
        else
            log "ERROR: no huggingface-cli, wget, or curl available"
            return 1
        fi
    fi

    if [[ -n "${size}" ]]; then
        verify_size "${file}" "${size}" || {
            log "ERROR: size verification failed after download"
            return 1
        }
    fi
    if [[ -n "${sha}" ]]; then
        verify_sha256 "${file}" "${sha}" || {
            log "WARN: sha256 verification failed; the file may still be usable"
        }
    fi
}

log "target dir: ${TARGET_DIR}"
log "repo: ${REPO}"
for f in "${FILES[@]}"; do
    download_file "${f}"
done
log "done"
