#!/usr/bin/env python3
"""Option A: relocate `src/**/*_test.rs` bodies to `tests/unit/**` while keeping
each as a private `#[cfg(test)] #[path=...] mod tests;` child of its source
module. Only the `#[path]` string in the source is retargeted; test contents,
visibility, and `use super::*` imports are untouched (private access preserved).

Usage:  python3 scripts/migrate_tests_option_a.py [--dry-run]
"""
import os, re, sys

DRY = "--dry-run" in sys.argv
BLOCK = re.compile(
    r'#\[cfg\((?:.+?)\)\]\s*\n'
    r'(?:[ \t]*#\[[^\]]*\][^\n]*\n)*'
    r'[ \t]*#\[path\s*=\s*"([^"]*_test\.rs)"\]\s*\n'
    r'[ \t]*mod\s+\w+\s*;',
)

def module_path(src_file):
    rel = src_file[len("src/"):] if src_file.startswith("src/") else src_file
    rel = rel[:-3] if rel.endswith(".rs") else rel
    if rel in ("lib", "main"):
        return ""
    parts = [p for p in rel.split("/") if p and p != "mod"]
    return "::".join(parts)

def main():
    moves = []   # (test_abs, dest, src_file, old_path_str, new_path_str)
    for root, _, files in os.walk("src"):
        for f in files:
            if not f.endswith(".rs") or f.endswith("_test.rs"):
                continue
            s = os.path.join(root, f)
            content = open(s, encoding="utf-8").read()
            for m in BLOCK.finditer(content):
                t = m.group(1)
                test_abs = os.path.normpath(os.path.join(root, t))
                p = module_path(s)
                pdir = p.replace("::", "/") if p else "_crate"
                dest = os.path.join("tests/unit", pdir, os.path.basename(t))
                new_rel = os.path.relpath(dest, root)
                moves.append((test_abs, dest, s, t, new_rel))

    print(f"tests to relocate: {len(moves)}")
    if DRY:
        for ta, d, s, t, nr in moves[:10]:
            print(f"  {ta} -> {d}\n      in {s}: #[path=\"{t}\"] -> #[path=\"{nr}\"]")
        # orphan check
        moved = {os.path.normpath(m[0]) for m in moves}
        orph = [os.path.join(r, f) for r, _, fs in os.walk("src") for f in fs
                if f.endswith("_test.rs") and os.path.normpath(os.path.join(r, f)) not in moved]
        print(f"orphans: {len(orph)}")
        for o in orph[:10]:
            print("   ", o)
        return

    # group path edits per source file (multiple blocks per file)
    edits = {}
    for test_abs, dest, s, old_t, new_rel in moves:
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        os.replace(test_abs, dest)
        edits.setdefault(s, []).append((old_t, new_rel))
    for s, subs in edits.items():
        c = open(s, encoding="utf-8").read()
        for old_t, new_rel in subs:
            c = c.replace(f'#[path = "{old_t}"]', f'#[path = "{new_rel}"]')
        open(s, "w", encoding="utf-8").write(c)
    print(f"relocated {len(moves)} tests, retargeted #[path] in {len(edits)} sources")

if __name__ == "__main__":
    main()
