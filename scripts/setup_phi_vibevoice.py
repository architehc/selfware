#!/usr/bin/env python3
"""Install Phi's isolated ONNX runtime and pinned VibeVoice model cache."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tempfile

REPO_ID = "elbruno/VibeVoice-Realtime-0.5B-ONNX"
REVISION = "6825ea6fd389843b39a33d5a088c5993bf4fab4e"
GRAPHS = (
    "lm_with_kv", "tts_lm_prefill", "tts_lm_step", "prediction_head",
    "acoustic_connector", "eos_classifier", "acoustic_decoder",
)
PATTERNS = [name + suffix for name in GRAPHS for suffix in (".onnx", ".onnx.data")]
ROOT_FILES = PATTERNS + ["tokenizer.json", "type_embeddings.npy", "model_config.json", "config.json", "LICENSE"]
PATTERNS = ROOT_FILES + ["voices/**"]
VOICE_FOLDERS = ("en-Carter_man", "en-Davis_man", "en-Emma_woman", "en-Frank_man", "en-Grace_woman", "en-Mike_man")
DEFAULT_CACHE = Path.home() / ".cache" / "selfware" / "vibevoice"


def required_files():
    required = set(ROOT_FILES)
    for folder in VOICE_FOLDERS:
        base = f"voices/{folder}/"
        required.update({base + "metadata.json", base + "negative/tts_lm_hidden.npy"})
        for prefix, layers, directory in (("lm", 4, ""), ("tts", 20, ""), ("tts", 20, "negative/")):
            required.update(f"{base}{directory}{prefix}_kv_{kind}_{layer}.npy"
                            for kind in ("key", "value") for layer in range(layers))
    return required


def verify_snapshot(model_dir: Path, info):
    """Check every selected pinned file, including regular Git blobs and voices."""
    if info.sha != REVISION:
        raise RuntimeError("Model metadata did not resolve to the pinned revision")
    siblings = {}
    for item in info.siblings:
        name = item.rfilename
        if name not in ROOT_FILES and not name.startswith("voices/"):
            continue
        path = PurePosixPath(name)
        if path.is_absolute() or ".." in path.parts or "\\" in name or "\0" in name:
            raise RuntimeError("Pinned model metadata contains an invalid asset path")
        if name in siblings:
            raise RuntimeError(f"Pinned model metadata repeats an asset: {name}")
        siblings[name] = item
    missing_metadata = sorted(required_files() - siblings.keys())
    if missing_metadata:
        raise RuntimeError("Pinned metadata is missing required assets: " + ", ".join(missing_metadata))
    checked = {}
    files = []
    for name, item in sorted(siblings.items()):
        path = model_dir / name
        if not path.is_file():
            raise RuntimeError(f"Missing required model component: {name}")
        stat = path.stat()
        if item.size is None or stat.st_size != item.size:
            raise RuntimeError(f"Downloaded size does not match the pinned model: {name}")
        expected = item.lfs.sha256 if item.lfs else item.blob_id
        expected_length = 64 if item.lfs else 40
        if not isinstance(expected, str) or len(expected) != expected_length or any(c not in "0123456789abcdef" for c in expected):
            raise RuntimeError(f"Pinned model metadata has no valid content hash: {name}")
        identity = (str(path.resolve()), stat.st_size, stat.st_mtime_ns)
        if identity not in checked:
            digest = hashlib.sha256()
            git_blob = hashlib.sha1(f"blob {stat.st_size}\0".encode())
            with path.open("rb") as handle:
                for block in iter(lambda: handle.read(4 * 1024 * 1024), b""):
                    digest.update(block)
                    git_blob.update(block)
            checked[identity] = (digest.hexdigest(), git_blob.hexdigest())
        sha256, blob_id = checked[identity]
        if (sha256 if item.lfs else blob_id) != expected:
            kind = "SHA-256" if item.lfs else "Git blob SHA-1"
            raise RuntimeError(f"Downloaded {kind} does not match the pinned model: {name}")
        files.append({"path": name, "bytes": stat.st_size, "sha256": sha256})
    return files


def write_installation(cache: Path, model_dir: Path, files):
    manifest = {"repo_id": REPO_ID, "revision": REVISION, "model_dir": str(model_dir),
                "runtime_python": sys.executable, "files": files}
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(mode="w", encoding="utf-8", prefix=".installation-", suffix=".json", dir=cache, delete=False) as handle:
            temporary = Path(handle.name)
            handle.write(json.dumps(manifest, indent=2) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(temporary, cache / "installation.json")
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def download(cache: Path):
    from huggingface_hub import HfApi, snapshot_download

    print(f"Downloading the pinned {REPO_ID} runtime files into {cache}.", flush=True)
    info = HfApi().model_info(REPO_ID, revision=REVISION, files_metadata=True)
    if info.sha != REVISION:
        raise RuntimeError("Model metadata did not resolve to the pinned revision")
    model_dir = Path(snapshot_download(REPO_ID, revision=REVISION, cache_dir=str(cache / "hub"),
                                       allow_patterns=PATTERNS, max_workers=8))
    files = verify_snapshot(model_dir, info)
    write_installation(cache, model_dir, files)
    print(json.dumps({"status": "downloaded_and_hash_checked", "model_dir": str(model_dir),
                      "files": len(files), "logical_bytes": sum(f["bytes"] for f in files)}, indent=2), flush=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cache-dir", type=Path, default=DEFAULT_CACHE)
    parser.add_argument("--python", help="Python 3.12+ executable for the isolated runtime")
    parser.add_argument("--download-only", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args(argv)
    cache = args.cache_dir.expanduser().resolve()
    cache.mkdir(parents=True, exist_ok=True)
    if args.download_only:
        download(cache)
        return 0
    python = args.python or (sys.executable if sys.version_info >= (3, 12) else shutil.which("python3.12"))
    if not python:
        parser.error("Python 3.12+ is required. Pass --python /path/to/python3.12.")
    version = subprocess.run([python, "-c", "import sys; print(int(sys.version_info >= (3,12)))"],
                             check=True, capture_output=True, text=True)
    if version.stdout.strip() != "1":
        parser.error("The chosen interpreter must be Python 3.12 or newer.")
    runtime = cache / "runtime-onnx-v1"
    executable = runtime / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    if not executable.exists():
        subprocess.run([python, "-m", "venv", str(runtime)], check=True)
    requirements = Path(__file__).with_name("phi_vibevoice_requirements.txt")
    uv = shutil.which("uv")
    command = ([uv, "pip", "install", "--python", str(executable)] if uv else
               [str(executable), "-m", "pip", "install"])
    subprocess.run(command + ["-r", str(requirements)], check=True)
    subprocess.run([str(executable), str(Path(__file__).resolve()), "--download-only",
                    "--cache-dir", str(cache)], check=True)
    print(f"Runtime installed: {executable}", flush=True)
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f"VibeVoice setup failed: {error}", file=sys.stderr)
        raise SystemExit(1)
