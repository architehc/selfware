"""Shared SSE response reader with wall-clock deadline enforcement."""

import json
import time
from typing import Dict, List, Tuple


def read_sse_response(
    resp,
    deadline: float,
    per_read_timeout: float = 600.0,
) -> Tuple[List[str], Dict]:
    """Read SSE stream from HTTP response, accumulating deltas and usage.

    Enforces wall-clock deadline across chunk reads using read1 (incremental reads)
    and dynamically clamps the underlying socket timeout so trickling data
    cannot outlast the overall deadline.
    """
    parts = []
    usage = {}

    if hasattr(resp, "read") or hasattr(resp, "read1"):
        buffer = b""
        done = False
        read_fn = getattr(resp, "read1", getattr(resp, "read", None))

        while not done:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("stream exceeded wall clock deadline")

            if hasattr(resp, "fp") and hasattr(resp.fp, "raw") and hasattr(resp.fp.raw, "_sock"):
                try:
                    resp.fp.raw._sock.settimeout(min(per_read_timeout, max(0.01, remaining)))
                except Exception:
                    pass

            chunk = read_fn(4096)
            if not chunk:
                break
            buffer += chunk

            while b"\n" in buffer:
                raw, buffer = buffer.split(b"\n", 1)
                line = raw.decode("utf-8", "replace").strip()
                if not line.startswith("data:"):
                    continue
                payload = line[5:].strip()
                if payload == "[DONE]":
                    done = True
                    break
                try:
                    chunk_obj = json.loads(payload)
                except json.JSONDecodeError:
                    continue
                if chunk_obj.get("usage"):
                    usage = chunk_obj["usage"]
                choices = chunk_obj.get("choices") or []
                delta = choices[0].get("delta", {}) if choices else {}
                if delta.get("content"):
                    parts.append(delta["content"])

        if buffer and not done:
            line = buffer.decode("utf-8", "replace").strip()
            if line.startswith("data:"):
                payload = line[5:].strip()
                if payload != "[DONE]":
                    try:
                        chunk_obj = json.loads(payload)
                        if chunk_obj.get("usage"):
                            usage = chunk_obj["usage"]
                        choices = chunk_obj.get("choices") or []
                        delta = choices[0].get("delta", {}) if choices else {}
                        if delta.get("content"):
                            parts.append(delta["content"])
                    except json.JSONDecodeError:
                        pass
    else:
        for raw in resp:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("stream exceeded wall clock deadline")
            line = raw.decode("utf-8", "replace").strip() if isinstance(raw, bytes) else raw.strip()
            if not line.startswith("data:"):
                continue
            payload = line[5:].strip()
            if payload == "[DONE]":
                break
            try:
                chunk_obj = json.loads(payload)
            except json.JSONDecodeError:
                continue
            if chunk_obj.get("usage"):
                usage = chunk_obj["usage"]
            choices = chunk_obj.get("choices") or []
            delta = choices[0].get("delta", {}) if choices else {}
            if delta.get("content"):
                parts.append(delta["content"])

    return parts, usage
