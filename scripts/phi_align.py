#!/usr/bin/env python3
"""phi_align.py — the alignment stage the Phi voice stack leaves open.

phi_vibevoice_runtime.py / phi_speech_worker.py synthesize VibeVoice audio and
report `alignment: "unavailable"` on purpose — VibeVoice emits no timestamps and
they don't fabricate any. This module fills that hole: forced alignment of the
KNOWN narration text against the produced audio, yielding per-word timestamps
for Phi's word-highlight and region spotlight.

Backend: whisperX (wav2vec2 CTC forced alignment). It is an OPTIONAL, lazily
imported dependency — the base voice stack stays onnxruntime-only. If whisperX
isn't installed, align() degrades to an even time distribution and says so in
`status`, so callers never crash.

    pip install "whisperx>=3.1"        # pulls torch — heavier than the base stack

`visemes_from_audio()` is the second, independent stage: mouth shapes derived
from the audio itself (RMS envelope -> jaw openness, zero-crossing rate ->
front/round). It needs no dictionary, no timestamps and no aligner, so it is
the only path that keeps lips on the actual waveform when alignment falls back.
Its output is explicitly an animation heuristic, not measured phonemes, and it
emits the viseme keys phi_rig.js accepts (see VISEMES there) so the renderer
does not silently drop frames to rest.

Wiring into phi_speech_worker (turns alignment "unavailable" -> real):
    from phi_align import align
    result = align(job.text, audio_float32, runtime.sample_rate, device="cpu")
    payload["alignment"] = result        # {status, backend, words:[{word,start,end,score}]}

CLI:
    python3 scripts/phi_align.py --wav seg.wav --text "Every run starts hardened."
    python3 scripts/phi_align.py --wav seg.wav --visemes
    python3 scripts/phi_align.py --selftest        # fallback path, no whisperX needed
"""
from __future__ import annotations
import argparse, json, math, os, sys, wave

_ALIGN_MODELS = {}   # (language, device) -> (model, metadata); loading is expensive


def _read_wav(path):
    with wave.open(path, "rb") as w:
        sr, n = w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    import array
    a = array.array("h"); a.frombytes(raw)
    return [s / 32768.0 for s in a], sr


def _resample_16k(samples, sr):
    import numpy as np
    x = np.asarray(samples, dtype="float32")
    if sr == 16000 or len(x) < 2:
        return x
    n = max(2, int(round(len(x) * 16000 / sr)))
    return np.interp(np.linspace(0, len(x) - 1, n), np.arange(len(x)), x).astype("float32")


def _even(text, dur):
    """Deterministic fallback: distribute words evenly across the duration."""
    words = text.split()
    per = dur / max(len(words), 1)
    return [{"word": w, "start": round(i * per, 3), "end": round((i + 1) * per, 3), "score": None}
            for i, w in enumerate(words)]


def align(text, audio, sample_rate=None, *, device="cpu", language="en"):
    """Forced-align `text` to `audio` -> word timestamps.

    `audio` may be a path to a wav, or a float sample sequence (+ sample_rate).
    Returns {status, backend, words:[{word,start,end,score}]} — a shape that
    drops straight into the worker's `alignment` field. Never raises: on any
    failure it returns the even-distribution fallback with status "fallback".
    """
    text = (text or "").strip()

    # normalise audio -> (samples, sr, path)
    path = audio if isinstance(audio, str) else None
    if path:
        if not os.path.exists(path):
            return {"status": "fallback", "backend": "even-distribution",
                    "reason": "audio path missing", "words": _even(text, 0.0)}
        samples, sample_rate = _read_wav(path)
    else:
        samples = list(audio or [])
    if not text or not samples or not sample_rate:
        return {"status": "fallback", "backend": "even-distribution",
                "reason": "empty text or audio", "words": _even(text, 0.0)}
    dur = len(samples) / sample_rate

    try:
        import whisperx
        key = (language, device)
        if key not in _ALIGN_MODELS:
            _ALIGN_MODELS[key] = whisperx.load_align_model(language_code=language, device=device)
        model_a, metadata = _ALIGN_MODELS[key]
        audio16 = whisperx.load_audio(path) if path else _resample_16k(samples, sample_rate)
        # one segment spanning the clip == forced alignment of our exact text
        segments = [{"text": text, "start": 0.0, "end": round(dur, 3)}]
        res = whisperx.align(segments, model_a, metadata, audio16, device,
                             return_char_alignments=False)
        words = [{"word": w.get("word", ""),
                  "start": round(float(w["start"]), 3),
                  "end": round(float(w["end"]), 3),
                  "score": round(float(w.get("score", 0.0)), 3)}
                 for w in res.get("word_segments", [])
                 if w.get("start") is not None and w.get("end") is not None]
        if words:
            return {"status": "aligned", "backend": "whisperx-wav2vec2", "words": words}
        return {"status": "fallback", "backend": "even-distribution",
                "reason": "whisperX returned no word timings", "words": _even(text, dur)}
    except Exception as e:  # noqa: BLE001 — never crash the voice stack over alignment
        return {"status": "fallback", "backend": "even-distribution",
                "reason": f"{type(e).__name__}: {e}", "words": _even(text, dur)}


# ── audio-derived mouth shapes ────────────────────────────────────────────────
# Keys phi_rig.js VISEMES accepts. setViseme() drops anything else to REST, so a
# private shorthand here would render as a closed mouth for the whole clip.
V_REST, V_MBP, V_AI, V_E, V_O, V_FV = "rest", "mbp", "ai", "e", "o", "fv"


def visemes_from_audio(samples, sample_rate, fps=30):
    """RMS envelope -> jaw openness; zero-crossing rate -> front/round vowel.

    Returns [{t, viseme, open}] at `fps`. Pure stdlib, no model and no
    timestamps required — this is what keeps the mouth on the real waveform
    when `align()` has degraded to even distribution. The shapes are animation
    heuristics derived from energy, NOT measured phonemes; label them as such
    wherever they are surfaced.
    """
    if not math.isfinite(fps) or fps <= 0 or sample_rate <= 0:
        raise ValueError("Sample rate and animation FPS must be positive")
    samples = list(samples or [])
    if not samples:
        return []
    hop = max(1, int(sample_rate / fps))
    win = hop * 2
    # Normalise to the loudest window so a quiet narration still opens the mouth.
    peaks = [math.sqrt(sum(v * v for v in seg) / len(seg))
             for seg in (samples[i:i + win] for i in range(0, len(samples), hop)) if seg]
    loud = max(peaks, default=0.0) or 1.0

    track = []
    for frame, start in enumerate(range(0, len(samples), hop)):
        seg = samples[start:start + win]
        if not seg:
            break
        rms = math.sqrt(sum(v * v for v in seg) / len(seg)) / loud
        crossings = sum(1 for j in range(1, len(seg)) if (seg[j - 1] >= 0) != (seg[j] >= 0)) / len(seg)
        if rms < 0.06:
            viseme = V_REST
        elif crossings > 0.22:
            viseme = V_FV if rms < 0.35 else V_E      # fricative vs wide vowel
        elif rms > 0.6:
            viseme = V_AI                             # loud + low crossing rate => open jaw
        elif crossings < 0.08:
            viseme = V_O                              # round
        else:
            viseme = V_MBP if rms < 0.2 else V_E
        track.append({"t": round(frame / fps, 3), "viseme": viseme, "open": round(min(rms, 1.0), 3)})
    return track


def main(argv=None):
    ap = argparse.ArgumentParser(description="Phi forced-alignment stage (whisperX)")
    ap.add_argument("--wav"); ap.add_argument("--text")
    ap.add_argument("--device", default="cpu", help="cpu / cuda / mps")
    ap.add_argument("--language", default="en")
    ap.add_argument("--visemes", action="store_true",
                    help="emit audio-derived mouth shapes instead of word timings")
    ap.add_argument("--fps", type=float, default=30.0)
    ap.add_argument("--selftest", action="store_true")
    a = ap.parse_args(argv)
    if a.selftest:
        import math
        sr = 24000
        pcm = [0.5 * math.sin(2 * math.pi * 200 * n / sr) for n in range(int(sr * 0.9))]
        r = align("every run starts hardened", pcm, sr)
        print(json.dumps(r, indent=2))
        assert r["status"] == "fallback" and len(r["words"]) == 4, "fallback broken"
        print("OK: degrades cleanly to even distribution when whisperX is absent")
        track = visemes_from_audio(pcm, sr)
        shapes = sorted({f["viseme"] for f in track})
        print(f"visemes: {len(track)} frames @{30}fps; shapes seen: {shapes}")
        assert track and any(f["viseme"] != "rest" for f in track), "extractor produced no mouth motion"
        print("OK: audio-derived mouth shapes run on raw PCM (no timestamps needed)")
        return
    if a.visemes:
        if not a.wav:
            sys.exit("need --wav for --visemes")
        samples, sr = _read_wav(a.wav)
        track = visemes_from_audio(samples, sr, a.fps)
        print(json.dumps({"source": a.wav, "sr": sr, "fps": a.fps,
                          "timing": "audio_energy_heuristic",
                          "frames": len(track), "visemes": track}, indent=2))
        return
    if not (a.wav and a.text):
        sys.exit("need --wav and --text (or --selftest)")
    print(json.dumps(align(a.text, a.wav, device=a.device, language=a.language), indent=2))


if __name__ == "__main__":
    main()
