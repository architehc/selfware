#!/usr/bin/env python3
"""
Unit tests for Selfware Mascot Assistant: "Phi the Fox" (Φ)
Verifies:
- Asset presence (HTML, JS modules, CSS, image assets)
- English Grapheme-to-Phoneme (G2P) & Viseme coverage
- 10-viseme standard set completeness
- Reading mission structure and line targeting
- HTTP server responsiveness and MIME type handling
"""

import os
import unittest
import threading
import time
import urllib.request
import http.server
import socketserver

ROOT_DIR = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
PHI_DIR = os.path.join(ROOT_DIR, "src", "evolve", "web", "phi")

EXPECTED_VISEMES = {
    'rest', 'mbp', 'etc', 'ai', 'e', 'o', 'u', 'fv', 'l_th', 'wq'
}

class PhiAssistantTests(unittest.TestCase):
    def test_all_phi_modules_and_assets_exist(self):
        required_files = [
            "index.html",
            "style.css",
            "app.js",
            "phi_rig.js",
            "phi_viseme.js",
            "phi_focus.js",
            "phi_agent.js",
            os.path.join("assets", "fox_phi_portrait.jpg"),
            os.path.join("assets", "fox_phi_banner.jpg"),
        ]
        for rel_path in required_files:
            abs_path = os.path.join(PHI_DIR, rel_path)
            self.assertTrue(os.path.isfile(abs_path), f"Missing expected Phi component: {rel_path}")

    def test_rig_declares_all_ten_visemes(self):
        rig_path = os.path.join(PHI_DIR, "phi_rig.js")
        with open(rig_path, "r", encoding="utf-8") as f:
            content = f.read()

        for viseme in EXPECTED_VISEMES:
            self.assertIn(f"'{viseme}'", content, f"Viseme {viseme} missing in phi_rig.js")

    def test_viseme_engine_contains_english_lexicon_and_heuristics(self):
        viseme_path = os.path.join(PHI_DIR, "phi_viseme.js")
        with open(viseme_path, "r", encoding="utf-8") as f:
            content = f.read()

        self.assertIn("COMMON_LEXICON", content)
        self.assertIn("PHONEME_TO_VISEME", content)
        self.assertIn("wordToVisemes", content)
        self.assertIn("sentenceToVisemes", content)

        # Check key security / selfware words in lexicon
        for word in ["selfware", "phi", "security", "container", "sandbox", "docker", "code", "fox"]:
            self.assertIn(f"'{word}'", content, f"Word {word} missing in COMMON_LEXICON")

    def test_agent_orchestrator_has_core_missions(self):
        agent_path = os.path.join(PHI_DIR, "phi_agent.js")
        with open(agent_path, "r", encoding="utf-8") as f:
            content = f.read()

        self.assertIn("container_security", content)
        self.assertIn("volume_sanitizer", content)
        self.assertIn("radix_attention", content)
        self.assertIn("generateReadingScriptForCode", content)
        self.assertIn("runMission", content)

    def test_local_server_serves_phi_workspace(self):
        web_root = os.path.join(ROOT_DIR, "src", "evolve", "web")
        test_port = 8799

        class Handler(http.server.SimpleHTTPRequestHandler):
            def __init__(self, *args, **kwargs):
                super().__init__(*args, directory=web_root, **kwargs)

        socketserver.TCPServer.allow_reuse_address = True
        httpd = socketserver.TCPServer(("", test_port), Handler)
        server_thread = threading.Thread(target=httpd.serve_forever, daemon=True)
        server_thread.start()
        time.sleep(0.3)

        try:
            url = f"http://localhost:{test_port}/phi/index.html"
            with urllib.request.urlopen(url, timeout=3) as resp:
                self.assertEqual(resp.status, 200)
                body = resp.read().decode("utf-8")
                self.assertIn("Phi Mascot Assistant", body)
                self.assertIn("app.js", body)
                self.assertIn("style.css", body)

            # Check JS module loading
            js_url = f"http://localhost:{test_port}/phi/phi_rig.js"
            with urllib.request.urlopen(js_url, timeout=3) as resp:
                self.assertEqual(resp.status, 200)
                self.assertIn("javascript", resp.headers.get("Content-Type", "").lower())
        finally:
            httpd.shutdown()

if __name__ == "__main__":
    unittest.main()
