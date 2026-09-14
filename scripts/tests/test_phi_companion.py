#!/usr/bin/env python3
"""Unit tests for phi_companion.py."""

import io
import json
import pathlib
import sys
import unittest
from unittest.mock import MagicMock, patch

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))
import phi_companion as pc


class TestPhiCompanion(unittest.TestCase):
    def test_scenarios_structure(self):
        for key in ["phantom_api", "circular_spin", "boilerplate_vomit", "late_night_fatigue"]:
            self.assertIn(key, pc.SCENARIOS)
            self.assertIn("title", pc.SCENARIOS[key])
            self.assertIn("pose", pc.SCENARIOS[key])
            self.assertIn("context", pc.SCENARIOS[key])
            self.assertIn("user_prompt", pc.SCENARIOS[key])
            self.assertIn(pc.SCENARIOS[key]["pose"], pc.PHI_ASCII_POSES)

    @patch("urllib.request.urlopen")
    def test_list_models(self, mock_urlopen):
        mock_resp = MagicMock()
        mock_resp.read.return_value = json.dumps({
            "data": [{"id": "qwen38-flash-next", "max_model_len": 1000000}]
        }).encode("utf-8")
        mock_resp.__enter__.return_value = mock_resp
        mock_urlopen.return_value = mock_resp

        models = pc.list_models("https://mock.endpoint/v1")
        self.assertEqual(len(models), 1)
        self.assertEqual(models[0]["id"], "qwen38-flash-next")
        self.assertEqual(models[0]["max_model_len"], 1000000)

    @patch("urllib.request.urlopen")
    def test_generate_companion_quip(self, mock_urlopen):
        mock_resp = MagicMock()
        mock_resp.read.return_value = json.dumps({
            "model": "qwen38-flash-next",
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "That method does not exist in std::slice.",
                    "reasoning_content": "Checking slice methods..."
                }
            }],
            "usage": {"total_tokens": 120}
        }).encode("utf-8")
        mock_resp.__enter__.return_value = mock_resp
        mock_urlopen.return_value = mock_resp

        res = pc.generate_companion_quip(
            "https://mock.endpoint/v1",
            "qwen38-flash-next",
            "phantom_api"
        )
        self.assertEqual(res["scenario"], "phantom_api")
        self.assertEqual(res["content"], "That method does not exist in std::slice.")
        self.assertEqual(res["pose"], "head_tilt")
        self.assertEqual(res["tokens"]["total_tokens"], 120)


if __name__ == "__main__":
    unittest.main()
