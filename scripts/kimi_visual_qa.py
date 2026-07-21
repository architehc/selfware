#!/usr/bin/env python3
"""Kimi K3 visual QA for the Selfware Evolve UI.

You run this yourself (e.g. `! python3 scripts/kimi_visual_qa.py`) so that the
screenshot upload happens under your own authority, not the agent's sandbox.

It captures a headless-Chrome screenshot of the running evolve server and sends
it to moonshotai/kimi-k3 (vision) via OpenRouter for a visual validation report.

Env:
  SELFWARE_API_KEY   OpenRouter key (falls back to the `selfware-api-key` keychain item)
  EVOLVE_URL         default http://127.0.0.1:7781
  EVOLVE_SHOT        default /tmp/evolve_shots/main.png  (reused if --no-capture)
Usage:
  python3 scripts/kimi_visual_qa.py                 # capture + validate main view
  python3 scripts/kimi_visual_qa.py --no-capture    # validate an existing PNG
  python3 scripts/kimi_visual_qa.py --shot path.png # validate a specific PNG
"""
import base64, json, os, subprocess, sys, urllib.request

URL   = os.environ.get("EVOLVE_URL", "http://127.0.0.1:7794")
SHOT  = os.environ.get("EVOLVE_SHOT", "/tmp/evolve_shots/main.png")
MODEL = os.environ.get("KIMI_MODEL", "moonshotai/kimi-k3")
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

def key():
    k = os.environ.get("SELFWARE_API_KEY")
    if k:
        return k
    return subprocess.check_output(
        ["security", "find-generic-password", "-s", "selfware-api-key", "-a", "ivo", "-w"]
    ).decode().strip()

def capture(path):
    os.makedirs(os.path.dirname(path) or ".", exist_ok=True)
    subprocess.run([CHROME, "--headless", "--disable-gpu", "--hide-scrollbars",
                    "--window-size=1600,1000", "--virtual-time-budget=6000",
                    f"--screenshot={path}", URL + "/"],
                   check=True, stderr=subprocess.DEVNULL)
    print(f"captured {path} ({os.path.getsize(path)} bytes)")

def validate(path):
    img = base64.b64encode(open(path, "rb").read()).decode()
    payload = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": "You are a meticulous visual QA validator "
             "for web UIs. Report only what is visibly rendered. If something looks "
             "broken, blank, misaligned, or errored, say so."},
            {"role": "user", "content": [
                {"type": "text", "text": "This is the 'Selfware Evolve' code-IDE web app. "
                 "Visually validate it: (1) list the UI regions/components you can see, "
                 "(2) does the code editor and file explorer appear populated and rendered "
                 "correctly, (3) any visible errors, blank panels, or layout problems? "
                 "Be concise and specific."},
                {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{img}"}},
            ]},
        ],
        "max_tokens": 700,
    }
    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/chat/completions",
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {key()}", "Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=120))
    print("MODEL:", r.get("model"), "| USAGE:", r.get("usage"))
    print("--- KIMI K3 VISUAL VALIDATION ---")
    print(r["choices"][0]["message"]["content"])

if __name__ == "__main__":
    args = sys.argv[1:]
    shot = SHOT
    if "--shot" in args:
        shot = args[args.index("--shot") + 1]
    if "--no-capture" not in args:
        capture(shot)
    validate(shot)
