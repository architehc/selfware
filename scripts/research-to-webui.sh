#!/bin/bash
# Import a finished deep-research run into Open WebUI (:3000) as a Note,
# so reports live alongside the persistent chat history.
#   scripts/research-to-webui.sh [research/<run-dir>]   (default: latest run)
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

RUN_DIR="${1:-$(ls -td research/2*/ 2>/dev/null | head -1)}"
[ -n "$RUN_DIR" ] && [ -f "$RUN_DIR/REPORT.md" ] || {
  echo "no completed run (REPORT.md) found in ${RUN_DIR:-research/}" >&2; exit 1; }

WEBUI="${WEBUI:-http://127.0.0.1:3000}"

python3 - "$RUN_DIR" <<'EOF'
import glob, json, os, sys, urllib.request

run = sys.argv[1].rstrip("/")
webui = os.environ.get("WEBUI", "http://127.0.0.1:3000")

def api(path, payload=None, token=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = json.dumps(payload).encode() if payload is not None else None
    req = urllib.request.Request(webui + path, data=data, headers=headers)
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)

token = api("/api/v1/auths/signin", {"email": "", "password": ""})["token"]

question = open(f"{run}/QUESTION.txt").read().strip()
body = [f"# {question}", "", f"*Run: `{run}`*", "", "## Report", "",
        open(f"{run}/REPORT.md").read().strip(), "", "## Collected notes", ""]
for f in sorted(glob.glob(f"{run}/*-notes.md")):
    body += [f"### {os.path.basename(f)}", "", open(f).read().strip(), ""]

title = "Research: " + (question if len(question) <= 80 else question[:77] + "...")
note = api("/api/v1/notes/create", {"title": title,
           "data": {"content": {"md": "\n".join(body)}}}, token)
print(f"imported -> Open WebUI note '{title}' (id {note['id']})")
EOF
