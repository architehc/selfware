#!/usr/bin/env python3
"""Browser contracts for Phi's real workspace protocol (fixture model responses).

The browser executes production modules. These tests deliberately simulate the
HTTP/model side; they do not claim live inference or audible speech verification.
"""
from functools import partial
from hashlib import sha256
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
from urllib.parse import urlparse
import json
import unittest

try:
    from playwright.sync_api import sync_playwright
except ImportError:
    sync_playwright = None

WEB = Path(__file__).resolve().parents[2] / 'src/evolve/web'
SOURCE = 'pub fn greeting() -> String {\n    String::from("Hello from your local workspace")\n}\n'


def document(content):
    return {'path': 'src/lib.rs', 'content': content, 'hash': sha256(content.encode()).hexdigest(), 'language': 'rust', 'lines': len(content.splitlines())}


class Handler(SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def reply(self, body, status=200):
        data = json.dumps(body).encode()
        self.send_response(status)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        state = self.server.state
        path = urlparse(self.path).path
        if path == '/api/workspace':
            return self.reply({'root': '/fixture/phi', 'name': 'Phi browser fixture', 'session_token': 'fixture-session', 'model': 'fixture-model', 'endpoint_host': 'fixture.local'})
        if path == '/api/phi/policy':
            return self.reply({'mode': 'advise', 'debt_gate_lines': 250, 'bulk_generation_lines': 500, 'capitulation_threshold': 2, 'interrupt_budget': 3, 'steward_quiet_ms': 12000, 'quiet_after_activity_ms': 4000, 'glance_cooldown_ms': 45000})
        if path == '/api/phi/arena':
            return self.reply({
                'profile': {'xp': 50, 'level': 1, 'best_streak': 1.25, 'citations_inspected': 1, 'drills_completed': 1, 'achievement_badges': [{'id': 'first_laser_lock', 'title': '🏆 Optical Lock'}]},
                'drill': {'seed': 0, 'question': 'Which workspace file defines `greeting`?', 'symbol_name': 'greeting', 'citation': 'src/lib.rs:1', 'path': 'src/lib.rs', 'start_line': 1, 'end_line': 3, 'options': ['src/lib.rs:1', 'src/safety/redact.rs', 'src/evolve/ontology.rs'], 'correct_index': 0},
            })
        if path == '/api/ide/files':
            return self.reply([{'path': 'src/lib.rs', 'is_dir': False}])
        if path == '/api/ide/document':
            return self.reply(document(state['content']))
        if path == '/api/assistant/review/status':
            state['polls'] += 1
            if not state['release']:
                return self.reply({'status': 'queued'})
            snapshot = state['snapshot']
            evidence = {'id': 'E1', 'path': snapshot['path'], 'content_hash': snapshot['hash'], 'start_line': 1, 'end_line': 3,
                        'excerpt': '\n'.join(str(i + 1).rjust(6) + ' | ' + line for i, line in enumerate(snapshot['content'].splitlines()))}
            return self.reply({'status': 'done', 'result': {'review': {'model': 'fixture-model', 'trust_state': 'structural', 'evidence_complete': True,
                'claims': [{'text': '`greeting` returns an owned String for its caller.', 'evidence_ids': ['E1']}], 'recommendations': [], 'evidence': [evidence]}}})
        super().do_GET()

    def do_POST(self):
        state = self.server.state
        body = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))))
        if self.headers.get('x-selfware-session') != 'fixture-session':
            return self.reply({'error': 'session_required'}, 401)
        if self.path == '/api/phi/arena/drill':
            correct = body.get('choice_index') == body.get('correct_index')
            return self.reply({
                'action': body.get('action', 'answer_drill'),
                'correct': correct,
                'xp_gained': 35 if correct else 0,
                'unlocked': [{'id': 'debt_slayer_i', 'title': '🏆 Debt Slayer I'}] if correct else [],
                'profile': {'xp': 85 if correct else 50, 'level': 1, 'best_streak': 1.5 if correct else 1.25, 'citations_inspected': 1, 'drills_completed': 2 if correct else 1, 'achievement_badges': [{'id': 'first_laser_lock', 'title': '🏆 Optical Lock'}, {'id': 'debt_slayer_i', 'title': '🏆 Debt Slayer I'}]},
                'drill': {'seed': 1, 'question': 'Which workspace file defines `greeting`?', 'symbol_name': 'greeting', 'citation': 'src/lib.rs:1', 'path': 'src/lib.rs', 'start_line': 1, 'end_line': 3, 'options': ['src/lib.rs:1', 'src/safety/redact.rs', 'src/evolve/ontology.rs'], 'correct_index': 0},
            })
        state['posts'].append({'path': self.path, 'body': body})
        if body.get('expected_hash') != document(state['content'])['hash']:
            return self.reply({'error': 'document_changed'}, 409)
        if self.path == '/api/assistant/review':
            state['snapshot'] = document(state['content'])
            return self.reply({'job_id': 'fixture-reading-1', 'status': 'queued'}, 202)
        if self.path == '/api/ide/write':
            state['content'] = body['content']
            return self.reply({'saved': True, 'write': {'hash': document(state['content'])['hash']}, 'graph_refresh': {'success': True}})
        if self.path == '/api/phi/chat':
            snap = document(state['content'])
            evidence = {'id': 'E1', 'path': snap['path'], 'content_hash': snap['hash'], 'start_line': 1, 'end_line': 3,
                        'excerpt': '\n'.join(str(i + 1).rjust(6) + ' | ' + line for i, line in enumerate(snap['content'].splitlines()))}
            return self.reply({
                'reply': f"Anchored to `{snap['path']}`: `greeting` returns an owned String.",
                'emotion': 'analytical',
                'trust_state': 'structural',
                'grounding_scope': 'snapshot_and_citation_integrity_only',
                'semantic_validation': 'not_performed',
                'hallucination_free_guarantee': False,
                'verification_note': 'Grounded in local workspace snapshots; no new test runs were executed by this chat turn.',
                'citations': [evidence],
                'claims': [{'text': '`greeting` returns an owned String.', 'evidence_ids': ['E1']}],
                'recommendations': [],
                'suggested_actions': [{
                    'id': 'laser:src/lib.rs:1',
                    'label': '↗ Aim laser at src/lib.rs:1-3',
                    'kind': 'focus_citation',
                    'target': 'src/lib.rs',
                    'start_line': 1,
                    'end_line': 3,
                }],
                'signals': {'activity_status': 'unavailable', 'agent_count': 0, 'running_agents': 0,
                            'failed_runs': 0, 'unreviewed_lines': 0, 'untested_lines': 0, 'gate_passed': True, 'gate_findings': []},
                'prompt_tokens_measured': 42,
            })
        self.reply({'error': 'unknown_route'}, 404)


@unittest.skipUnless(sync_playwright, 'Playwright is required for Phi workspace browser contracts')
class PhiWorkspaceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Handler, directory=str(WEB)))
        cls.thread = Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.playwright = sync_playwright().start()
        try:
            cls.browser = cls.playwright.chromium.launch(channel='chrome', headless=True)
        except Exception:
            cls.browser = cls.playwright.chromium.launch(headless=True)

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)

    def setUp(self):
        self.server.state = {'content': SOURCE, 'posts': [], 'polls': 0, 'release': False, 'snapshot': None}
        self.page = self.browser.new_page(viewport={'width': 1440, 'height': 1000})
        self.errors = []
        self.page.on('pageerror', lambda error: self.errors.append(str(error)))
        self.page.goto(f'http://127.0.0.1:{self.server.server_port}/phi/')
        self.page.wait_for_function("window.phiApp?.document?.hash && !phiApp.examples")
        self.page.evaluate('phiApp.viseme.setAudioEnabled(false)')

    def tearDown(self):
        self.page.close()
        self.assertEqual(self.errors, [])

    def reading(self):
        self.page.locator('#btn-read-all').click()
        self.page.wait_for_function('phiApp.workspace.pending()?.id')
        self.server.state['release'] = True
        self.page.wait_for_function('phiApp.facts.length === 1')

    def test_pending_reading_survives_reload_without_second_model_request(self):
        self.page.locator('#btn-read-all').click()
        self.page.wait_for_function('phiApp.workspace.pending()?.id')
        self.page.reload()
        self.page.wait_for_function('phiApp.polling')
        self.server.state['release'] = True
        self.page.wait_for_function('phiApp.facts.length === 1')
        self.assertEqual(len(self.server.state['posts']), 1)
        self.assertEqual(self.server.state['posts'][0]['body']['expected_hash'], document(SOURCE)['hash'])
        self.assertIn('Source references checked', self.page.locator('#facts-status').inner_text())
        self.assertNotIn('verified', self.page.locator('#agent-state').inner_text().lower())
        self.page.evaluate('phiApp.viseme.setAudioEnabled(false)')
        self.page.locator('.super-fact .btn-cyber').click()
        self.page.wait_for_function('phiApp.focus.active?.target?.descriptor?.start === 7')
        self.assertEqual(self.page.evaluate('phiApp.focus.active.target.descriptor.line'), 1)

    def test_saved_edit_is_read_back_and_dirty_source_cannot_generate(self):
        self.page.locator('#btn-edit').click()
        changed = SOURCE.replace('Hello', 'A new idea')
        self.page.locator('#code-buffer').fill(changed)
        self.assertTrue(self.page.locator('#btn-read-all').is_disabled())
        self.page.locator('#btn-save').click()
        self.page.wait_for_function('!phiApp.dirty')
        self.assertEqual(self.server.state['content'], changed)
        self.assertEqual(self.page.evaluate('phiApp.document.hash'), document(changed)['hash'])
        self.assertIn('read back from disk', self.page.locator('#workspace-status').inner_text())

    def test_conflicting_save_keeps_the_user_buffer(self):
        self.page.locator('#btn-edit').click()
        changed = SOURCE.replace('Hello', 'Unsaved work')
        self.page.locator('#code-buffer').fill(changed)
        self.server.state['content'] = SOURCE + '// another writer\n'
        self.page.locator('#btn-save').click()
        self.page.wait_for_function("document.getElementById('workspace-status').textContent.includes('document_changed')")
        self.assertEqual(self.page.locator('#code-buffer').input_value(), changed)
        self.assertTrue(self.page.evaluate('phiApp.dirty'))
        self.assertTrue(self.server.state['content'].endswith('// another writer\n'))
        self.page.evaluate('phiApp.dirty=false')

    def test_stale_citation_does_not_point_at_new_source(self):
        self.reading()
        self.server.state['content'] = SOURCE.replace('greeting', 'different_function')
        self.page.locator('.citation').click()
        self.page.wait_for_function("document.getElementById('workspace-status').textContent.includes('different saved version')")
        self.assertIsNone(self.page.evaluate('phiApp.focus.active'))

    def test_selected_words_feed_exact_source_offsets_and_captions(self):
        self.page.evaluate('''() => {
          const code=document.querySelector('[data-line="2"] .line-code');
          // Resolve the same source offsets through syntax-colored text nodes.
          const point=offset=>{const walker=document.createTreeWalker(code,NodeFilter.SHOW_TEXT);let node;
            while(node=walker.nextNode()){if(offset<=node.length)return[node,offset];offset-=node.length;}throw Error('offset unavailable');};
          const range=document.createRange();range.setStart(...point(18));range.setEnd(...point(49));
          const selection=getSelection();selection.removeAllRanges();selection.addRange(range);
        }''')
        self.page.locator('#btn-read-selection').click()
        self.page.wait_for_function('phiApp.focus.active?.target?.descriptor?.line === 2')
        self.assertEqual(self.page.evaluate('phiApp.focus.active.target.descriptor.start'), 18)
        self.assertEqual(self.page.evaluate('phiApp.currentText'), SOURCE.splitlines()[1][18:49])
        self.page.locator('#btn-stop-mission').click()
        self.assertFalse(self.page.evaluate('phiApp.viseme.isPlaying'))

    def test_responsive_workspace_and_docked_mascot_leave_request_accessible(self):
        for width in (390, 768, 1440):
            self.page.set_viewport_size({'width': width, 'height': 1000})
            self.page.evaluate('phiApp.dock()')
            self.page.wait_for_timeout(550)
            self.assertFalse(self.page.evaluate('document.documentElement.scrollWidth>innerWidth'), str(width))
        self.page.locator('#reading-question').fill('Explain the return value.')
        self.assertEqual(self.page.locator('#reading-question').input_value(), 'Explain the return value.')

    def test_phi_bot_multi_turn_chat_quick_chips_and_laser_citation_focus(self):
        self.assertEqual(self.server.state['posts'], [])
        self.page.locator('[data-bot-intent="explain_selection"]').click()
        self.page.wait_for_function('phiApp.bot.turns.length === 3')
        self.assertEqual(len(self.server.state['posts']), 1)
        self.assertEqual(self.server.state['posts'][0]['path'], '/api/phi/chat')
        self.assertEqual(self.server.state['posts'][0]['body']['intent'], 'explain_selection')
        self.assertEqual(self.server.state['posts'][0]['body']['expected_hash'], document(SOURCE)['hash'])
        self.assertIn('42 prompt tokens measured', self.page.locator('#phi-bot-status').inner_text())
        self.assertEqual(self.page.locator('#phi-bot-trust-badge').inner_text(), 'STRUCTURAL')

        self.page.locator('.phi-bot-citation').first.click()
        self.page.wait_for_function('phiApp.focus.active?.target?.descriptor?.line === 1')
        self.assertEqual(self.page.evaluate('phiApp.focus.active.target.descriptor.endLine'), 3)

        self.page.locator('#phi-bot-input').fill('Follow up question about greeting')
        self.page.locator('#btn-phi-bot-send').click()
        self.page.wait_for_function('phiApp.bot.turns.length === 5')
        self.assertEqual(len(self.server.state['posts']), 2)
        self.assertEqual(len(self.server.state['posts'][1]['body']['history']), 2)

        # Verify Boss Battle Drill card & XP progression in browser UI
        self.page.locator('#btn-phi-bot-drill').click()
        self.page.wait_for_function("document.querySelectorAll('.phi-bot-drill-card').length === 1")
        self.page.locator('.phi-bot-drill-option[data-drill-choice="0"]').click()
        self.page.wait_for_function("document.getElementById('phi-bot-arena-badge').textContent.includes('85 XP')")
        self.assertIn('CRITICAL HIT', self.page.locator('.phi-bot-drill-feedback').inner_text())


if __name__ == '__main__':
    unittest.main()
