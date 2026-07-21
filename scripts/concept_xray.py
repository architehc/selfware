#!/usr/bin/env python3
"""Ontology X-ray for a selected code concept.

Select a concept (a type/trait/entity) and get its grounded ontological
neighborhood: where it's defined, who implements it, where it's referenced,
which concepts co-occur with it, and its module footprint. Every fact cites a
real file:line — no hallucinated relationships.

Usage:
  python3 scripts/concept_xray.py <Concept>          # xray one concept
  python3 scripts/concept_xray.py --list [substr]    # list concept vocabulary
  python3 scripts/concept_xray.py --hubs             # most-connected concepts
"""
import os, re, sys
from collections import defaultdict, Counter

DEF_RE  = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(struct|enum|trait|type)\s+([A-Z][A-Za-z0-9_]*)')
IMPL_RE = re.compile(r'^\s*impl(?:<[^>]*>)?\s+(?:([A-Z][A-Za-z0-9_]*)(?:<[^>]*>)?\s+for\s+)?([A-Z][A-Za-z0-9_]*)')
WORD    = lambda name: re.compile(r'\b' + re.escape(name) + r'\b')

def walk_src():
    for root, _, fs in os.walk("src"):
        for f in fs:
            if f.endswith(".rs"):
                yield os.path.join(root, f)

def module_of(path):
    p = path[len("src/"):] if path.startswith("src/") else path
    p = p[:-3] if p.endswith(".rs") else p
    parts = [x for x in p.split("/") if x and x != "mod"]
    return "::".join(parts) or "crate"

def index():
    defs = defaultdict(list)          # Concept -> [(path,line,kind)]
    impls = defaultdict(list)         # Concept -> [(trait_or_None, path,line)]
    lines_by_file = {}
    for path in walk_src():
        txt = open(path, encoding="utf-8", errors="ignore").read().splitlines()
        lines_by_file[path] = txt
        for i, line in enumerate(txt, 1):
            m = DEF_RE.match(line)
            if m:
                defs[m.group(2)].append((path, i, m.group(1)))
            im = IMPL_RE.match(line)
            if im:
                impls[im.group(2)].append((im.group(1), path, i))
    return defs, impls, lines_by_file

def xray(concept):
    defs, impls, lbf = index()
    if concept not in defs:
        near = sorted(c for c in defs if concept.lower() in c.lower())
        print(f"'{concept}' is not a defined type." +
              (f" Did you mean: {', '.join(near[:8])}" if near else ""))
        return
    wre = WORD(concept)
    # references per module (exclude the definition lines themselves)
    refs = Counter()
    ref_files = set()
    cooccur = Counter()
    concept_names = set(defs)
    for path, txt in lbf.items():
        hits = sum(1 for line in txt if wre.search(line))
        if hits:
            refs[module_of(path)] += hits
            ref_files.add(path)
    # co-occurring concepts: other defined types appearing in files that use this one
    for path in ref_files:
        present = {c for c in concept_names if c != concept and WORD(c).search("\n".join(lbf[path]))}
        for c in present:
            cooccur[c] += 1

    print(f"═══ ONTOLOGY X-RAY: {concept} ═══\n")
    print(f"DEFINITION ({len(defs[concept])} site{'s' if len(defs[concept])>1 else ''}):")
    for p, ln, kind in defs[concept]:
        print(f"    {kind:6} {concept}   @ {p}:{ln}")

    implementors = [(t, p, ln) for (t, p, ln) in impls.get(concept, []) if t]
    inherent     = [(p, ln) for (t, p, ln) in impls.get(concept, []) if not t]
    if implementors:
        print(f"\nTRAITS IMPLEMENTED FOR {concept} ({len(implementors)}):")
        for t, p, ln in implementors[:12]:
            print(f"    impl {t} for {concept}   @ {module_of(p)}")
    # who implements THIS (if it's a trait): impl <concept> for X
    impls_of_trait = []
    for target, lst in impls.items():
        for (t, p, ln) in lst:
            if t == concept:
                impls_of_trait.append((target, module_of(p)))
    if impls_of_trait:
        print(f"\nIMPLEMENTORS OF trait {concept} ({len(impls_of_trait)}):")
        for target, mod in impls_of_trait[:14]:
            print(f"    {target}   ({mod})")

    top_refs = refs.most_common(12)
    print(f"\nREFERENCED IN {len(refs)} modules ({sum(refs.values())} mentions):")
    for mod, n in top_refs:
        print(f"    {n:4}×  {mod}")

    print(f"\nRELATED CONCEPTS (co-occur in the same files):")
    for c, n in cooccur.most_common(12):
        kinds = ",".join(sorted({k for _, _, k in defs[c]}))
        print(f"    {n:3} files  {c:28} [{kinds}]")

    # module footprint
    home = defs[concept][0][0]
    print(f"\nFOOTPRINT: defined in {module_of(home)}/ · "
          f"used across {len({m.split('::')[0] for m in refs})} top-level modules")

def list_concepts(substr=None):
    defs, _, _ = index()
    items = sorted(defs)
    if substr:
        items = [c for c in items if substr.lower() in c.lower()]
    print(f"{len(items)} concepts:")
    for c in items:
        print(f"    {c:32} ({len(defs[c])} def, {defs[c][0][2]})")

def hubs():
    defs, impls, lbf = index()
    concept_names = set(defs)
    conn = Counter()
    for path, txt in lbf.items():
        blob = "\n".join(txt)
        present = [c for c in concept_names if WORD(c).search(blob)]
        for c in present:
            conn[c] += 1
    print("MOST-CONNECTED CONCEPTS (appear in most files):")
    for c, n in conn.most_common(20):
        print(f"    {n:4} files  {c}")

if __name__ == "__main__":
    a = sys.argv[1:]
    if not a: print(__doc__)
    elif a[0] == "--list": list_concepts(a[1] if len(a) > 1 else None)
    elif a[0] == "--hubs": hubs()
    else: xray(a[0])
