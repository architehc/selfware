#!/usr/bin/env python3
"""Kimi K3 visual QA for the tqec-lab educational web app.

Adapted from scripts/kimi_visual_qa.py. Captures headless-Chrome screenshots of
the running tqec-lab server (one per hash-routed view) and sends each to
moonshotai/kimi-k3 (vision) via OpenRouter for a visual validation report.

Env:
  SELFWARE_API_KEY   OpenRouter key (falls back to the `selfware-api-key` keychain item)
  TQEC_LAB_URL       default http://127.0.0.1:7837
  TQEC_LAB_SHOTS     default /tmp/tqec_lab_shots
Usage:
  python3 scripts/tqec_lab_visual_qa.py                    # capture + validate all 5 views
  python3 scripts/tqec_lab_visual_qa.py --view lattice     # one view
  python3 scripts/tqec_lab_visual_qa.py --view graph --no-capture
  python3 scripts/tqec_lab_visual_qa.py --view graph --shot path.png

Notes:
  - A fresh Chrome profile is used per run (--user-data-dir under /tmp, wiped
    before capture) so localStorage lesson progress never leaks between captures.
  - Hash routes: verified on Chrome 150 (macOS) that headless --screenshot
    honors the URL fragment when the URL is passed as a single argv element
    (no shell stripping), so captures hit the hash-routed URL directly. If a
    future Chrome strips the fragment, fall back to a wrapper HTML in /tmp
    that sets window.location (fragment included) and redirects.
  - Captures use a tall 1600x6000 window (widgets render at the bottom of long
    lesson pages, far below a 1000px fold) and the uniform-background tail is
    cropped with PIL afterwards.
  - Chrome 150 headless sometimes never exits after writing the screenshot;
    capture() polls for a stable PNG and then terminates Chrome itself.
  - Requires PIL (pillow) for the trim step.
"""
import base64, json, os, shutil, subprocess, sys, time, urllib.request

URL = os.environ.get("TQEC_LAB_URL", "http://127.0.0.1:7837").rstrip("/")
SHOT_DIR = os.environ.get("TQEC_LAB_SHOTS", "/tmp/tqec_lab_shots")
MODEL = os.environ.get("KIMI_MODEL", "moonshotai/kimi-k3")
CHROME = "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
PROFILE = "/tmp/tqec_lab_chrome_profile"

VIEWS = {
    "graph": "/",
    "lesson": "/#/lesson/why-tqec",
    "lattice": "/#/lesson/surface-code",
    "decoder": "/#/lesson/mwpm-decoding",
    "surgery": "/#/lesson/lattice-surgery",
}

# Per-view expectations, from docs/superpowers/specs/2026-08-05-tqec-lab-visual-checklist.md.
EXPECT = {
    "graph": "the curriculum map: an SVG with 19 lesson node cards arranged in 6 "
             "horizontal tier rows, straight edges connecting prerequisites, and on "
             "this fresh profile only the 'Why TQEC?' node highlighted/unlocked with "
             "all other cards dimmed. Edges must not cross through card interiors.",
    "lesson": "a text-only lesson page ('Why TQEC? The NISQ wall'): rendered markdown "
              "with headings and paragraphs, a 'mark complete' button and a 'back to "
              "map' link at the bottom. This lesson has NO interactive widget — that "
              "is correct, do not flag the absence of an SVG here.",
    "lattice": "the 'surface code' lesson with its interactive lattice widget at the "
               "bottom: an SVG d=3 rotated-code grid (dots for data qubits, colored "
               "squares for checks), X error / Z error / erase mode buttons, and a "
               "click counter or readout.",
    "decoder": "the 'MWPM decoding' lesson with its interactive decoder widget at the "
               "bottom: an SVG grid showing 4 defect markers (scenario A) and a step "
               "button that reveals matching edges.",
    "surgery": "the 'lattice surgery' lesson with its interactive widget at the "
               "bottom: two d=3 lattice patches separated by a gap, with merge/split "
               "controls.",
}


def key():
    k = os.environ.get("SELFWARE_API_KEY")
    if k:
        return k
    return subprocess.check_output(
        ["security", "find-generic-password", "-s", "selfware-api-key", "-a", "ivo", "-w"]
    ).decode().strip()


def trim_blank_bottom(path):
    """Crop the uniform-background tail off a tall capture (PIL)."""
    from PIL import Image
    im = Image.open(path)
    bg = im.getpixel((5, im.height - 5))
    px = im.load()
    last = im.height - 1
    while last > 0:
        row_blank = all(px[x, last] == bg for x in range(0, im.width, 16))
        if not row_blank:
            break
        last -= 1
    bottom = min(im.height, last + 40)  # keep a little padding
    if bottom < im.height:
        im.crop((0, 0, im.width, bottom)).save(path)


def capture(view):
    os.makedirs(SHOT_DIR, exist_ok=True)
    path = os.path.join(SHOT_DIR, f"{view}.png")
    # Fresh profile per run: localStorage progress must not leak between runs.
    shutil.rmtree(PROFILE, ignore_errors=True)
    target = URL + VIEWS[view]
    # Tall window: the interactive widget renders at the *bottom* of long
    # lesson pages, far below a 1000px fold; trim_blank_bottom() crops the
    # unused tail afterwards.
    # Chrome 150 headless writes the screenshot but then sometimes fails to
    # exit (observed: process lingers for minutes). Poll for the file to
    # appear and stabilize, then kill Chrome ourselves.
    proc = subprocess.Popen(
        [CHROME, "--headless", "--disable-gpu", "--hide-scrollbars",
         "--window-size=1600,6000", "--virtual-time-budget=8000",
         f"--user-data-dir={PROFILE}",
         f"--screenshot={path}", target],
        stderr=subprocess.DEVNULL)
    deadline = time.time() + 60
    last_size = -1
    stable = 0
    while time.time() < deadline:
        if os.path.exists(path):
            size = os.path.getsize(path)
            if size > 0 and size == last_size:
                stable += 1
                if stable >= 3:  # ~3s with an unchanged size: done
                    break
            else:
                stable = 0
            last_size = size
        time.sleep(1)
    proc.terminate()
    try:
        proc.wait(timeout=5)
    except subprocess.TimeoutExpired:
        proc.kill()
    if not os.path.exists(path) or os.path.getsize(path) == 0:
        raise RuntimeError(f"screenshot for '{view}' was not written")
    trim_blank_bottom(path)
    print(f"captured {view}: {path} ({os.path.getsize(path)} bytes)")
    return path


def validate(view, path):
    img = base64.b64encode(open(path, "rb").read()).decode()
    payload = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": "You are a meticulous visual QA validator "
             "for web UIs. Report only what is visibly rendered. If something looks "
             "broken, blank, misaligned, or errored, say so."},
            {"role": "user", "content": [
                {"type": "text", "text": f"This is the '{view}' view of an educational TQEC "
                 f"web app, captured as a full-page screenshot. It must show: {EXPECT[view]} "
                 "List the visible components, confirm the expected elements are rendered, "
                 "and flag anything blank, overlapping, or error-text. "
                 "End with a verdict line: 'VERDICT: PASS' or 'VERDICT: FAIL: <reason>'."},
                {"type": "image_url", "image_url": {"url": f"data:image/png;base64,{img}"}},
            ]},
        ],
        # K3 is a reasoning model: leave ample headroom above its reasoning
        # tokens or the visible answer comes back empty (3000 was exhausted
        # by reasoning alone on the dense graph view).
        "max_tokens": 8000,
    }
    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/chat/completions",
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {key()}", "Content-Type": "application/json"})
    r = None
    for attempt in range(3):
        body = urllib.request.urlopen(req, timeout=180).read()
        try:
            r = json.loads(body)
            break
        except json.JSONDecodeError:
            # Observed: OpenRouter occasionally returns a truncated body.
            print(f"(attempt {attempt + 1}: undecodable response, {len(body)} bytes; retrying)")
            time.sleep(5)
    if r is None:
        raise RuntimeError("OpenRouter returned undecodable JSON 3 times")
    if "error" in r:
        raise RuntimeError(f"OpenRouter error: {r['error']}")
    print("MODEL:", r.get("model"), "| USAGE:", r.get("usage"))
    print(f"--- KIMI K3 VISUAL VALIDATION: {view} ---")
    msg = r["choices"][0]["message"]
    print(msg.get("content") or msg.get("reasoning") or "(empty response)")


if __name__ == "__main__":
    args = sys.argv[1:]
    views = list(VIEWS)
    if "--view" in args:
        v = args[args.index("--view") + 1]
        if v not in VIEWS:
            sys.exit(f"unknown view '{v}'; choose from {', '.join(VIEWS)}")
        views = [v]
    for view in views:
        shot = os.path.join(SHOT_DIR, f"{view}.png")
        if "--shot" in args:
            shot = args[args.index("--shot") + 1]
        if "--no-capture" not in args:
            shot = capture(view)
        validate(view, shot)
