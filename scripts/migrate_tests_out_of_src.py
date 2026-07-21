#!/usr/bin/env python3
"""Relocate every `src/**/*_test.rs` unit test to `tests/unit/**` as an
integration test of the public `selfware` crate.

Source of truth: each `#[cfg(test)] #[path = "X_test.rs"] mod NAME;` block in a
source file S. S's module path P determines the crate-relative rewrite of the
test's `super::` / `crate::` imports. The test file moves under tests/unit/<P>/,
the block is removed from S, and the moved file is wired into tests/unit/mod.rs.

Private items the tests reach are NOT pub-ified here — that is done afterwards,
driven by the compiler's `is private` errors (unconditional `pub`, per decision).

Usage:  python3 scripts/migrate_tests_out_of_src.py [--dry-run]
"""
import os, re, sys

DRY = "--dry-run" in sys.argv
UNIT = "tests/unit"
BLOCK = re.compile(
    r'[ \t]*#\[cfg\((.+?)\)\]\s*\n'            # 1: cfg inner (test | all(test, feature="x"))
    r'(?:[ \t]*#\[[^\]]*\][^\n]*\n)*'          # optional intervening attributes (e.g. #[allow(...)])
    r'[ \t]*#\[path\s*=\s*"([^"]*_test\.rs)"\]\s*\n'   # 2: test file
    r'[ \t]*mod\s+\w+\s*;\s*\n',
)

def residual_gate(cfg_inner):
    """Given a cfg predicate that includes `test`, return the non-test part to
    re-apply in the integration test (where `test` is always on), or None."""
    c = cfg_inner.strip()
    if c == "test":
        return None
    if c.startswith("all(") and c.endswith(")"):
        parts = [p.strip() for p in split_top(c[4:-1])]
        rest = [p for p in parts if p != "test"]
        if not rest:
            return None
        return rest[0] if len(rest) == 1 else "all(" + ", ".join(rest) + ")"
    return c if "test" not in c else None

def split_top(s):
    out, depth, cur = [], 0, ""
    for ch in s:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur); cur = ""
        else:
            cur += ch
    if cur.strip():
        out.append(cur)
    return out

def module_path(src_file):
    """src/safety/confirm.rs -> safety::confirm ; src/agent/mod.rs -> agent ;
    src/tokens.rs -> tokens ; src/lib.rs and src/main.rs -> "" (crate root)."""
    rel = src_file[len("src/"):] if src_file.startswith("src/") else src_file
    rel = rel[:-3] if rel.endswith(".rs") else rel
    if rel in ("lib", "main"):
        return ""
    parts = [p for p in rel.split("/") if p and p != "mod"]
    return "::".join(parts)

def rewrite_imports(text, p):
    segs = p.split("::") if p else []
    def super_target(k):  # k = number of super:: in the run
        keep = segs[: len(segs) - (k - 1)] if len(segs) - (k - 1) > 0 else []
        return "::".join(["selfware"] + keep)
    # Replace runs of `super::` (longest runs first) then crate::
    def repl(m):
        k = m.group(0).count("super::")
        return super_target(k) + "::"
    text = re.sub(r'(?:super::)+', repl, text)
    text = re.sub(r'\bcrate::', 'selfware::', text)
    return text

def unique_mod_name(rel_under_unit):
    stem = rel_under_unit[:-3] if rel_under_unit.endswith(".rs") else rel_under_unit
    return re.sub(r'[^A-Za-z0-9_]', '_', stem)

def main():
    moves = []       # (test_abs_src, dest_rel_under_unit, P)
    wirings = []     # (rel_under_unit, mod_name)
    src_edits = {}   # src_file -> new_content

    for root, _, files in os.walk("src"):
        for f in files:
            if not f.endswith(".rs") or f.endswith("_test.rs"):
                continue
            s = os.path.join(root, f)
            content = open(s, encoding="utf-8").read()
            blocks = list(BLOCK.finditer(content))
            if not blocks:
                continue
            p = module_path(s)
            for b in blocks:
                test_rel = b.group(2)                       # e.g. confirm_test.rs
                gate = residual_gate(b.group(1))
                test_abs = os.path.normpath(os.path.join(root, test_rel))
                pdir = p.replace("::", "/") if p else "_crate"
                dest_rel = f"{pdir}/{os.path.basename(test_rel)}"   # under tests/unit/
                moves.append((test_abs, dest_rel, p))
                wirings.append((dest_rel, unique_mod_name(dest_rel), gate))
            src_edits[s] = BLOCK.sub("", content)

    # Orphan check: any src _test.rs not accounted for by a #[path] block.
    moved_abs = {os.path.normpath(m[0]) for m in moves}
    orphans = []
    for root, _, files in os.walk("src"):
        for f in files:
            if f.endswith("_test.rs"):
                ap = os.path.normpath(os.path.join(root, f))
                if ap not in moved_abs:
                    orphans.append(ap)
    print(f"source files touched: {len(src_edits)}")
    print(f"test files to move:   {len(moves)}")
    if orphans:
        print(f"!! ORPHAN _test.rs (no #[path] block found) — {len(orphans)}:")
        for o in orphans:
            print(f"     {o}")
    if DRY:
        for ta, dr, p in moves[:12]:
            print(f"  {ta}  ->  {UNIT}/{dr}   (P={p})")
        print("  …")
        return

    # 1) move + rewrite imports
    for test_abs, dest_rel, p in moves:
        dest = os.path.join(UNIT, dest_rel)
        os.makedirs(os.path.dirname(dest), exist_ok=True)
        text = open(test_abs, encoding="utf-8").read()
        open(dest, "w", encoding="utf-8").write(rewrite_imports(text, p))
        os.remove(test_abs)
    # 2) strip the test-mod blocks from sources
    for s, new in src_edits.items():
        open(s, "w", encoding="utf-8").write(new)
    # 3) wire into tests/unit/mod.rs
    modrs = os.path.join(UNIT, "mod.rs")
    lines = open(modrs, encoding="utf-8").read().rstrip() + "\n\n// Relocated unit tests (mirror the src module tree).\n"
    for rel, name, gate in sorted(set(wirings)):
        if gate:
            lines += f'#[cfg({gate})]\n'
        lines += f'#[path = "{rel}"]\nmod {name};\n'
    open(modrs, "w", encoding="utf-8").write(lines)
    print(f"moved {len(moves)} tests, edited {len(src_edits)} sources, wired {len(set(wirings))} modules")

if __name__ == "__main__":
    main()
