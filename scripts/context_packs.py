#!/usr/bin/env python3
"""Build tiered context representations of selfware at fixed token budgets and
(optionally) ask Kimi K3 for extension suggestions at each depth.

The premise: to design a new feature you need different amounts of context —
just the harness *interfaces* at a small budget, interfaces plus the *detailed
code* of relevant subsystems at a medium budget, or *all the code* at a large
one. This assembles a pack for each budget (interface skeleton always included,
then full file bodies added core-first until the budget is hit) and runs the
budget × query matrix against OpenRouter Kimi K3.

You run it (your key, your code → OpenRouter):
  python3 scripts/context_packs.py --dry-run                 # build packs, report sizes only
  python3 scripts/context_packs.py                           # full matrix vs Kimi K3
  python3 scripts/context_packs.py --budgets 64 128 256      # subset (k tokens)

Env: SELFWARE_API_KEY (or falls back to the `selfware-api-key` keychain item).
"""
import json, os, re, subprocess, sys, urllib.request
from pathlib import Path

import tiktoken

BUDGETS_K = [64, 128, 256, 512, 1024, 2048]   # token budgets (thousands)
_ENC = tiktoken.get_encoding("cl100k_base")    # precise token counts


def tok(text):
    return len(_ENC.encode(text, disallowed_special=()))
MODEL = os.environ.get("KIMI_MODEL", "moonshotai/kimi-k3")
KIMI_CONTEXT_K = 1024                           # Kimi K3 window; larger packs are noted, not sent

# Core-first ordering for greedy body inclusion — the subsystems you most need
# detail on to extend the harness.
CORE_ORDER = ["lib.rs", "main.rs", "agent", "api", "evolve", "tools", "safety",
              "cognitive", "config", "orchestration", "mcp", "session", "memory"]

SIG_RE = re.compile(
    r'^\s*(?:pub(?:\([^)]*\))?\s+)(?:async\s+|unsafe\s+|const\s+)*'
    r'(fn|struct|enum|trait|type|const|static)\s+[A-Za-z_]\w*'
)
IMPL_RE = re.compile(r'^\s*impl(?:<[^>]*>)?\s+')
DOC_RE = re.compile(r'^\s*(///|//!)')

QUERIES = {
    "resilience": "Suggest 3 concrete features to make the agent loop more resilient to provider/API quirks. For each: what to build, which selfware modules/interfaces it touches, and a rough implementation sketch.",
    "observability": "Propose 3 observability features (metrics, tracing, or debugging surfaces) that fit selfware's existing architecture. Name the exact modules and interfaces each would extend.",
    "context_mgmt": "Suggest 3 improvements to selfware's context management / token budgeting. Ground each in the real code you can see and cite the modules involved.",
}


def key():
    k = os.environ.get("SELFWARE_API_KEY")
    if k:
        return k
    return subprocess.check_output(
        ["security", "find-generic-password", "-s", "selfware-api-key", "-a", "ivo", "-w"]
    ).decode().strip()


def collect_files(include_tests):
    files = []
    for p in sorted(Path("src").rglob("*.rs")):
        if not include_tests and p.name.endswith("_test.rs"):
            continue
        files.append((str(p), p.read_text(errors="ignore")))
    return files


def skeleton(path, content):
    """Compact interface view of a file: module doc + pub signatures + impl heads."""
    out = [f"// ==== {path} ===="]
    lines = content.splitlines()
    for i, line in enumerate(lines):
        if DOC_RE.match(line) and i + 1 < len(lines) and (
            SIG_RE.match(lines[i + 1]) or IMPL_RE.match(lines[i + 1]) or line.strip().startswith("//!")
        ):
            out.append(line.rstrip())
        elif SIG_RE.match(line) or IMPL_RE.match(line):
            out.append(line.rstrip().rstrip("{").rstrip() + " { … }")
    return "\n".join(out) + "\n" if len(out) > 1 else ""


def core_rank(path):
    for i, key_ in enumerate(CORE_ORDER):
        if path == f"src/{key_}" or f"/{key_}/" in path or path.endswith(f"/{key_}") or path == f"src/{key_}":
            return i
    return len(CORE_ORDER) + 1


HEADER = (
    "You are extending SELFWARE, a Rust CLI AI-agent framework (~200K LOC). "
    "Below is a representation of its codebase at a fixed context budget: an "
    "interface skeleton of every module, plus full source for the subsystems "
    "most relevant to the request. Use only what you can see; cite modules by path.\n\n"
)

STOP = set(
    "the a an of to for and or in on with how what which make more into some this that "
    "detail suggest propose concrete feature features each its rough sketch name exact "
    "module modules interface interfaces you would have knowledge about improve improvements "
    "fit existing architecture given real ground cite involved touches build rough".split()
)


def query_terms(query):
    return {w for w in re.findall(r"[a-z_]{4,}", query.lower()) if w not in STOP}


def relevance(path, content, terms):
    """How relevant a file is to the query — path hits weigh heavily."""
    if not terms:
        return 0
    pl, cl = path.lower(), content.lower()
    return sum(8 * pl.count(t) + cl.count(t) for t in terms)


def build_pack(budget_k, query="", include_tests=False):
    budget = budget_k * 1000
    files = collect_files(include_tests)
    terms = query_terms(query)

    # Query-aware order: most relevant first, core order + path as tiebreakers.
    ordered = sorted(
        files,
        key=lambda f: (-relevance(f[0], f[1], terms), core_rank(f[0]), f[0]),
    )

    skel = {p: (skeleton(p, c) or f"// ==== {p} (no public API) ====\n") for p, c in files}
    full = {p: f"\n// ==== FULL: {p} ====\n{c}\n" for p, c in files}
    skel_tok = {p: tok(s) for p, s in skel.items()}
    full_tok = {p: tok(f) for p, f in full.items()}

    chosen = {}
    used = tok(HEADER)

    # Pass 1 (breadth): interface skeletons, relevance-first, until budget.
    for p, _ in ordered:
        if used + skel_tok[p] <= budget:
            chosen[p] = "skel"
            used += skel_tok[p]

    # Pass 2 (depth): upgrade to full source, relevance-first, while budget allows.
    for p, _ in ordered:
        if chosen.get(p) != "skel":
            continue
        delta = full_tok[p] - skel_tok[p]
        if delta > 0 and used + delta <= budget:
            used += delta
            chosen[p] = "full"

    parts = [HEADER]
    for p, _ in ordered:
        state = chosen.get(p)
        if state == "full":
            parts.append(full[p])
        elif state == "skel":
            parts.append(skel[p])
    pack = "".join(parts)
    full_n = sum(1 for v in chosen.values() if v == "full")
    manifest = {
        "budget_k": budget_k,
        "tokens": tok(pack),
        "full_files": full_n,
        "skeleton_files": len(chosen) - full_n,
        "omitted_files": len(files) - len(chosen),
        "total_files": len(files),
        "top_full": [p for p, _ in ordered if chosen.get(p) == "full"][:8],
    }
    return pack, manifest


def component_of(path):
    rel = path[len("src/"):] if path.startswith("src/") else path
    return rel.split("/")[0].replace(".rs", "")


def extract_functions(content):
    """Yield (name, body_text) for each `fn` via brace matching."""
    for m in re.finditer(r'\bfn\s+([A-Za-z_]\w*)', content):
        name = m.group(1)
        i = content.find("{", m.end())
        semi = content.find(";", m.end())
        if i == -1 or (semi != -1 and semi < i):
            continue
        depth, j = 0, i
        while j < len(content):
            if content[j] == "{":
                depth += 1
            elif content[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        yield name, content[i:j + 1]


def breakdown(include_tests=False):
    """Precise (tiktoken) token breakdown: per component, per file, per function."""
    files = collect_files(include_tests)
    per_file = [(p, tok(c)) for p, c in files]
    total = sum(t for _, t in per_file) or 1

    comp = {}
    for (p, _), (_, t) in zip(files, per_file):
        k = component_of(p)
        e = comp.setdefault(k, [0, 0])
        e[0] += t
        e[1] += 1

    print(f"=== PER COMPONENT (top-level module) — total {total:,} tok ===")
    print(f"  {'component':22}{'tokens':>12}{'%':>7}{'files':>7}")
    for k, (t, n) in sorted(comp.items(), key=lambda x: -x[1][0]):
        print(f"  {k:22}{t:>12,}{100*t/total:>6.1f}%{n:>7}")

    print(f"\n=== TOP FILES ===")
    for p, t in sorted(per_file, key=lambda x: -x[1])[:15]:
        print(f"  {t:>10,}  {p}")

    print(f"\n=== TOP FUNCTIONS (by tokens) ===")
    fns = []
    for p, c in files:
        for name, body in extract_functions(c):
            fns.append((tok(body), name, p))
    for t, name, p in sorted(fns, reverse=True)[:15]:
        print(f"  {t:>8,}  {name}()  {p}")
    print(f"\n  functions analyzed: {len(fns):,}  |  production tokens (real): {total:,}")


def ask_kimi(pack, query):
    payload = {
        "model": MODEL,
        "messages": [
            {"role": "system", "content": pack},
            {"role": "user", "content": query},
        ],
        "max_tokens": 1500,
    }
    req = urllib.request.Request(
        "https://openrouter.ai/api/v1/chat/completions",
        data=json.dumps(payload).encode(),
        headers={"Authorization": f"Bearer {key()}", "Content-Type": "application/json"})
    r = json.load(urllib.request.urlopen(req, timeout=300))
    return r["choices"][0]["message"]["content"], r.get("usage", {})


def main():
    args = sys.argv[1:]
    if "--breakdown" in args:
        breakdown(include_tests="--tests" in args)
        return

    dry = "--dry-run" in args
    budgets = BUDGETS_K
    if "--budgets" in args:
        i = args.index("--budgets")
        budgets = [int(x) for x in args[i + 1:] if x.isdigit()]

    outdir = Path("context_pack_results")
    # Query-aware: the pack is rebuilt per query so the budget is spent on the
    # files most relevant to that request.
    for qname, query in QUERIES.items():
        print(f"\n### query: {qname}")
        print(f"{'budget':>8} {'tokens':>10} {'full':>5} {'skel':>5}  top full-source files (relevance-ranked)")
        for b in budgets:
            pack, m = build_pack(b, query=query, include_tests=(b >= 2048))
            top = ", ".join(component_of(p) for p in m["top_full"][:5]) or "(none)"
            print(f"{b:>6}k {m['tokens']:>10,} {m['full_files']:>5} {m['skeleton_files']:>5}  {top}")
            if dry:
                continue
            if b > KIMI_CONTEXT_K:
                print(f"       skip {b}k: exceeds Kimi K3 1M window")
                continue
            outdir.mkdir(exist_ok=True)
            try:
                answer, usage = ask_kimi(pack, query)
                f = outdir / f"{b}k_{qname}.md"
                f.write_text(f"# {b}k · {qname}\n\nprompt_tokens={usage.get('prompt_tokens')}\n\n{answer}\n")
                print(f"       ✓ -> {f}  (prompt {usage.get('prompt_tokens')} tok)")
            except Exception as e:
                print(f"       ✗ {b}k/{qname}: {e}")


if __name__ == "__main__":
    main()
