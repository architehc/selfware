#!/usr/bin/env python3
"""Validate expansion_recommendation/*.json against _schema.json's rules
and regenerate index.json. Run in CI and after any catalog edit.

Usage: scripts/validate_expansion.py [--write-index]

Exits non-zero on any violation: malformed JSON, wrong example count,
missing required fields, examples referencing components absent from the
recommendation index, or a stale hand-edited index.json.
"""

import json
import sys
from datetime import date
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CATALOG = ROOT / "expansion_recommendation"
INDEX = CATALOG / "index.json"
EXPECTED_EXAMPLES = 20

# Top-level + per-example required fields (the UI-consumption schema).
TOP_REQUIRED = {"component", "tier", "loop_stage", "summary", "examples"}
EXAMPLE_REQUIRED = {
    "id",
    "title",
    "intent",
    "pattern",
    "loop_stage",
    "wiring",
    "loop_objects_touched",
    "how_it_shapes_the_loop",
    "touch_interaction",
    "pitfall",
    "mini_scenario",
}

ALLOWED_TIERS = {"engine", "tooling", "foundation", "full"}


def fail(msg, errors):
    errors.append(msg)


def validate_file(path, errors):
    try:
        data = json.loads(path.read_text())
    except Exception as exc:  # noqa: BLE001
        fail(f"{path.name}: invalid JSON ({exc})", errors)
        return None

    missing = TOP_REQUIRED - data.keys()
    if missing:
        fail(f"{path.name}: missing top-level fields {sorted(missing)}", errors)
    tier = data.get("tier")
    if tier and tier not in ALLOWED_TIERS:
        fail(f"{path.name}: unknown tier '{tier}'", errors)

    examples = data.get("examples", [])
    if len(examples) != EXPECTED_EXAMPLES:
        fail(
            f"{path.name}: {len(examples)} examples, expected {EXPECTED_EXAMPLES}",
            errors,
        )

    component = data.get("component", path.stem)
    seen_ids = set()
    for i, ex in enumerate(examples):
        where = f"{path.name} examples[{i}]"
        if not isinstance(ex, dict):
            fail(f"{where}: not an object", errors)
            continue
        missing_ex = EXAMPLE_REQUIRED - ex.keys()
        if missing_ex:
            fail(f"{where}: missing fields {sorted(missing_ex)}", errors)
        ex_id = ex.get("id", "")
        if not ex_id.startswith(f"{component}-"):
            fail(f"{where}: id '{ex_id}' does not start with '{component}-'", errors)
        if ex_id in seen_ids:
            fail(f"{where}: duplicate id '{ex_id}'", errors)
        seen_ids.add(ex_id)
        for text_field in ("wiring", "how_it_shapes_the_loop", "mini_scenario"):
            value = ex.get(text_field, "")
            if isinstance(value, str) and value and not value.rstrip().endswith((".", ")", '"')):
                fail(f"{where}: '{text_field}' looks truncated mid-sentence", errors)
    return data


def main():
    errors = []
    files = sorted(p for p in CATALOG.glob("*.json") if p.name not in {"index.json", "_schema.json"})
    if not files:
        fail("no recommendation files found", errors)

    components = {}
    for path in files:
        data = validate_file(path, errors)
        if data:
            components[data.get("component", path.stem)] = data

    write_index = "--write-index" in sys.argv
    index_doc = {
        "generated": str(date.today()),
        "components": sorted(components),
        "counts": {name: len(doc["examples"]) for name, doc in sorted(components.items())},
        "total_examples": sum(len(doc["examples"]) for doc in components.values()),
    }
    if INDEX.exists():
        existing = json.loads(INDEX.read_text())
        if existing.get("components") != index_doc["components"] or existing.get("counts") != index_doc["counts"]:
            if write_index:
                INDEX.write_text(json.dumps(index_doc, indent=2) + "\n")
                print(f"index.json regenerated ({len(components)} components, {index_doc['total_examples']} examples)")
            else:
                fail("index.json is stale (run with --write-index)", errors)
    elif write_index:
        INDEX.write_text(json.dumps(index_doc, indent=2) + "\n")
        print(f"index.json created ({len(components)} components, {index_doc['total_examples']} examples)")
    else:
        fail("index.json missing (run with --write-index)", errors)

    if errors:
        for error in errors:
            print(f"ERROR: {error}", file=sys.stderr)
        return 1
    print(f"ok: {len(files)} files, {index_doc['total_examples']} examples validated")
    return 0


if __name__ == "__main__":
    sys.exit(main())
