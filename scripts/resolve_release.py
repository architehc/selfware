#!/usr/bin/env python3
"""Resolve a release tag to a commit, failing closed on lookup errors."""

import json
import os
import re
import subprocess
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


def main():
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
