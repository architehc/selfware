"""Fresh Chrome against production Phi modules; synthetic typed signals, no model calls."""
from functools import partial
from http.server import ThreadingHTTPServer
from pathlib import Path
from threading import Thread
import json
import os
import time
import unittest
from urllib.parse import urlparse

import test_phi_workspace as fixture

PREFS = 'selfware.phi.companion.preferences.v1'


class Handler(fixture.Handler):
    def do_GET(self):
        if urlparse(self.path).path == '/api/friction/events':
            self.server.state.setdefault('friction_gets', []).append(dict(self.headers))
            if self.headers.get('x-selfware-session') != 'fixture-session':
                return self.reply({'error': 'session_required'}, 401)
            return self.reply({'events': [], 'cursor': 0, 'reset': False, 'capabilities': {
                'storage': 'process_memory', 'network': 'none', 'local_model': False,
                'generation_link_verification': 'unavailable',
                'hooks': {'diagnostics': 'server_completed_runs', 'generation': 'server_apply_registry',
                          'review': 'ide_reports', 'undo': 'ide_reports', 'activity': 'ide_reports', 'lsp': 'unavailable'}}})
        super().do_GET()


@unittest.skipUnless(fixture.sync_playwright, 'Playwright is required for actual companion browser contracts')
class FrictionUITests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(('127.0.0.1', 0), partial(Handler, directory=str(fixture.WEB)))
        cls.thread = Thread(target=cls.server.serve_forever, daemon=True); cls.thread.start()
        cls.playwright = fixture.sync_playwright().start()
        try:
            cls.browser = cls.playwright.chromium.launch(channel='chrome', headless=True)
        except Exception:
            cls.browser = cls.playwright.chromium.launch(headless=True)

    tearDownClass = classmethod(fixture.PhiWorkspaceTests.tearDownClass.__func__)
    def setUp(self):
        self.server.state = {'content': fixture.SOURCE, 'posts': [], 'polls': 0, 'release': False, 'snapshot': None}
        self.context = self.browser.new_context(viewport={'width':1440,'height':1000})
        self.page = self.context.new_page(); self.errors = []
        self.page.on('pageerror', lambda error: self.errors.append(str(error)))
        self.page.goto(f'http://127.0.0.1:{self.server.server_port}/phi/')
        self.wait('Boolean(window.phiApp?.document?.hash && !phiApp.examples)')
        self.page.evaluate('phiApp.viseme.setAudioEnabled(false)')
        self.wait("phiApp.companion.connection === 'connected'")
        self.page.evaluate("""() => {
          window.__typedEvents=[]; window.__speechCalls=0;
          const monitor=phiApp.companion.monitor, ingest=monitor.ingest.bind(monitor);
          monitor.ingest=e=>{__typedEvents.push(JSON.parse(JSON.stringify(e)));return ingest(e)};
          const speak=phiApp.viseme.speak.bind(phiApp.viseme);
          phiApp.viseme.speak=(...args)=>{__speechCalls++;return speak(...args)};
        }""")

    def tearDown(self):
        try:
            fixture.PhiWorkspaceTests.tearDown(self)
        finally:
            self.context.close()

    def wait(self, expression, timeout=5):
        end = time.monotonic() + timeout
        while time.monotonic() < end:
            value = self.page.evaluate(expression)
            if value:
                return value
            time.sleep(.04)
        self.fail('Timed out: ' + expression)

    def emit(self, generation='fixture-generation'):
        return self.page.evaluate("""generation=>phiApp.companion.monitor.ingest({id:'event-'+generation,
          kind:'generation_finished',task_id:'fixture-task',generation_id:generation,at_ms:Date.now(),source:'server',
          data:{status:'staged',added_lines:400,deleted_lines:0,files_changed:1}})""", generation)

    def settings(self):
        if self.page.locator('.friction-settings').get_attribute('open') is None:
            self.page.locator('.friction-settings summary').click()

    def test_quiet_nudge_preserves_typing_caret_and_escape_default(self):
        self.page.locator('#btn-edit').click()
        area = self.page.locator('#code-buffer'); area.click(); area.press('End'); area.press('A')
        before = self.page.evaluate("({text:document.querySelector('#code-buffer').value,caret:document.querySelector('#code-buffer').selectionStart})")
        self.page.evaluate("window.addEventListener('keydown',e=>{if(e.key==='Escape')window.__escapePrevented=e.defaultPrevented})")
        self.assertEqual(self.emit()['kind'], 'large_diff')
        self.assertEqual(self.page.evaluate('document.activeElement.id'), 'code-buffer')
        self.assertEqual(self.page.locator('.friction-notice [role=status]').get_attribute('aria-live'), 'polite')
        self.assertEqual(self.page.locator('#code-buffer').input_value(), before['text'])
        self.assertEqual(self.page.evaluate("document.querySelector('#code-buffer').selectionStart"), before['caret'])
        area.press('B'); self.page.keyboard.press('Escape')
        self.assertTrue(self.page.locator('.friction-notice').is_hidden())
        self.assertFalse(self.page.evaluate('__escapePrevented'))
        self.assertEqual(self.page.evaluate('document.activeElement.id'), 'code-buffer')
        self.assertEqual(self.page.evaluate('__speechCalls'), 0)
        self.assertIn('AB', area.input_value())

    def test_intervention_and_evidence_do_not_interrupt_reading_or_change_prompt(self):
        self.page.locator('#reading-question').fill('Keep this exact request.')
        self.page.evaluate("() => {window.__reading=phiApp.viseme.speak('A deliberately long silent reading remains active during the nudge.',{useSpeechSynthesis:false,speechRate:.1})}")
        self.wait('Boolean(phiApp.viseme.session)')
        before = self.page.evaluate('({text:phiApp.viseme.session.text,emotion:phiApp.rig.emotion,god:phiApp.rig.godMode})')
        self.emit()
        self.page.locator('.friction-actions button').click()
        self.assertEqual(self.page.locator('#reading-question').input_value(), 'Keep this exact request.')
        self.assertEqual(self.page.evaluate('phiApp.viseme.session.text'), before['text'])
        self.assertEqual(self.page.evaluate('phiApp.rig.godMode'), before['god'])
        self.assertEqual(self.page.evaluate('__speechCalls'), 1)
        self.assertTrue(self.page.locator('.friction-evidence').is_visible())
        self.assertEqual(self.server.state['posts'], [])
        self.page.evaluate('phiApp.viseme.stop()')

    def test_undo_uses_semantic_beforeinput_and_never_keystroke_contents(self):
        self.page.locator('#btn-edit').click(); area = self.page.locator('#code-buffer'); area.click(); area.press('End')
        area.press_sequentially('typed text for undo')
        self.page.evaluate("window.dispatchEvent(new KeyboardEvent('keydown',{key:'z',ctrlKey:true,bubbles:true}))")
        self.assertEqual(self.page.evaluate("__typedEvents.filter(e=>e.kind==='editor_undo').length"), 0)
        self.page.keyboard.press('ControlOrMeta+z')
        self.wait("__typedEvents.some(e=>e.kind==='editor_undo')")
        events = self.page.evaluate("__typedEvents.filter(e=>e.kind==='editor_undo')")
        self.assertTrue(all(e['data']['count'] == 1 for e in events))
        self.assertTrue(all('generation_id' not in e for e in events))
        self.assertNotIn('typed text', json.dumps(events))
        self.assertTrue(all(set(e['data']) <= {'count', 'document_id'} for e in events))
        self.assertIsNone(self.page.evaluate('phiApp.companion.current'))

    def test_context_navigation_preserves_active_time_cooldown_and_snooze(self):
        result = self.page.evaluate("""() => {
          const c=phiApp.companion;let now=Date.now();c.now=()=>now;c.monitor.now=()=>now;
          now+=1000;c.monitor.ingest({id:'active',kind:'activity',source:'ide',at_ms:now,data:{active_ms:1000,local_hour:1}});
          c.monitor.ingest({id:'gen-a',kind:'generation_finished',task_id:'task-a',generation_id:'g-a',source:'server',at_ms:now,data:{status:'staged',added_lines:400}});
          const first=c.monitor.getSnapshot();c.contextChanged('b'.repeat(64));const changed=c.monitor.getSnapshot();
          c.monitor.ingest({id:'gen-b',kind:'generation_finished',task_id:'task-b',generation_id:'g-b',source:'server',at_ms:now,data:{status:'staged',added_lines:400}});
          const suppressed=c.current;c.snooze();const until=c.monitor.getSnapshot().snoozedUntil;c.contextChanged('c'.repeat(64));
          return {first,changed,suppressed,until,after:c.monitor.getSnapshot()};
        }""")
        self.assertEqual(result['first']['activeMs'], 1000)
        self.assertEqual(result['changed']['activeMs'], 1000)
        self.assertEqual(result['changed']['lastInterventionAt'], result['first']['lastInterventionAt'])
        self.assertIsNone(result['suppressed'])
        self.assertEqual(result['after']['snoozedUntil'], result['until'])
        self.assertEqual(result['after']['activeMs'], 1000)

    def test_off_stops_polling_and_observation_and_syncs_preferences(self):
        self.page.evaluate('phiApp.companion.setEnabled(false)')
        self.assertFalse(self.page.evaluate('phiApp.companion.connected'))
        self.assertIn('server observations may continue', self.page.locator('.friction-status').inner_text())
        count = self.page.evaluate('__typedEvents.length')
        self.page.locator('#btn-edit').click(); self.page.locator('#code-buffer').fill('No recorded activity while off.')
        self.page.evaluate('phiApp.companion.tickActivity()')
        self.assertEqual(self.page.evaluate('__typedEvents.length'), count)
        stored = self.page.evaluate('key=>JSON.parse(localStorage.getItem(key))', PREFS)
        self.assertEqual(stored, {'enabled': False, 'lateNightEnabled': False})
        # A real second same-origin tab writes preferences; storage event updates this UI.
        other = self.page.context.new_page(); other.goto(f'http://127.0.0.1:{self.server.server_port}/phi/')
        other.evaluate('key=>localStorage.setItem(key,JSON.stringify({enabled:true,lateNightEnabled:false}))', PREFS)
        self.wait('phiApp.companion.preferences.enabled === true')
        other.evaluate('key=>localStorage.setItem(key,JSON.stringify({enabled:false,lateNightEnabled:false}))', PREFS)
        self.wait('phiApp.companion.preferences.enabled === false'); other.close()
        self.assertFalse(self.page.evaluate('phiApp.companion.connected'))

    def test_simulations_are_explicit_and_isolated_from_real_evidence(self):
        before = self.page.evaluate('phiApp.companion.monitor.getSnapshot()')
        for kind in ['unresolved_api', 'circular_spin', 'large_diff', 'review_rejections', 'rapid_undo', 'late_night']:
            result = self.page.evaluate('kind=>phiApp.companion.simulate(kind)', kind)
            self.assertEqual(result['current']['kind'], kind)
            self.assertIn('Simulation', self.page.locator('.friction-mode').inner_text())
            self.assertEqual(self.page.evaluate('phiApp.companion.monitor.getSnapshot()'), before)
        self.assertEqual(self.page.evaluate('__speechCalls'), 0)
        self.assertEqual(self.server.state['posts'], [])
        self.assertFalse(self.page.evaluate('phiApp.companion.preferences.lateNightEnabled'))
        self.page.evaluate('phiApp.companion.stopSimulation()')
        self.assertTrue(self.page.locator('.friction-notice').is_hidden())

    def test_missing_hooks_do_not_turn_save_errors_into_diagnostics(self):
        self.settings()
        self.assertIn('No LSP feed', self.page.locator('.friction-settings').inner_text())
        self.page.locator('#btn-edit').click(); self.page.locator('#code-buffer').fill('unsaved buffer')
        self.server.state['content'] = 'Different saved content'
        self.page.locator('#btn-save').click()
        self.wait("document.querySelector('#workspace-status').textContent.includes('Save could not be confirmed')")
        self.assertIsNone(self.page.evaluate('phiApp.companion.current'))
        self.assertFalse(self.page.evaluate("__typedEvents.some(e=>e.kind==='diagnostics_finished')"))
        self.assertEqual(self.page.locator('#code-buffer').input_value(), 'unsaved buffer')
        self.assertEqual(self.page.evaluate('__speechCalls'), 0)

    def test_stalled_body_deadline_changes_connected_feed_to_unavailable(self):
        self.page.evaluate("""() => {
          const c=phiApp.companion;c.requestTimeoutMs=40;c.pollIntervalMs=60000;
          c.fetch=async()=>new Response(new ReadableStream({start(controller){controller.enqueue(new TextEncoder().encode('{'))}}),{headers:{'content-type':'application/json'}});
          c.connect();
        }""")
        self.wait("phiApp.companion.connection === 'unavailable'", timeout=2)
        self.assertIn('unavailable', self.page.locator('.friction-status').inner_text())
        self.assertNotIn('watching', self.page.locator('.friction-status').inner_text())

    def test_disconnect_and_reconnect_reject_late_and_historical_events(self):
        self.page.evaluate("""() => {
          const c=phiApp.companion;c.fetch=()=>new Promise(resolve=>window.__late=resolve);c.connect();c.disconnect();
          __late(new Response(JSON.stringify({events:[{id:'old',kind:'generation_finished',task_id:'old-task',generation_id:'old-gen',source:'server',at_ms:Date.now(),data:{status:'staged',added_lines:400}}],cursor:1,reset:false,capabilities:{}}),{headers:{'content-type':'application/json'}}));
        }""")
        time.sleep(.1)
        self.assertEqual(self.page.evaluate('phiApp.companion.connection'), 'disconnected')
        self.assertIsNone(self.page.evaluate('phiApp.companion.current'))
        self.page.evaluate("""() => {
          const c=phiApp.companion;c.fetch=async()=>new Response(JSON.stringify({events:[{id:'historical',kind:'generation_finished',task_id:'old-task',generation_id:'old-gen',source:'server',at_ms:Date.now()-10000,data:{status:'staged',added_lines:400}}],cursor:2,reset:false,capabilities:{}}),{headers:{'content-type':'application/json'}});c.connect();
        }""")
        self.wait('phiApp.companion.cursor === 2')
        self.assertEqual(self.page.evaluate('phiApp.companion.monitor.getSnapshot().historySize'), 0)
        self.assertIsNone(self.page.evaluate('phiApp.companion.current'))

    def test_successful_diagnostics_clear_the_previous_nudge(self):
        self.emit('resolved-generation')
        self.assertFalse(self.page.locator('.friction-notice').is_hidden())
        self.page.evaluate("""async() => {
          const c=phiApp.companion;c.fetch=async()=>new Response(JSON.stringify({events:[{id:'successful-check',kind:'diagnostics_finished',task_id:'fixture-task',generation_id:'resolved-generation',source:'server',at_ms:Date.now(),data:{success:true,evidence_complete:true,diagnostics:[]}}],cursor:1,reset:false,capabilities:{}}),{headers:{'content-type':'application/json'}});
          await c.poll(c.epoch);
        }""")
        self.assertIsNone(self.page.evaluate('phiApp.companion.current'))
        self.assertTrue(self.page.locator('.friction-notice').is_hidden())

    def test_actual_editing_counts_only_focused_recent_activity(self):
        self.page.locator('#btn-edit').click(); area = self.page.locator('#code-buffer'); area.click(); area.press('End'); area.press('x')
        result = self.page.evaluate("""() => {
          const c=phiApp.companion;let now=Date.now();c.now=()=>now;c.monitor.now=()=>now;c.lastActivityTick=now;c.lastInputAt=now;
          now+=1000;c.tickActivity();const focused=c.monitor.getSnapshot().activeMs;
          document.querySelector('#reading-question').focus();now+=1000;c.tickActivity();const elsewhere=c.monitor.getSnapshot().activeMs;
          c.editor.focus();now+=31000;c.tickActivity();return {focused,elsewhere,stale:c.monitor.getSnapshot().activeMs};
        }""")
        self.assertGreaterEqual(result['focused'], 1000)
        self.assertEqual(result['focused'], result['elsewhere'])
        self.assertEqual(result['elsewhere'], result['stale'])

    def test_disabling_late_night_hides_current_real_nudge(self):
        self.page.evaluate("""() => {
          const c=phiApp.companion;c.setLateNightEnabled(true);
          c.show({id:'late',kind:'late_night',title:'An optional stopping point',message:'A fixture nudge.',motion:'sleep',evidence:[],actions:[],expires_at_ms:Date.now()+60000});
          c.setLateNightEnabled(false);
        }""")
        self.assertTrue(self.page.locator('.friction-notice').is_hidden())

    def test_panel_stays_outside_editor_at_mobile_and_desktop_widths(self):
        self.emit()
        for width in [390, 768, 1440]:
            self.page.set_viewport_size({'width': width, 'height': 1000}); time.sleep(.1)
            result = self.page.evaluate("""() => {
              const a=document.querySelector('#phi-friction-panel').getBoundingClientRect(),b=document.querySelector('.phi-editor-stage').getBoundingClientRect();
              return {overlap:a.left<b.right&&a.right>b.left&&a.top<b.bottom&&a.bottom>b.top,overflow:document.documentElement.scrollWidth>innerWidth};
            }""")
            self.assertFalse(result['overlap'])
            self.assertFalse(result['overflow'])
        if os.environ.get('PHI_FRICTION_ARTIFACTS'):
            out = Path(os.environ['PHI_FRICTION_ARTIFACTS']); out.mkdir(parents=True, exist_ok=True)
            self.page.screenshot(path=str(out/'companion-desktop.png'))
            self.page.set_viewport_size({'width':390,'height':1000}); self.page.locator('#phi-friction-panel').scroll_into_view_if_needed()
            self.page.screenshot(path=str(out/'companion-mobile.png'))

    def test_destroy_removes_polling_editor_and_escape_listeners(self):
        self.page.evaluate('phiApp.companion.destroy()'); count = self.page.evaluate('__typedEvents.length')
        self.page.locator('#btn-edit').click(); self.page.locator('#code-buffer').fill('Editing after companion destruction.')
        self.page.keyboard.press('Escape')
        self.assertEqual(self.page.evaluate('__typedEvents.length'), count)
        self.assertEqual(self.page.locator('#phi-friction-panel').inner_text(), '')


if __name__ == '__main__':
    unittest.main()
