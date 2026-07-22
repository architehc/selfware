#!/usr/bin/env python3
"""Safe, compiler-verified prune of unused `pub fn`s.

Dead candidates are `pub fn`s whose name occurs only at their own definition
across ALL source roots (src, tests, examples, benches, fuzz, doctests-in-src) —
so cross-platform cfg code and test-only helpers are not false-flagged. Because
text analysis can still miss macro-built calls, the compiler is the ground
truth: functions are removed per-file and `cargo build --all-targets` must stay
green, else that file is reverted.

  python3 scripts/prune_dead_code.py --list [component]   # report candidates
  python3 scripts/prune_dead_code.py --prune <component>  # verified prune
"""
import os, re, subprocess, sys
from collections import defaultdict

ROOTS = ["src", "tests", "examples", "benches", "fuzz"]
DISPATCHED = {
    "new", "default", "fmt", "from", "into", "try_from", "try_into", "drop",
    "clone", "eq", "hash", "cmp", "partial_cmp", "deref", "deref_mut", "next",
    "poll", "main", "run", "execute", "handle", "visit", "build", "call",
}
PUB_FN = re.compile(r'^(\s*)pub(?:\([^)]*\))?\s+(?:async\s+|unsafe\s+|const\s+|extern\s+"[^"]*"\s+)*fn\s+([A-Za-z_]\w*)')
IDENT = re.compile(r'[A-Za-z_]\w*')


def all_rs():
    for root in ROOTS:
        if not os.path.isdir(root):
            continue
        for dp, _, fs in os.walk(root):
            for f in fs:
                if f.endswith(".rs"):
                    yield os.path.join(dp, f)


def occurrences():
    occ = defaultdict(int)
    for p in all_rs():
        try:
            for m in IDENT.finditer(open(p, encoding="utf-8", errors="ignore").read()):
                occ[m.group(0)] += 1
        except OSError:
            pass
    return occ


def component(path):
    rel = path[len("src/"):] if path.startswith("src/") else path
    return rel.split("/")[0].replace(".rs", "")


def find_pub_fns(path):
    """Yield (name, start_line, end_line, indent) for each pub fn, spans include
    the signature through the matching closing brace (or `;` for trait decls)."""
    lines = open(path, encoding="utf-8", errors="ignore").read().split("\n")
    i = 0
    while i < len(lines):
        m = PUB_FN.match(lines[i])
        if not m:
            i += 1
            continue
        indent, name = m.group(1), m.group(2)
        # extend start upward over contiguous #[attr] / /// doc lines
        start = i
        while start > 0 and (lines[start - 1].strip().startswith("#[")
                             or lines[start - 1].strip().startswith("///")):
            start -= 1
        # find body end: brace match, or `;` on the signature (no body)
        depth, j, opened = 0, i, False
        while j < len(lines):
            for ch in lines[j]:
                if ch == "{":
                    depth += 1
                    opened = True
                elif ch == "}":
                    depth -= 1
            if opened and depth == 0:
                break
            if not opened and lines[j].rstrip().endswith(";"):
                break
            j += 1
        yield name, start, j, indent
        i = j + 1


def candidates():
    occ = occurrences()
    defs_by_name = defaultdict(int)
    fns = []  # (name, path, start, end)
    for p in all_rs():
        if not p.startswith("src/"):
            continue
        for name, s, e, _ in find_pub_fns(p):
            defs_by_name[name] += 1
            fns.append((name, p, s, e))
    dead = []
    for name, p, s, e in fns:
        if name in DISPATCHED or name.startswith("__"):
            continue
        if occ.get(name, 0) <= defs_by_name[name]:
            dead.append((name, p, s, e))
    return dead


def cmd_list(comp=None):
    dead = candidates()
    by_comp = defaultdict(list)
    for name, p, s, e in dead:
        by_comp[component(p)].append((name, p, s + 1))
    print(f"total dead pub fns (all roots scanned): {len(dead)}")
    for c, items in sorted(by_comp.items(), key=lambda x: -len(x[1])):
        if comp and c != comp:
            continue
        print(f"\n  {c}: {len(items)}")
        for name, p, ln in sorted(items)[:40 if comp else 6]:
            print(f"    {name}()  {p}:{ln}")


def build_ok():
    return subprocess.run(
        ["cargo", "build", "--all-targets"],
        capture_output=True, text=True,
    ).returncode == 0


def remove_spans(by_file):
    """Apply removals, return {path: original_text} for revert."""
    backup = {}
    for path, spans in by_file.items():
        backup[path] = open(path, encoding="utf-8").read()
        lines = backup[path].split("\n")
        drop = set()
        for s, e, _ in spans:
            drop.update(range(s, e + 1))
        open(path, "w", encoding="utf-8").write(
            "\n".join(l for i, l in enumerate(lines) if i not in drop))
    return backup


def restore(backup):
    for path, text in backup.items():
        open(path, "w", encoding="utf-8").write(text)


def cmd_prune(comp):
    dead = candidates()
    if comp != "all":
        dead = [d for d in dead if component(d[1]) == comp]
    by_file = defaultdict(list)
    for name, p, s, e in dead:
        by_file[p].append((s, e, name))
    print(f"pruning {len(dead)} dead pub fns across {len(by_file)} files in '{comp}'")

    # Fast path: remove everything, one build.
    backup = remove_spans(by_file)
    if build_ok():
        print(f"done (batch): {len(dead)} functions pruned, build green")
        return
    restore(backup)
    print("  batch build broke — bisecting per file (some fns are reached via macros/cfg)")

    removed = 0
    for path, spans in by_file.items():
        b = remove_spans({path: spans})
        if build_ok():
            removed += len(spans)
            print(f"  ✓ {path}: removed {len(spans)}")
        else:
            restore(b)
            print(f"  ✗ {path}: reverted ({len(spans)} not truly dead)")
    print(f"done: {removed}/{len(dead)} pruned, build verified green")


if __name__ == "__main__":
    a = sys.argv[1:]
    if a and a[0] == "--prune" and len(a) > 1:
        cmd_prune(a[1])
    elif a and a[0] == "--list":
        cmd_list(a[1] if len(a) > 1 else None)
    else:
        print(__doc__)
