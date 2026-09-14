"""Offline installation integrity checks using tiny synthetic pinned assets."""

import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock

SCRIPT = Path(__file__).resolve().parents[1] / "setup_phi_vibevoice.py"
spec = importlib.util.spec_from_file_location("phi_vibevoice_setup", SCRIPT)
setup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(setup)


class PhiVibeVoiceSetupTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.cache = Path(self.temporary.name)
        self.models = self.cache / "snapshot"
        self.models.mkdir()
        siblings = []
        for name in sorted(setup.required_files()):
            path = self.models / name
            path.parent.mkdir(parents=True, exist_ok=True)
            body = ("synthetic fixture for " + name).encode()
            path.write_bytes(body)
            lfs = SimpleNamespace(sha256=hashlib.sha256(body).hexdigest()) if name.endswith((".npy", ".onnx.data")) else None
            siblings.append(SimpleNamespace(rfilename=name, size=len(body), lfs=lfs,
                                            blob_id=hashlib.sha1(b"blob " + str(len(body)).encode() + b"\0" + body).hexdigest()))
        self.info = SimpleNamespace(sha=setup.REVISION, siblings=siblings)

    def test_complete_six_voice_snapshot_verifies_every_required_asset(self):
        files = setup.verify_snapshot(self.models, self.info)
        self.assertEqual({item["path"] for item in files}, setup.required_files())
        self.assertEqual(len(setup.VOICE_FOLDERS), 6)
        self.assertEqual(len(setup.GRAPHS), 7)
        for item in files:
            body = (self.models / item["path"]).read_bytes()
            self.assertEqual(item["bytes"], len(body))
            self.assertEqual(item["sha256"], hashlib.sha256(body).hexdigest())

    def test_missing_tokenizer_type_embeddings_and_voice_arrays_each_fail(self):
        for name in ["tokenizer.json", "type_embeddings.npy", "voices/en-Emma_woman/negative/tts_kv_value_19.npy", "voices/en-Mike_man/lm_kv_key_3.npy"]:
            with self.subTest(name=name):
                path = self.models / name
                body = path.read_bytes()
                path.unlink()
                try:
                    with self.assertRaisesRegex(RuntimeError, "Missing required model component"):
                        setup.verify_snapshot(self.models, self.info)
                finally:
                    path.write_bytes(body)

    def test_missing_required_metadata_and_wrong_revision_fail(self):
        self.info.sha = "0" * 40
        with self.assertRaisesRegex(RuntimeError, "pinned revision"):
            setup.verify_snapshot(self.models, self.info)
        self.info.sha = setup.REVISION
        self.info.siblings = [item for item in self.info.siblings if item.rfilename != "tokenizer.json"]
        with self.assertRaisesRegex(RuntimeError, "metadata is missing.*tokenizer"):
            setup.verify_snapshot(self.models, self.info)

    def test_same_size_corruption_fails_lfs_sha256_and_regular_git_blob_checks(self):
        for name, expected in [("lm_with_kv.onnx.data", "SHA-256"), ("tokenizer.json", "Git blob SHA-1")]:
            with self.subTest(name=name):
                path = self.models / name
                original = path.read_bytes()
                path.write_bytes(b"!" + original[1:])
                try:
                    with self.assertRaisesRegex(RuntimeError, expected):
                        setup.verify_snapshot(self.models, self.info)
                finally:
                    path.write_bytes(original)

    def test_pinned_extra_voice_assets_are_also_required_and_verified(self):
        name = "voices/en-Emma_woman/additional.npy"
        self.info.siblings.append(SimpleNamespace(rfilename=name, size=1, lfs=SimpleNamespace(sha256=hashlib.sha256(b"x").hexdigest()), blob_id=None))
        with self.assertRaisesRegex(RuntimeError, "Missing required model component"):
            setup.verify_snapshot(self.models, self.info)
        (self.models / name).write_bytes(b"x")
        self.assertIn(name, {item["path"] for item in setup.verify_snapshot(self.models, self.info)})

    def test_missing_content_hash_cannot_be_reported_as_hash_checked(self):
        item = next(item for item in self.info.siblings if item.rfilename == "tokenizer.json")
        item.blob_id = None
        with self.assertRaisesRegex(RuntimeError, "no valid content hash"):
            setup.verify_snapshot(self.models, self.info)

    def test_huggingface_blob_symlinks_are_supported_but_expected_hash_still_applies(self):
        name = "type_embeddings.npy"
        path = self.models / name
        blob = self.cache / "synthetic-blob"
        path.replace(blob)
        path.symlink_to(blob)
        self.assertIn(name, {item["path"] for item in setup.verify_snapshot(self.models, self.info)})
        blob.write_bytes(b"!" + blob.read_bytes()[1:])
        with self.assertRaisesRegex(RuntimeError, "SHA-256"):
            setup.verify_snapshot(self.models, self.info)

    def test_failed_atomic_publish_preserves_previous_receipt_and_cleans_temporary(self):
        receipt = self.cache / "installation.json"
        previous = b'{"previous":"verified installation"}\n'
        receipt.write_bytes(previous)
        files = setup.verify_snapshot(self.models, self.info)
        with mock.patch.object(setup.os, "replace", side_effect=OSError("fixture publication failure")):
            with self.assertRaisesRegex(OSError, "fixture publication failure"):
                setup.write_installation(self.cache, self.models, files)
        self.assertEqual(receipt.read_bytes(), previous)
        self.assertEqual(list(self.cache.glob(".installation-*")), [])
        setup.write_installation(self.cache, self.models, files)
        installed = json.loads(receipt.read_text())
        self.assertEqual(installed["revision"], setup.REVISION)
        self.assertEqual(installed["files"], files)

    def test_download_pins_metadata_and_snapshot_and_never_publishes_partial_install(self):
        hub = SimpleNamespace(HfApi=mock.Mock(), snapshot_download=mock.Mock(return_value=str(self.models)))
        hub.HfApi.return_value.model_info.return_value = self.info
        with mock.patch.dict(sys.modules, {"huggingface_hub": hub}), contextlib.redirect_stdout(io.StringIO()):
            setup.download(self.cache)
        hub.HfApi.return_value.model_info.assert_called_once_with(setup.REPO_ID, revision=setup.REVISION, files_metadata=True)
        self.assertEqual(hub.snapshot_download.call_args.kwargs["revision"], setup.REVISION)
        self.assertEqual(hub.snapshot_download.call_args.kwargs["allow_patterns"], setup.PATTERNS)
        previous = (self.cache / "installation.json").read_bytes()
        (self.models / "tokenizer.json").unlink()
        with mock.patch.dict(sys.modules, {"huggingface_hub": hub}), contextlib.redirect_stdout(io.StringIO()):
            with self.assertRaisesRegex(RuntimeError, "Missing required model component"):
                setup.download(self.cache)
        self.assertEqual((self.cache / "installation.json").read_bytes(), previous)


if __name__ == "__main__":
    unittest.main()
