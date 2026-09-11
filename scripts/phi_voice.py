#!/usr/bin/env python3
"""Generate a local VibeVoice WAV and an explicitly approximate mascot track.

Uses Phi's installed, pinned seven-graph ONNX runtime. Word placement is a
proportional estimate; energy-derived mouth shapes are animation heuristics,
not measured phonemes. No forced aligner is implemented in this helper.

Install once with scripts/setup_phi_vibevoice.py, then run with the isolated
runtime Python:
    python scripts/phi_voice.py --text "Every run starts hardened." \
        --preset Emma --out /tmp/phi_seg
    python scripts/phi_voice.py --selftest
"""
from __future__ import annotations
import argparse, json, math, os, struct, sys, wave
from pathlib import Path

PRESETS = ["Carter", "Davis", "Emma", "Frank", "Grace", "Mike"]
PHI_DEFAULT_PRESET = "Emma"     # bright, small-creature timbre; "Grace" is a good alt
SR = 24000
REPO = "elbruno/VibeVoice-Realtime-0.5B-ONNX"

# ── viseme set the Phi renderer understands (matches phi.html VIS keys) ──
#   rest · m (closed: m/b/p) · a (open) · e (wide) · o (round) · f (teeth)
def _read_wav(path):
    with wave.open(path, "rb") as w:
        n, sr = w.getnframes(), w.getframerate()
        raw = w.readframes(n)
    import array
    a = array.array("h"); a.frombytes(raw)
    return [s / 32768.0 for s in a], sr

def pcm_to_visemes(samples, sr, fps=30):
    """RMS energy -> jaw openness; zero-crossing rate -> front/round vowel.
    Returns [{t, viseme, open}] at `fps`. Pure-python, no deps."""
    if not math.isfinite(fps) or fps <= 0 or sr <= 0:
        raise ValueError("Sample rate and animation FPS must be positive")
    hop = max(1, int(sr / fps))
    win = hop * 2
    track, i = [], 0
    # normalise to the loudest window so quiet voices still open the mouth
    peaks = []
    while i < len(samples):
        seg = samples[i:i + win]
        if seg:
            peaks.append(math.sqrt(sum(s * s for s in seg) / len(seg)))
        i += hop
    loud = max(peaks, default=0) or 1.0
    i, k = 0, 0
    while i < len(samples):
        seg = samples[i:i + win]
        if not seg:
            break
        rms = math.sqrt(sum(s * s for s in seg) / len(seg)) / loud
        # zero-crossing rate ~ high => fricative/front vowel, low => round/open
        zc = sum(1 for j in range(1, len(seg)) if (seg[j - 1] >= 0) != (seg[j] >= 0)) / len(seg)
        if rms < 0.06:
            v = "rest"
        elif zc > 0.22:
            v = "f" if rms < 0.35 else "e"      # fricative vs wide vowel
        elif rms > 0.6:
            v = "a"                             # loud + low zc => open
        elif zc < 0.08:
            v = "o"                             # round
        else:
            v = "m" if rms < 0.2 else "e"
        track.append({"t": round(k / fps, 3), "viseme": v, "open": round(min(rms, 1.0), 3)})
        i += hop; k += 1
    return track

def synthesize(text, preset=PHI_DEFAULT_PRESET, steps=20, providers=None):
    """Return real mono PCM from the installed runtime, or raise on failure."""
    from phi_vibevoice_runtime import VibeVoiceRuntime
    from setup_phi_vibevoice import REVISION
    cache = Path(os.environ.get("SELFWARE_PHI_SPEECH_CACHE", Path.home() / ".cache/selfware/vibevoice"))
    manifest_path = cache / "installation.json"
    if not manifest_path.is_file() or manifest_path.stat().st_size > 1024 * 1024:
        raise RuntimeError("Install the local model with scripts/setup_phi_vibevoice.py first")
    manifest = json.loads(manifest_path.read_text())
    if manifest.get("repo_id") != REPO or manifest.get("revision") != REVISION:
        raise RuntimeError("The local model installation does not match Phi's pinned revision")
    selected = providers or ["CPUExecutionProvider"]
    if not isinstance(selected, (list, tuple)) or not selected:
        raise ValueError("providers must be a nonempty list of ONNX provider names")
    runtime = VibeVoiceRuntime(manifest["model_dir"], provider=selected[0])
    result = runtime.synthesize(text, preset, steps=steps, timeout_seconds=180)
    if result["metadata"].get("complete") is not True:
        raise RuntimeError("The model did not complete the narration")
    return result["audio"].tolist(), result["sample_rate"]


def align_words(text, samples, sr):
    """Legacy helper name: proportional word positions, not forced alignment."""
    if sr <= 0:
        raise ValueError("Sample rate must be positive")
    words = text.split()
    duration = len(samples) / sr
    per = duration / max(len(words), 1)
    return [{"word": word, "start": round(i * per, 3), "end": round((i + 1) * per, 3)}
            for i, word in enumerate(words)]


def build_track(text, preset=PHI_DEFAULT_PRESET, focus=None, out=None):
    samples, sr = synthesize(text, preset)
    track = {
        "text": text, "preset": preset, "focus": focus, "sr": sr,
        "duration": round(len(samples) / sr, 3),
        "alignment": {"status": "unavailable", "word_timing": "audio_duration_approximate",
                      "mouth_timing": "audio_energy_heuristic"},
        "visemes": pcm_to_visemes(samples, sr),
        "words": align_words(text, samples, sr),
    }
    if out:
        _write_wav(out + ".wav", samples, sr)
        with open(out + ".json", "w") as f:
            json.dump({**track, "audio": os.path.basename(out) + ".wav"}, f, indent=2)
    return track

def _write_wav(path, samples, sr):
    with wave.open(path, "wb") as w:
        w.setnchannels(1); w.setsampwidth(2); w.setframerate(sr)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in samples))

def _selftest():
    # synth a word-like burst train (no model needed) and prove the extractor runs
    sr, out = SR, []
    import random
    for _ in range(6):
        f = random.choice([180, 320, 90, 500])         # varied "phones"
        for n in range(int(sr * 0.18)):
            out.append(0.6 * math.sin(2 * math.pi * f * n / sr))
        out += [0.0] * int(sr * 0.06)                    # gap
    vis = pcm_to_visemes(out, sr)
    kinds = sorted({v["viseme"] for v in vis})
    print(f"synthetic {len(out)/sr:.2f}s -> {len(vis)} viseme frames; shapes seen: {kinds}")
    print("  sample:", vis[:4])
    assert vis and any(v["viseme"] != "rest" for v in vis), "extractor produced no mouth motion"
    print("OK: viseme extractor works on raw PCM (no timestamps needed)")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--text"); ap.add_argument("--preset", default=PHI_DEFAULT_PRESET, choices=PRESETS)
    ap.add_argument("--focus", default=None); ap.add_argument("--out", default=None)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args()
    if a.selftest: return _selftest()
    if not a.text: sys.exit("need --text (or --selftest)")
    t = build_track(a.text, a.preset, a.focus, a.out)
    print(json.dumps({k: v for k, v in t.items() if k != "visemes"}, indent=2))
    print(f"visemes: {len(t['visemes'])} frames @30fps")

if __name__ == "__main__":
    main()
