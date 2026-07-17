#!/usr/bin/env python3
"""Fuzzy patch application — tries multiple strategies to apply a diff.

Usage: python3 scripts/fuzzy_apply.py <repo_dir> <patch_file>

Strategies (in order):
1. git apply (strict)
2. git apply --3way
3. patch -p1 --fuzz=3
4. Python fuzzy matcher (finds closest matching context in file)

Returns exit code 0 if patch applied, 1 if failed.
Prints applied strategy to stdout.
"""

import difflib
import os
import re
import subprocess
import sys
from pathlib import Path


def try_git_apply(repo_dir, patch_file):
    r = subprocess.run(
        ["git", "apply", patch_file],
        cwd=repo_dir, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    if r.returncode == 0:
        return True, "git_apply"
    return False, r.stdout


def try_git_apply_3way(repo_dir, patch_file):
    r = subprocess.run(
        ["git", "apply", "--3way", patch_file],
        cwd=repo_dir, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    if r.returncode == 0:
        return True, "git_apply_3way"
    return False, r.stdout


def try_patch_fuzz(repo_dir, patch_file):
    r = subprocess.run(
        ["patch", "-p1", "--fuzz=3", "-i", patch_file],
        cwd=repo_dir, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True
    )
    if r.returncode == 0:
        return True, "patch_fuzz3"
    # Clean up .rej files
    for f in Path(repo_dir).rglob("*.rej"):
        f.unlink()
    for f in Path(repo_dir).rglob("*.orig"):
        f.unlink()
    return False, r.stdout


def parse_hunks(patch_text):
    """Parse a unified diff into file-level hunks."""
    files = []
    current_file = None
    current_hunks = []

    for line in patch_text.split('\n'):
        if line.startswith('diff --git') or line.startswith('--- a/'):
            if line.startswith('--- a/'):
                path = line[6:]
                if current_file and current_hunks:
                    files.append({"path": current_file, "hunks": current_hunks})
                current_file = path
                current_hunks = []
        elif line.startswith('+++ b/'):
            current_file = line[6:]
        elif line.startswith('@@'):
            # Parse @@ -start,count +start,count @@
            m = re.match(r'@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@', line)
            if m:
                current_hunks.append({
                    "old_start": int(m.group(1)),
                    "old_count": int(m.group(2) or 1),
                    "new_start": int(m.group(3)),
                    "new_count": int(m.group(4) or 1),
                    "lines": [],
                })
        elif current_hunks:
            current_hunks[-1]["lines"].append(line)

    if current_file and current_hunks:
        files.append({"path": current_file, "hunks": current_hunks})

    return files


def try_fuzzy_apply(repo_dir, patch_file):
    """Apply patch using fuzzy context matching."""
    with open(patch_file) as f:
        patch_text = f.read()

    files = parse_hunks(patch_text)
    if not files:
        return False, "No hunks found in patch"

    all_applied = True
    details = []

    for file_info in files:
        filepath = os.path.join(repo_dir, file_info["path"])
        if not os.path.exists(filepath):
            details.append(f"File not found: {file_info['path']}")
            all_applied = False
            continue

        with open(filepath) as f:
            original_lines = f.readlines()

        modified_lines = list(original_lines)
        offset = 0  # Track cumulative line offset from previous hunks

        for hunk in file_info["hunks"]:
            # Extract context and changes from hunk
            context_lines = []  # Lines starting with ' '
            remove_lines = []   # Lines starting with '-'
            add_lines = []      # Lines starting with '+'

            for line in hunk["lines"]:
                if line.startswith(' '):
                    context_lines.append(line[1:])
                elif line.startswith('-'):
                    remove_lines.append(line[1:])
                elif line.startswith('+'):
                    add_lines.append(line[1:])

            # Build the "old" block (context + removed lines in order)
            old_block = []
            for line in hunk["lines"]:
                if line.startswith(' ') or line.startswith('-'):
                    old_block.append(line[1:])

            if not old_block:
                continue

            # Find the best matching location in the file
            target_start = hunk["old_start"] - 1 + offset
            best_pos = find_best_match(modified_lines, old_block, target_start)

            if best_pos is None:
                details.append(f"Could not find matching context in {file_info['path']} for hunk at line {hunk['old_start']}")
                all_applied = False
                continue

            # Apply the hunk: replace old_block with new_block
            new_block = []
            for line in hunk["lines"]:
                if line.startswith(' ') or line.startswith('+'):
                    new_block.append(line[1:])

            # Replace lines
            old_len = len(old_block)
            new_len = len(new_block)

            # Ensure lines end with newline
            new_block_lines = []
            for l in new_block:
                if not l.endswith('\n'):
                    l += '\n'
                new_block_lines.append(l)

            modified_lines[best_pos:best_pos + old_len] = new_block_lines
            offset += new_len - old_len

            details.append(f"Applied hunk at line {best_pos+1} in {file_info['path']} (target was {hunk['old_start']})")

        # Write back
        with open(filepath, 'w') as f:
            f.writelines(modified_lines)

    if all_applied:
        return True, "fuzzy_apply: " + "; ".join(details)
    else:
        # Revert changes on failure
        subprocess.run(["git", "checkout", "--", "."], cwd=repo_dir,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        return False, "fuzzy_apply failed: " + "; ".join(details)


def find_best_match(file_lines, old_block, hint_pos):
    """Find the position in file_lines that best matches old_block."""
    if not old_block:
        return None

    # Normalize for comparison
    def normalize(lines):
        return [l.rstrip() for l in lines]

    norm_block = normalize(old_block)
    norm_file = normalize([l.rstrip('\n') for l in file_lines])

    # Strategy 1: Exact match at hinted position
    for start in range(max(0, hint_pos - 5), min(len(norm_file), hint_pos + 6)):
        end = start + len(norm_block)
        if end <= len(norm_file):
            if norm_file[start:end] == norm_block:
                return start

    # Strategy 2: Exact match anywhere
    for start in range(len(norm_file) - len(norm_block) + 1):
        if norm_file[start:start + len(norm_block)] == norm_block:
            return start

    # Strategy 3: Fuzzy match — find best similarity score
    best_score = 0.0
    best_pos = None
    block_str = '\n'.join(norm_block)

    # Search in a window around the hint
    search_start = max(0, hint_pos - 100)
    search_end = min(len(norm_file) - len(norm_block) + 1, hint_pos + 100)

    for start in range(search_start, max(search_end, search_start + 1)):
        end = start + len(norm_block)
        if end > len(norm_file):
            break
        candidate = '\n'.join(norm_file[start:end])
        ratio = difflib.SequenceMatcher(None, block_str, candidate).ratio()
        if ratio > best_score:
            best_score = ratio
            best_pos = start

    # Accept if similarity > 80%
    if best_score > 0.8:
        return best_pos

    return None


def main():
    if len(sys.argv) != 3:
        print("Usage: fuzzy_apply.py <repo_dir> <patch_file>", file=sys.stderr)
        sys.exit(1)

    repo_dir = sys.argv[1]
    patch_file = sys.argv[2]

    # Try strategies in order
    strategies = [
        ("git_apply", try_git_apply),
        ("git_apply_3way", try_git_apply_3way),
        ("patch_fuzz3", try_patch_fuzz),
        ("fuzzy_apply", try_fuzzy_apply),
    ]

    for name, strategy in strategies:
        success, detail = strategy(repo_dir, patch_file)
        if success:
            print(f"APPLIED:{name}")
            sys.exit(0)
        else:
            print(f"TRIED:{name}:{detail[:200]}", file=sys.stderr)

    print("FAILED:all_strategies_exhausted")
    sys.exit(1)


if __name__ == "__main__":
    main()
