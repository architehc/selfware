#!/usr/bin/env python3
"""Change-impact analysis over the code graph (fine-grained, file-level).

"What does changing/deleting node X trigger?" — builds an accurate reverse
dependency graph by resolving each `use crate::a::b::c` path to the exact file
module that defines it (longest-prefix match against real files), then computes
the transitive blast radius. Grounded: every impacted node is reached through a
real, resolved `use` edge.

Usage:
  python3 scripts/impact_analysis.py <module::path or substring>
  python3 scripts/impact_analysis.py --top          # rank by blast radius
  python3 scripts/impact_analysis.py --leaves       # true safe-delete leaves
  python3 scripts/impact_analysis.py --self-check    # edge-resolution coverage
"""
import os, re, sys
from collections import defaultdict, deque

USE_RE = re.compile(r'^\s*(?:pub\s+)?use\s+(crate|super|self)((?:::(?:r#)?[A-Za-z_][A-Za-z0-9_]*)+)')
MOD_RE = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?mod\s+(?:r#)?([A-Za-z_][A-Za-z0-9_]*)\s*;')

def module_id(relpath):
    """src/config/loader.rs -> 'config::loader'; src/config/mod.rs -> 'config'."""
    p = relpath[len("src/"):] if relpath.startswith("src/") else relpath
    p = p[:-3] if p.endswith(".rs") else p
    parts = [x for x in p.split("/") if x]
    if parts and parts[-1] == "mod":
        parts = parts[:-1]
    return "::".join(parts) if parts else "crate"

def build():
    files = {}           # module_id -> relpath
    for root, _, fs in os.walk("src"):
        for f in fs:
            if f.endswith(".rs"):
                rp = os.path.join(root, f)
                files[module_id(rp)] = rp
    idset = set(files)

    def resolve(segments):
        """Longest-prefix match of a :: path to a real file module id."""
        for cut in range(len(segments), 0, -1):
            cand = "::".join(segments[:cut])
            if cand in idset:
                return cand
        return None

    fwd = defaultdict(set)   # who-depends-on-what
    rev = defaultdict(set)   # what-is-depended-on-by
    unresolved = 0
    for mid, rp in files.items():
        parent = mid.rsplit("::", 1)[0] if "::" in mid else ""
        # lib.rs / main.rs are crate roots: their `mod X;` names a top-level module.
        base = [] if mid in ("crate", "lib", "main") else mid.split("::")
        for line in open(rp, encoding="utf-8", errors="ignore"):
            # `mod child;` — F structurally depends on the child module's existence
            # (deleting the child leaves a dangling declaration in F).
            md = MOD_RE.match(line)
            if md:
                child = resolve(base + [md.group(1)])
                if child and child != mid:
                    fwd[mid].add(child); rev[child].add(mid)
                continue
            m = USE_RE.match(line)
            if not m:
                continue
            anchor, tail = m.group(1), m.group(2).strip(":")
            segs = [s[2:] if s.startswith("r#") else s for s in tail.split("::")]
            if anchor == "crate":
                target_segs = segs
            elif anchor == "self":
                target_segs = (mid.split("::") if mid != "crate" else []) + segs
            else:  # super
                target_segs = (parent.split("::") if parent else []) + segs
            tgt = resolve(target_segs)
            if tgt and tgt != mid:
                fwd[mid].add(tgt); rev[tgt].add(mid)
            elif not tgt:
                unresolved += 1
    return files, fwd, rev, unresolved

def blast(rev, start):
    seen, depth = set(), {}
    q = deque([(start, 0)])
    while q:
        node, dist = q.popleft()
        for user in rev.get(node, ()):
            if user not in seen:
                seen.add(user); depth[user] = dist + 1
                q.append((user, dist + 1))
    return depth

def resolve_query(files, rev, q):
    if q in files: return q
    hits = sorted([m for m in files if q in m], key=lambda n: (-len(rev.get(n, ())), n))
    return hits[0] if hits else None

def report(q):
    files, fwd, rev, _ = build()
    node = resolve_query(files, rev, q)
    if not node: print(f"no module matching '{q}'"); return
    direct = sorted(rev.get(node, ()))
    depth = blast(rev, node)
    is_test = files[node].endswith("_test.rs")
    print(f"IMPACT OF CHANGING: {node}   ({files[node]})")
    print(f"\n  direct dependents (1 hop): {len(direct)}")
    for u in direct[:14]: print(f"    - {u}")
    if len(direct) > 14: print(f"    … +{len(direct)-14} more")
    print(f"\n  TOTAL BLAST RADIUS (transitive): {len(depth)} modules")
    byd = defaultdict(int)
    for d in depth.values(): byd[d] += 1
    for d in sorted(byd)[:6]: print(f"    hop {d}: {byd[d]}")
    v = ("LEAF — nothing imports it; safe to change/delete" if not direct else
         "HIGH-IMPACT — wide ripple; change with care" if len(depth) > 15 else
         "MODERATE — bounded ripple")
    print(f"\n  verdict: {v}" + ("  [test file]" if is_test else ""))

def top(k=18):
    files, fwd, rev, unres = build()
    scored = sorted(((len(blast(rev, m)), len(rev.get(m, ())), m) for m in files), reverse=True)
    print(f"TOP {k} HIGHEST-IMPACT MODULES (transitive blast radius):")
    print(f"  {'radius':>7}{'direct':>7}  module")
    for r, dn, m in scored[:k]:
        print(f"  {r:7}{dn:7}  {m}")

def leaves():
    files, fwd, rev, _ = build()
    prod = {m: p for m, p in files.items() if not p.endswith("_test.rs")}
    true_leaves = sorted(m for m in prod if not rev.get(m))
    print(f"production modules: {len(prod)}")
    print(f"true safe-delete leaves (0 dependents, non-test): {len(true_leaves)}")
    for m in true_leaves[:25]: print(f"    - {m}  ({files[m]})")
    if len(true_leaves) > 25: print(f"    … +{len(true_leaves)-25} more")

def self_check():
    files, fwd, rev, unres = build()
    edges = sum(len(v) for v in fwd.values())
    print(f"files: {len(files)}  resolved edges: {edges}  unresolved use-paths: {unres}")
    print(f"resolution coverage: {100*edges/(edges+unres):.1f}%")

if __name__ == "__main__":
    a = sys.argv[1:]
    if not a: print(__doc__)
    elif a[0] == "--top": top()
    elif a[0] == "--leaves": leaves()
    elif a[0] == "--self-check": self_check()
    else: report(a[0])
