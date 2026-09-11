"""Real Evolve DOM/editor handlers and reporter; fixture APIs never apply changes.

Loads the unchanged production index, app.js and reporter from a local static
server. Monaco's loader is deliberately unavailable to exercise the supported
native textarea fallback, including the browser's real undo input event.
"""
import functools
import http.server
import json
from pathlib import Path
import threading
import time
import unittest

try:
    from playwright.sync_api import sync_playwright
except ImportError:
    sync_playwright = None


ROOT = Path(__file__).resolve().parents[2]
WEB = ROOT / "src/evolve/web"
PRIVATE_SOURCE = "fn main() { println!(\"private-source-sentinel\"); }\n"
DIGEST = "a" * 64


class QuietHandler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *_args):
        pass


@unittest.skipUnless(sync_playwright, "Playwright is required for IDE integration")
class PhiFrictionIdeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        handler = functools.partial(QuietHandler, directory=str(WEB))
        cls.server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.playwright = sync_playwright().start()
        try:
            cls.browser = cls.playwright.chromium.launch(channel="chrome", headless=True)
        except Exception:
            cls.browser = cls.playwright.chromium.launch(headless=True)
        cls.url = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)

    def setUp(self):
        self.context = self.browser.new_context(viewport={"width": 1500, "height": 1100})
        self.page = self.context.new_page()
        self.posts = []
        self.writes = []
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.route("**/api/**", self.api)
        self.page.route("**/vendor/monaco/vs/loader.js", lambda route: route.abort())
        self.page.goto(self.url + "/#inspector=node", wait_until="networkidle")
        self.wait_js("typeof phiFriction !== 'undefined' && phiFriction && state.sessionToken && state.editorMode === 'fallback' && state.activePath")
        self.page.evaluate("selectInspector('node')")

    def tearDown(self):
        self.context.close()

    def api(self, route):
        request = route.request
        path = request.url.split("/api/", 1)[1].split("?", 1)[0]
        if request.method != "GET":
            if path == "friction/events":
                body = request.post_data_json
                self.posts.append({"body": body, "headers": request.headers})
                payload = {"accepted": len(body["events"]), "cursor": len(self.posts)}
                route.fulfill(json=payload)
                return
            self.writes.append({"path": path, "body": request.post_data})
            route.fulfill(status=409, json={"error": {"code": "fixture_no_mutations", "message": "Fixture does not apply changes"}})
            return
        fixtures = {
            "workspace": {"name": "Friction fixture", "root": "/fixture", "session_token": "fixture-session"},
            "context": {"mode": "lite", "files": 1, "tokens": 12},
            "context/sizes": {},
            "ide/files": {"files": [{"path": "src/main.rs", "language": "rust"}]},
            "ide/document": {"path": "src/main.rs", "content": PRIVATE_SOURCE, "hash": DIGEST},
            "ide/ast": {"hash": DIGEST, "items": []},
            "ide/summary": {"hash": DIGEST, "summary": "Fixture source"},
            "git/status": {"branch": "fixture", "head": "b" * 40, "dirty": False, "changes": []},
            "actions/apply/status": self.staged(),
        }
        route.fulfill(json=fixtures.get(path, {}))

    def wait_js(self, expression, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            if self.page.evaluate("() => Boolean(" + expression + ")"):
                return
            self.page.wait_for_timeout(25)
        self.fail("Timed out: " + expression)

    @staticmethod
    def staged():
        return {"id": "run-fixture", "status": "staged", "diff": {
            "files_changed": 2, "insertions": 400, "deletions": 5,
            "digest": DIGEST,
            "preview": "--- a/private-source.rs\n+++ b/private-source.rs\n+private-diff-sentinel\n",
        }}

    def render_staged(self):
        self.page.evaluate("run => renderApplyRun(run, applyRunStatus(run))", self.staged())
        self.page.locator("#node-result").get_by_text("Diff preview", exact=False).wait_for()

    def events(self):
        return [event for post in self.posts for event in post["body"]["events"]]

    def flush(self):
        self.page.evaluate("async () => { while (phiFriction.busy) await new Promise(r => setTimeout(r, 10)); await phiFriction.flush(); }")

    def assert_no_mutations_or_raw_payloads(self):
        self.assertEqual(self.writes, [], "A review test must never request apply/write/model work")
        serialized = json.dumps(self.posts)
        for secret in ["private-source-sentinel", "private-diff-sentinel", "private-source.rs", "src/main.rs"]:
            self.assertNotIn(secret, serialized)
        self.assertTrue(all(post["headers"].get("x-selfware-session") == "fixture-session" for post in self.posts))
        self.assertEqual(self.errors, [])

    def test_staged_preview_measures_engagement_and_explicit_reject_keeps_worktree(self):
        self.render_staged()
        self.page.wait_for_timeout(1050)
        self.assertEqual(self.page.evaluate("phiFriction.review.activeMs"), 0)
        started = time.monotonic()
        self.page.locator("#node-result details summary").click()
        self.page.wait_for_timeout(1100)
        self.page.locator("#node-result").get_by_role("button", name="Reject diff", exact=True).click()
        self.flush()
        reviews = [event for event in self.events() if event["kind"] == "review_closed"]
        self.assertEqual(len(reviews), 1)
        event = reviews[0]
        self.assertEqual(event["generation_id"], "run-fixture")
        self.assertEqual(event["data"]["decision"], "rejected")
        self.assertEqual(event["data"]["added_lines"], 400)
        self.assertEqual(event["data"]["diff_digest"], DIGEST)
        self.assertGreater(event["data"]["active_review_ms"], 500)
        self.assertLess(event["data"]["active_review_ms"], (time.monotonic() - started + 2) * 1000)
        self.assertIn("staged worktree is retained", self.page.locator("#node-result").inner_text())
        self.assertTrue(self.page.locator("#node-result").get_by_role("button", name="Apply", exact=True).is_enabled())
        self.assertEqual(self.page.evaluate("state.activeStagedRun.id"), "run-fixture")
        self.assert_no_mutations_or_raw_payloads()

    def test_validator_refusal_does_not_report_developer_rejection(self):
        run = {"id": "validator-refused", "status": {"rejected": "compile_failed: E0308"}}
        self.page.evaluate("run => renderApplyRun(run, applyRunStatus(run))", run)
        self.flush()
        self.assertIn("Apply run rejected: compile_failed: E0308", self.page.locator("#node-result").inner_text())
        self.assertFalse(any(event["kind"] == "review_closed" and event["data"]["decision"] == "rejected" for event in self.events()))
        self.assertEqual(self.page.locator("#node-result").get_by_role("button", name="Reject diff", exact=True).count(), 0)
        self.assert_no_mutations_or_raw_payloads()

    def test_native_editor_undo_is_semantic_and_has_no_invented_generation(self):
        self.render_staged()
        editor = self.page.locator("#editor-fallback")
        editor.click()
        before = editor.input_value()
        editor.press("ControlOrMeta+End")
        self.page.keyboard.insert_text(" // undo-private-sentinel")
        self.assertIn("undo-private-sentinel", editor.input_value())
        editor.press("ControlOrMeta+z")
        self.assertEqual(editor.input_value(), before)
        self.flush()
        undo = [event for event in self.events() if event["kind"] == "editor_undo"]
        self.assertEqual(len(undo), 1)
        self.assertEqual(undo[0]["data"], {"count": 1})
        self.assertNotIn("generation_id", undo[0])
        self.assertNotIn("task_id", undo[0])
        self.assertNotIn("undo-private-sentinel", json.dumps(self.posts))
        self.assert_no_mutations_or_raw_payloads()

    def test_preferences_disable_reporting_for_actual_review_and_undo_handlers(self):
        self.page.evaluate("""() => {
          localStorage.setItem('selfware.phi.companion.preferences.v1', JSON.stringify({enabled:false,lateNightEnabled:false}));
          window.dispatchEvent(new Event('phi-friction-preferences'));
        }""")
        self.assertFalse(self.page.evaluate("phiFriction.enabled"))
        self.render_staged()
        self.page.locator("#node-result details summary").click()
        self.page.locator("#node-result").get_by_role("button", name="Reject diff", exact=True).click()
        editor = self.page.locator("#editor-fallback")
        editor.click()
        self.page.keyboard.type("x")
        editor.press("ControlOrMeta+z")
        self.flush()
        self.assertEqual(self.posts, [])
        self.assertEqual(self.page.evaluate("phiFriction.queue.length"), 0)
        self.assert_no_mutations_or_raw_payloads()


if __name__ == "__main__":
    unittest.main()
