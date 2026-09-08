#!/usr/bin/env python3
"""Send a large context pack + question to the 1M-context local endpoint.

Usage: python3 scripts/pack_query.py <pack_file> [extra_question] [out_file]

One-shot review vehicle for the 700-800k evolve packs — the multi-chat
fan-out agents have no tools, so codebase-scale review goes through the
1M endpoint with the pack inlined. Streaming SSE accumulation (some sglang
builds hang on non-streaming). Stdlib only.
"""

import json
import sys
import urllib.request

ENDPOINT = "https://llm.selfware.design/v1"
MODEL = "qwen38-flash-next"


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__)
        return 2
    pack_path = sys.argv[1]
    extra = sys.argv[2] if len(sys.argv) > 2 else ""
    out_path = sys.argv[3] if len(sys.argv) > 3 else "/dev/stdout"

    pack = open(pack_path).read()
    messages = [{"role": "user", "content": pack}]
    if extra:
        messages.append({"role": "user", "content": extra})

    body = json.dumps({
        "model": MODEL,
        "messages": messages,
        "temperature": 0.2,
        # Reasoning model: budget covers reasoning + a long-form review.
        "max_tokens": 65536,
        "stream": True,
    }).encode()
    req = urllib.request.Request(
        f"{ENDPOINT}/chat/completions",
        data=body,
        headers={"Content-Type": "application/json"},
    )
    parts = []
    with urllib.request.urlopen(req, timeout=3600) as resp:
        for raw in resp:
            line = raw.decode("utf-8", "replace").strip()
            if not line.startswith("data:"):
                continue
            payload = line[5:].strip()
            if payload == "[DONE]":
                break
            try:
                chunk = json.loads(payload)
            except json.JSONDecodeError:
                continue
            delta = chunk.get("choices", [{}])[0].get("delta", {})
            if delta.get("content"):
                parts.append(delta["content"])
    result = "".join(parts)
    with open(out_path, "w") as f:
        f.write(result + "\n")
    print(f"wrote {len(result)} chars to {out_path}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
