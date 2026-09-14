#!/usr/bin/env python3
"""Delayed-response regressions for the production Phi workspace UI.

The existing HTTP fixture boots the real browser modules. Deferred bridge reads
and writes control response ordering deterministically; they do not contact a
model or modify a real workspace. No sleeps simulate network races.
"""
from hashlib import sha256
from pathlib import Path
import importlib.util
import unittest

_fixture_path = Path(__file__).with_name('test_phi_workspace.py')
_spec = importlib.util.spec_from_file_location('_phi_workspace_race_fixture', _fixture_path)
fixture = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fixture)

OTHER_SOURCE = 'pub fn other() {}\n'
OTHER_DOCUMENT = {'path': 'src/other.rs', 'content': OTHER_SOURCE,
                  'hash': sha256(OTHER_SOURCE.encode()).hexdigest(), 'language': 'rust'}


@unittest.skipUnless(fixture.sync_playwright, 'Playwright is required for Phi workspace race contracts')
class PhiWorkspaceRaceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixture.PhiWorkspaceTests.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        fixture.PhiWorkspaceTests.tearDownClass.__func__(cls)

    def setUp(self):
        fixture.PhiWorkspaceTests.setUp(self)
        self.page.evaluate('value => {window.otherDocument=value}', OTHER_DOCUMENT)

    def tearDown(self):
        self.page.evaluate('phiApp.dirty=false')
        fixture.PhiWorkspaceTests.tearDown(self)

    def test_edits_entered_while_another_file_loads_are_retained(self):
        self.page.evaluate('''() => {
          phiApp.workspace.read = () => new Promise(resolve => {window.releaseOpen=resolve});
          window.pendingOpen=phiApp.openFile(otherDocument.path);
        }''')
        self.page.locator('#btn-edit').click()
        changed = fixture.SOURCE.replace('Hello', 'Unsaved during loading')
        self.page.locator('#code-buffer').fill(changed)
        self.assertTrue(self.page.evaluate('phiApp.dirty'))
        self.page.evaluate('''async () => {releaseOpen(otherDocument);await pendingOpen}''')
        self.assertEqual(self.page.evaluate('phiApp.document.path'), 'src/lib.rs')
        self.assertEqual(self.page.locator('#code-buffer').input_value(), changed)
        self.assertTrue(self.page.evaluate('phiApp.dirty'))
        self.assertTrue(self.page.locator('#code-buffer').is_visible())
        self.assertTrue(self.page.locator('#btn-read-all').is_disabled())

    def test_cancelled_citation_cannot_rebind_another_files_dirty_buffer(self):
        self.page.evaluate('''() => {
          const app=phiApp;
          window.originalDocument=app.document;
          const first=originalDocument.content.split('\\n')[0];
          const evidence={path:originalDocument.path,content_hash:originalDocument.hash,
            start_line:1,end_line:1,excerpt:'     1 | '+first};
          app.workspace.read=path=>path===originalDocument.path
            ? new Promise(resolve=>{window.releaseCitation=resolve})
            : Promise.resolve(otherDocument);
          window.pendingCitation=app.focusCitation(evidence);
        }''')
        self.page.locator('#btn-stop-mission').click()
        self.assertTrue(self.page.evaluate('phiApp.openFile(otherDocument.path)'))
        self.page.locator('#btn-edit').click()
        changed = 'pub fn user_edits_belong_to_other_file() {}\n'
        self.page.locator('#code-buffer').fill(changed)
        self.page.evaluate('''async () => {releaseCitation(originalDocument);await pendingCitation}''')
        self.assertEqual(self.page.evaluate('phiApp.document.path'), OTHER_DOCUMENT['path'])
        self.assertEqual(self.page.locator('#active-tab-title').inner_text(), OTHER_DOCUMENT['path'])
        self.assertEqual(self.page.locator('#code-buffer').input_value(), changed)
        self.assertTrue(self.page.evaluate('phiApp.dirty'))
        self.assertIsNone(self.page.evaluate('phiApp.focus.active'))
        # Saving afterward must address the file that owns these edits, with
        # that file's prior hash. Merely preserving textarea text is not enough.
        self.page.evaluate('''async () => {
          phiApp.workspace.write=async(document,content)=>{
            window.capturedWrite={path:document.path,expected_hash:document.hash,content};
            return {write:{hash:'saved-other-fixture-hash'}};
          };
          phiApp.workspace.read=async path=>({path,content:capturedWrite.content,
            hash:'saved-other-fixture-hash',language:'rust'});
          await phiApp.save();
        }''')
        self.assertEqual(self.page.evaluate('capturedWrite'), {
            'path': OTHER_DOCUMENT['path'], 'expected_hash': OTHER_DOCUMENT['hash'], 'content': changed})
        self.assertFalse(self.page.evaluate('phiApp.dirty'))

    def test_delayed_save_does_not_restore_a_file_after_user_reverts_and_navigates(self):
        self.page.locator('#btn-edit').click()
        changed = fixture.SOURCE.replace('Hello', 'Saved in background')
        self.page.locator('#code-buffer').fill(changed)
        self.page.evaluate('''() => {
          window.saveSnapshot={...phiApp.document,content:document.getElementById('code-buffer').value,
            hash:'saved-lib-fixture-hash'};
          phiApp.workspace.write=(document,content)=>{
            window.capturedWrite={path:document.path,expected_hash:document.hash,content};
            return new Promise(resolve=>{window.releaseSave=resolve});
          };
          phiApp.workspace.read=async path=>path===otherDocument.path?otherDocument:saveSnapshot;
          window.pendingSave=phiApp.save();
        }''')
        # Reverting the visible buffer legitimately clears dirty while the
        # earlier save is still pending; opening another file is now allowed.
        self.page.locator('#code-buffer').fill(fixture.SOURCE)
        self.assertFalse(self.page.evaluate('phiApp.dirty'))
        self.assertTrue(self.page.evaluate('phiApp.openFile(otherDocument.path)'))
        self.page.evaluate('''async () => {
          releaseSave({write:{hash:saveSnapshot.hash},graph_refresh:{success:true}});
          await pendingSave;
        }''')
        self.assertEqual(self.page.evaluate('capturedWrite.content'), changed)
        self.assertEqual(self.page.evaluate('capturedWrite.path'), 'src/lib.rs')
        self.assertEqual(self.page.evaluate('phiApp.document'), OTHER_DOCUMENT)
        self.assertEqual(self.page.locator('#active-tab-title').inner_text(), OTHER_DOCUMENT['path'])
        self.assertEqual(self.page.locator('#code-buffer').input_value(), OTHER_SOURCE)
        self.assertFalse(self.page.evaluate('phiApp.dirty'))
        self.assertTrue(self.page.locator('#btn-save').is_disabled())

    def install_voice_fixture(self):
        # Exercise production session/mission transitions without depending on
        # an installed OS voice or producing sound from an automated test.
        self.page.evaluate('''() => {
          phiApp.viseme.ready=Promise.resolve(true);
          window.SpeechSynthesisUtterance=class {constructor(text){this.text=text}};
          window.fixtureSynth={calls:[],getVoices:()=>[{name:'Local fixture',voiceURI:'fixture',
            lang:'en-US',localService:true}],
            speak(utterance){this.calls.push(utterance);this.current=utterance;utterance.onstart?.()},
            cancel(){},pause(){},resume(){}};
          phiApp.viseme.synth=fixtureSynth;phiApp.viseme.initSpeech();phiApp.viseme.setAudioEnabled(true);
          const play=phiApp.play.bind(phiApp);
          phiApp.play=(...args)=>{window.pendingPlayback=play(...args);return pendingPlayback};
        }''')

    def assert_paused_before_speech_then_resume(self):
        self.page.wait_for_function('phiApp.viseme.session?.paused === true')
        self.assertEqual(self.page.evaluate('fixtureSynth.calls.length'), 0)
        self.assertFalse(self.page.evaluate('phiApp.viseme.isPlaying'))
        self.assertEqual(self.page.locator('#btn-pause').get_attribute('aria-pressed'), 'true')
        self.assertEqual(self.page.locator('#btn-pause').inner_text(), 'Resume reading')
        self.assertEqual(self.page.locator('#speech-status').inner_text(), 'Reading paused')
        self.page.locator('#btn-pause').click()
        self.page.wait_for_function('fixtureSynth.calls.length === 1')
        self.assertFalse(self.page.evaluate('phiApp.viseme.session.paused'))
        self.assertTrue(self.page.evaluate('phiApp.viseme.isPlaying'))
        self.assertEqual(self.page.locator('#btn-pause').get_attribute('aria-pressed'), 'false')
        self.page.evaluate('''async () => {fixtureSynth.current.onend();await pendingPlayback}''')
        self.assertEqual(self.page.locator('#deck-mission-status').inner_text(), 'Reading complete')
        self.assertEqual(self.page.locator('#speech-status').inner_text(), 'Speech complete')
        self.assertEqual(self.page.locator('#reading-progress').get_attribute('aria-valuenow'), '100')
        self.assertFalse(self.page.evaluate('phiApp.orchestrator.isRunning'))
        self.assertIsNone(self.page.evaluate('phiApp.viseme.session'))

    def test_pause_during_focus_scroll_holds_the_upcoming_speech(self):
        self.install_voice_fixture()
        self.page.evaluate('''() => {
          phiApp.focus.waitForScroll=()=>new Promise(resolve=>{window.releaseScroll=resolve});
        }''')
        self.page.locator('[data-line="1"] .line-num').click()
        self.page.wait_for_function("typeof releaseScroll === 'function'")
        self.assertIsNone(self.page.evaluate('phiApp.viseme.session'))
        self.page.locator('#btn-pause').click()
        self.page.evaluate('releaseScroll(true)')
        self.assert_paused_before_speech_then_resume()

    def test_pause_during_fact_snapshot_fetch_survives_starting_playback(self):
        self.install_voice_fixture()
        self.page.evaluate('''() => {
          const snapshot=phiApp.document,first=snapshot.content.split('\\n')[0];
          window.citationSnapshot=snapshot;
          phiApp.reading={review:{trust_state:'structural',evidence_complete:true,
            claims:[{text:'`greeting` returns a source value.',evidence_ids:['E1']}],recommendations:[],
            evidence:[{id:'E1',path:snapshot.path,content_hash:snapshot.hash,start_line:1,end_line:1,
              excerpt:'     1 | '+first}]}};
          phiApp.renderFacts();
          phiApp.workspace.read=()=>new Promise(resolve=>{window.releaseFact=resolve});
          phiApp.focus.waitForScroll=async()=>true;
        }''')
        self.page.locator('.super-fact .btn-cyber').click()
        self.page.wait_for_function("typeof releaseFact === 'function'")
        self.assertIsNone(self.page.evaluate('phiApp.viseme.session'))
        self.page.locator('#btn-pause').click()
        self.page.evaluate('releaseFact(citationSnapshot)')
        self.assert_paused_before_speech_then_resume()

    def test_voice_off_stops_mission_before_later_steps_and_reports_stopped(self):
        self.install_voice_fixture()
        self.page.evaluate('''() => {
          phiApp.focus.waitForScroll=async()=>true;
          phiApp.play([{line:1,text:'First explanation',status:'First step'},
            {line:2,text:'Second explanation must not start',status:'Second step'}]);
        }''')
        self.page.wait_for_function('fixtureSynth.calls.length===1 && phiApp.viseme.session?.audible===true')
        self.page.locator('#btn-toggle-audio').click()
        self.page.evaluate('pendingPlayback')
        self.assertEqual(self.page.evaluate('fixtureSynth.calls.length'), 1)
        self.assertEqual(self.page.evaluate('phiApp.viseme.getStatus().reason'), 'audio_disabled')
        self.assertFalse(self.page.evaluate('phiApp.orchestrator.isRunning'))
        self.assertIsNone(self.page.evaluate('phiApp.viseme.session'))
        self.assertIsNone(self.page.evaluate('phiApp.focus.active'))
        self.assertEqual(self.page.locator('#deck-mission-status').inner_text(), 'Reading stopped')
        self.assertEqual(self.page.locator('#speech-status').inner_text(), 'Speech stopped')
        self.assertEqual(self.page.locator('#btn-toggle-audio').inner_text(), 'Voice off')
        self.assertEqual(self.page.locator('#btn-pause').inner_text(), 'Pause reading')
        self.assertEqual(self.page.locator('#btn-pause').get_attribute('aria-pressed'), 'false')
        self.assertNotEqual(self.page.locator('#reading-progress').get_attribute('aria-valuenow'), '100')

    def test_fresh_multiline_citation_focuses_entire_last_short_line(self):
        result = self.page.evaluate('''async () => {
          const {resolveEvidence,focusForClaim}=await import('/phi/phi_workspace.js');
          const snapshot=phiApp.document,lines=snapshot.content.split('\\n');
          const evidence={path:snapshot.path,content_hash:snapshot.hash,start_line:1,end_line:3,
            excerpt:lines.slice(0,3).map((line,i)=>String(i+1).padStart(6)+' | '+line).join('\\n')};
          const descriptor=resolveEvidence(evidence,snapshot);
          const fallback=focusForClaim('This function returns a greeting.',evidence,snapshot);
          const target=phiApp.focus.resolveRange(descriptor);
          const receipt=await phiApp.focus.focusRange(descriptor,{scrollIntoView:false,dwellMs:0});
          return {descriptor,fallback,receipt,expectedText:lines.slice(0,3).join('\\n'),
            target:target?{text:target.text,startOffset:target.range.startOffset,endOffset:target.range.endOffset,
              startLine:target.range.startContainer.parentElement.closest('[data-line]').dataset.line,
              endLine:target.range.endContainer.parentElement.closest('[data-line]').dataset.line,
              endNodeText:target.range.endContainer.textContent,rangeText:target.range.toString()}:null};
        }''')
        self.assertEqual(result['descriptor'], {'line': 1, 'endLine': 3, 'start': 0, 'end': 1, 'kind': 'citation'})
        self.assertEqual(result['fallback'], result['descriptor'])
        self.assertEqual(result['receipt']['status'], 'completed')
        self.assertIsNotNone(result['target'])
        self.assertEqual(result['target']['text'], result['expectedText'])
        self.assertEqual(result['target']['startLine'], '1')
        self.assertEqual(result['target']['endLine'], '3')
        self.assertEqual(result['target']['startOffset'], 0)
        self.assertEqual(result['target']['endOffset'], len(result['target']['endNodeText']))
        self.assertEqual(result['target']['endNodeText'], '}')
        self.assertTrue(result['target']['rangeText'].endswith('}'))


if __name__ == '__main__':
    unittest.main()
