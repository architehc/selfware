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

Wiring into phi_speech_worker (turns alignment "unavailable" -> real):
    from phi_align import align
    result = align(job.text, audio_float32, runtime.sample_rate, device="cpu")
    payload["alignment"] = result        # {status, backend, words:[{word,start,end,score}]}

CLI:
    python3 scripts/phi_align.py --wav seg.wav --text "Every run starts hardened."
    python3 scripts/phi_align.py --selftest        # fallback path, no whisperX needed
"""
from __future__ import annotations
import argparse, json, os, sys, wave

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


def main(argv=None):
    ap = argparse.ArgumentParser(description="Phi forced-alignment stage (whisperX)")
    ap.add_argument("--wav"); ap.add_argument("--text")
    ap.add_argument("--device", default="cpu", help="cpu / cuda / mps")
    ap.add_argument("--language", default="en")
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
        return
    if not (a.wav and a.text):
        sys.exit("need --wav and --text (or --selftest)")
    print(json.dumps(align(a.text, a.wav, device=a.device, language=a.language), indent=2))


if __name__ == "__main__":
    main()
