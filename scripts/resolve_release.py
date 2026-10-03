#!/usr/bin/env python3
"""Resolve a release tag to a commit, failing closed on lookup errors."""

import json
import os
import re
import subprocess
import sys
import urllib.error
import urllib.parse
import urllib.request


def resolve_release(repository, tag, fallback, allow_missing, get):
    subprocess.run(["git", "check-ref-format", f"refs/tags/{tag}"], check=True,
                   capture_output=True)
    if not re.fullmatch(r"[0-9a-f]{40}", fallback):
        raise ValueError("dispatch revision must be a full commit SHA")
    base = f"repos/{repository}/git"
    try:
        obj = get(f"{base}/ref/tags/{urllib.parse.quote(tag, safe='')}")["object"]
    except urllib.error.HTTPError as error:
        if error.code != 404 or not allow_missing:
            raise
        return fallback
    seen = set()
    while obj["type"] == "tag":
        sha = obj["sha"]
        if sha in seen or len(seen) >= 16 or not re.fullmatch(r"[0-9a-f]{40}", sha):
            raise ValueError("invalid or cyclic annotated release tag")
        seen.add(sha)
        obj = get(f"{base}/tags/{sha}")["object"]
    if obj["type"] != "commit" or not re.fullmatch(r"[0-9a-f]{40}", obj["sha"]):
        raise ValueError("release tag does not resolve to a commit")
    return obj["sha"]


def cargo_package_version(manifest_path):
    """Read the root `[package].version` without requiring third-party TOML."""
    in_package = False
    with open(manifest_path, encoding="utf-8") as manifest:
        for line in manifest:
            stripped = line.strip()
            if stripped.startswith("["):
                if stripped == "[package]":
                    in_package = True
                    continue
                if in_package:
                    break
            if in_package:
                match = re.fullmatch(r'version\s*=\s*"([^"]+)"\s*(?:#.*)?', stripped)
                if match:
                    return match.group(1)
    raise ValueError(f"{manifest_path} has no [package].version")


def validate_release_tag(tag, manifest_path):
    """Require a release tag to name the exact package version it builds."""
    expected = f"v{cargo_package_version(manifest_path)}"
    if tag != expected:
        raise ValueError(f"release tag {tag!r} does not match package version {expected!r}")
    return expected


def main():
    if len(sys.argv) > 1:
        if len(sys.argv) != 4 or sys.argv[1] != "--validate-version":
            raise SystemExit("usage: resolve_release.py --validate-version TAG Cargo.toml")
        tag = validate_release_tag(sys.argv[2], sys.argv[3])
        print(f"Release tag {tag} matches the package version")
        return

    api = os.environ.get("GITHUB_API_URL", "https://api.github.com").rstrip("/")

    def get(path):
        request = urllib.request.Request(f"{api}/{path}", headers={
            "Authorization": f"Bearer {os.environ['GH_TOKEN']}",
            "Accept": "application/vnd.github+json",
        })
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)

    tag = os.environ["RELEASE_TAG"]
    sha = resolve_release(os.environ["GITHUB_REPOSITORY"], tag,
                          os.environ["GITHUB_SHA"],
                          os.environ["GITHUB_EVENT_NAME"] == "workflow_dispatch", get)
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"tag={tag}\nsha={sha}\n")
    print(f"Release {tag} resolves to commit {sha}")


if __name__ == "__main__":
    main()
