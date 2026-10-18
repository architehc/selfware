#!/usr/bin/env bash
# phi_tmux_arena.sh — One-command Phi Fox Tmux Cockpit & Debt Slayer Arena launcher
#
# Spawns (or re-attaches to) a 3-pane tmux session:
#   - Left pane (60% width):       Interactive developer / selfware agent shell
#   - Top-right pane (40% width):  Phi Fox Tmux Cockpit & Debt Slayer HUD (scripts/phi_tmux_game.py)
#   - Bottom-right pane (40%):     Live telemetry / verification activity stream
#
# Usage:
#   ./scripts/phi_tmux_arena.sh [options]
#
# Options:
#   --session <name>     Tmux session name (default: phi-arena)
#   --workspace <path>   Workspace root directory (default: repo root)
#   --server <url>       Optional Selfware evolve server URL for /api/phi/chat
#   --token <token>      Optional X-Selfware-Session token
#   --no-attach          Create/update session in background without attaching
#   --dry-run            Print the tmux orchestration plan without executing tmux
#   -h, --help           Show usage help

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

SESSION_NAME="${PHI_TMUX_SESSION:-phi-arena}"
WORKSPACE_ROOT="${REPO_ROOT}"
SERVER_URL="${SELFWARE_SERVER_URL:-}"
SESSION_TOKEN="${SELFWARE_SESSION_TOKEN:-}"
ATTACH=1
DRY_RUN=0

usage() {
    cat <<'EOF'
Usage: ./scripts/phi_tmux_arena.sh [options]

Options:
  --session <name>     Tmux session name (default: phi-arena)
  --workspace <path>   Workspace root directory (default: repository root)
  --server <url>       Optional Selfware evolve server URL for /api/phi/chat
  --token <token>      Optional X-Selfware-Session token
  --no-attach          Create session in background without attaching
  --dry-run            Print the tmux commands without executing them
  -h, --help           Show this help text
EOF
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        --session)
            SESSION_NAME="$2"
            shift 2
            ;;
        --workspace)
            WORKSPACE_ROOT="$2"
            shift 2
            ;;
        --server)
            SERVER_URL="$2"
            shift 2
            ;;
        --token)
            SESSION_TOKEN="$2"
            shift 2
            ;;
        --no-attach)
            ATTACH=0
            shift
            ;;
        --dry-run)
            DRY_RUN=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            usage >&2
            exit 2
            ;;
    esac
done

WORKSPACE_ROOT="$(cd "${WORKSPACE_ROOT}" && pwd)"
GAME_SCRIPT="${SCRIPT_DIR}/phi_tmux_game.py"

if [[ ! -f "${GAME_SCRIPT}" ]]; then
    echo "Error: ${GAME_SCRIPT} not found." >&2
    exit 1
fi

GAME_CMD="python3 $(printf '%q' "${GAME_SCRIPT}") --workspace $(printf '%q' "${WORKSPACE_ROOT}") --tmux-session $(printf '%q' "${SESSION_NAME}") --editor-pane 0.0"
if [[ -n "${SERVER_URL}" ]]; then
    GAME_CMD="${GAME_CMD} --server $(printf '%q' "${SERVER_URL}")"
fi
if [[ -n "${SESSION_TOKEN}" ]]; then
    GAME_CMD="${GAME_CMD} --session-token $(printf '%q' "${SESSION_TOKEN}")"
fi

TELEMETRY_CMD="python3 -c '
import json, pathlib, time, sys
ws = pathlib.Path(sys.argv[1])
act = ws / \".selfware\" / \"phi\" / \"activity\"
print(\"\\033[1;38;5;214mφ TELEMETRY STREAM\\033[0m — watching\", act)
last_raw = None
while True:
    try:
        if act.is_file() and not act.is_symlink():
            raw = act.read_text(encoding=\"utf-8\")
            if raw != last_raw:
                last_raw = raw
                data = json.loads(raw)
                ts = time.strftime(\"%H:%M:%S\")
                agent = data.get(\"agent_state\", \"idle\")
                phase = data.get(\"phase\", \"idle\")
                unrev = data.get(\"unreviewed_lines\", 0)
                untest = data.get(\"untested_lines\", 0)
                print(f\"[{ts}] state={agent:<8} phase={phase:<10} unreviewed={unrev} untested={untest}\", flush=True)
    except Exception as exc:
        print(f\"[telemetry] {exc}\", flush=True)
    time.sleep(1.0)
' $(printf '%q' "${WORKSPACE_ROOT}")"

if [[ "${DRY_RUN}" -eq 1 ]]; then
    cat <<EOF
# Phi Fox Tmux Arena Dry Run
session_name=${SESSION_NAME}
workspace=${WORKSPACE_ROOT}
layout=3-pane-cockpit (left 60% editor/agent, top-right 40% phi_tmux_game.py, bottom-right 40% telemetry)
tmux new-session -d -s "${SESSION_NAME}" -n "cockpit" -c "${WORKSPACE_ROOT}"
tmux set-option -t "${SESSION_NAME}" status-style "bg=#14110f,fg=#f59e0b"
tmux set-option -t "${SESSION_NAME}" status-left "#[bg=#f59e0b,fg=#14110f,bold] φ SELFWARE ARENA #[bg=#14110f,fg=#f59e0b] #S "
tmux set-option -t "${SESSION_NAME}" status-right "#[fg=#a8a29e]honest-telemetry: observed_signals #[fg=#f59e0b,bold]| %H:%M "
tmux split-window -h -p 42 -t "${SESSION_NAME}:0.0" -c "${WORKSPACE_ROOT}" "${GAME_CMD}"
tmux split-window -v -p 28 -t "${SESSION_NAME}:0.1" -c "${WORKSPACE_ROOT}" "${TELEMETRY_CMD}"
tmux select-pane -t "${SESSION_NAME}:0.1"
EOF
    exit 0
fi

if ! command -v tmux >/dev/null 2>&1; then
    echo "Error: tmux is not installed or not in PATH." >&2
    exit 1
fi

if ! tmux has-session -t "${SESSION_NAME}" 2>/dev/null; then
    tmux new-session -d -s "${SESSION_NAME}" -n "cockpit" -c "${WORKSPACE_ROOT}"
    tmux set-option -t "${SESSION_NAME}" status-style "bg=#14110f,fg=#f59e0b"
    tmux set-option -t "${SESSION_NAME}" status-left-length 40
    tmux set-option -t "${SESSION_NAME}" status-right-length 80
    tmux set-option -t "${SESSION_NAME}" status-left "#[bg=#f59e0b,fg=#14110f,bold] φ SELFWARE ARENA #[bg=#14110f,fg=#f59e0b] #S "
    tmux set-option -t "${SESSION_NAME}" status-right "#[fg=#a8a29e]trust: observed_signals #[fg=#f59e0b,bold]| [r]ed [d]ebt [h]unt [c]hat | %H:%M "
    tmux set-option -t "${SESSION_NAME}" pane-border-style "fg=#44403c"
    tmux set-option -t "${SESSION_NAME}" pane-active-border-style "fg=#f59e0b"

    # Right column (42% width): Phi Fox Cockpit HUD
    tmux split-window -h -p 42 -t "${SESSION_NAME}:0.0" -c "${WORKSPACE_ROOT}" "${GAME_CMD}"
    # Bottom-right pane (28% height of right column): Live telemetry watcher
    tmux split-window -v -p 28 -t "${SESSION_NAME}:0.1" -c "${WORKSPACE_ROOT}" "${TELEMETRY_CMD}"
    # Focus the Phi Fox HUD pane so hotkeys work immediately
    tmux select-pane -t "${SESSION_NAME}:0.1"
fi

if [[ "${ATTACH}" -eq 1 ]]; then
    if [[ -n "${TMUX:-}" ]]; then
        exec tmux switch-client -t "${SESSION_NAME}"
    else
        exec tmux attach-session -t "${SESSION_NAME}"
    fi
else
    echo "Phi Fox Tmux Arena session '${SESSION_NAME}' is ready."
fi
