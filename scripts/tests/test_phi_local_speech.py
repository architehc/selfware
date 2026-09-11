"""Actual Phi speech owner: local generation and media lifecycle, no model calls."""
import json
import pathlib
import shutil
import subprocess
import unittest
import test_phi_workspace as workspace_fixture

ROOT = pathlib.Path(__file__).resolve().parents[2]


class LocalSpeechTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required for the actual speech owner regressions')
        source = f"""
import assert from 'node:assert/strict';
import {{PhiVisemeEngine}} from {json.dumps((ROOT/'src/evolve/web/phi/phi_viseme.js').as_uri())};
const flush=()=>new Promise(resolve=>setImmediate(resolve));
let clock=0,disposed=0;const media=[];
class AudioFixture extends EventTarget{{constructor(url){{super();this.url=url;this.currentTime=0;this.plays=0;this.pauses=0;media.push(this)}}play(){{this.plays++;return Promise.resolve()}}pause(){{this.pauses++}}fire(type){{this.dispatchEvent(new Event(type))}}}}
globalThis.Audio=AudioFixture;globalThis.SpeechSynthesisUtterance=class{{constructor(text){{this.text=text}}}};
const synth={{calls:[],getVoices:()=>[{{name:'Daniel',voiceURI:'local',localService:true,lang:'en-US'}}],addEventListener(){{}},removeEventListener(){{}},speak(u){{this.calls.push(u)}},cancel(){{}},pause(){{}},resume(){{}}}};
globalThis.window={{speechSynthesis:synth,location:{{origin:'http://localhost',href:'http://localhost/phi/'}}}};
const rig={{calls:[],setSpeechText(text){{this.text=text}},setViseme(v,o){{this.calls.push([v,o])}},setAudioVolume(){{}}}};
const result=()=>({{url:'blob:fixture',job:{{audio:{{duration:4}}}},dispose(){{disposed++}}}});
function make(client){{return new PhiVisemeEngine(rig,{{speechClient:client,loadDictionary:false,now:()=>clock}})}}
""" + body
        done = subprocess.run(['node', '--input-type=module', '-'], input=source, text=True,
                              capture_output=True, timeout=10)
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_stop_during_generation_settles_and_discards_late_audio(self):
        self.node("""
let release,signal;const e=make({synthesize:(text,voice,options)=>{signal=options.signal;return new Promise(r=>release=r)}});
const first=e.speak('Pending local narration',{engine:'vibevoice'});await flush();const session=e.session;e.stop();
assert.equal((await first).status,'cancelled');assert(signal.aborted);release(result());await flush();assert.equal(media.length,0);assert.equal(disposed,1);assert.equal(e.session,null);e.destroy();
""")

    def test_superseded_generation_cannot_replace_new_speech(self):
        self.node("""
const releases=[];const e=make({synthesize:()=>new Promise(r=>releases.push(r))});
const first=e.speak('First',{engine:'vibevoice'});await flush();const second=e.speak('Second',{engine:'vibevoice'});await flush();
assert.equal((await first).status,'cancelled');releases[0](result());await flush();assert.equal(media.length,0);assert.equal(e.session.text,'Second');
releases[1](result());await flush();assert.equal(media.length,1);assert.equal(disposed,1);media[0].fire('playing');media[0].fire('ended');
assert.equal((await second).status,'completed');assert.equal(disposed,2);e.destroy();
""")

    def test_pause_before_ready_retains_same_session_and_prevents_autoplay(self):
        self.node("""
let release;const e=make({synthesize:()=>new Promise(r=>release=r)});const p=e.speak('Pause this narration',{engine:'vibevoice',speechRate:1.5});await flush();
const original=e.session;e.pause();release(result());await flush();assert.equal(e.session,original);assert.equal(media[0].plays,0);assert.equal(e.getStatus().status,'paused');
e.resume();assert.equal(media[0].plays,1);assert.equal(media[0].playbackRate,1.5);media[0].fire('playing');assert.equal(e.getStatus().status,'playing');
e.pause();media[0].fire('playing');assert.equal(e.getStatus().status,'paused');e.stop();assert.equal((await p).status,'cancelled');assert.equal(disposed,1);e.destroy();
""")

    def test_audio_off_and_silent_engine_never_generate(self):
        self.node("""
let calls=0;const e=make({synthesize:()=>{calls++;return Promise.resolve(result())}});e.setEngine('vibevoice');e.setAudioEnabled(false);
let p=e.speak('Muted narration');await flush();clock=2000;e.update(0);assert.equal((await p).mode,'silent_approximate');assert.equal(calls,0);
e.setAudioEnabled(true);p=e.speak('Silent narration',{engine:'silent'});await flush();clock=4000;e.update(0);assert.equal((await p).audible,false);assert.equal(calls,0);assert.equal(synth.calls.length,0);e.destroy();
""")

    def test_mute_during_generation_aborts_and_does_not_fallback(self):
        self.node("""
let release,signal;const e=make({synthesize:(_,v,o)=>{signal=o.signal;return new Promise(r=>release=r)}});
const p=e.speak('Pending',{engine:'vibevoice',fallback:true});await flush();e.setAudioEnabled(false);assert.equal((await p).reason,'audio_disabled');assert(signal.aborted);
release(result());await flush();assert.equal(media.length,0);assert.equal(synth.calls.length,0);assert.equal(disposed,1);e.destroy();
""")

    def test_missing_service_is_visible_and_fallback_requires_explicit_policy(self):
        self.node("""
const e=make({synthesize:async()=>{const error=new Error('Worker unavailable');error.code='worker_unavailable';throw error}}),states=[];e.onStateChange=s=>states.push(s);
const failed=await e.speak('No service',{engine:'vibevoice'});assert.equal(failed.status,'error');assert.equal(failed.reason,'worker_unavailable');assert.equal(synth.calls.length,0);
const p=e.speak('Browser fallback',{engine:'vibevoice',fallback:true});await flush();assert.equal(synth.calls.length,1);assert(states.some(s=>s.status==='fallback'&&s.fallbackReason==='worker_unavailable'));
const u=synth.calls[0];u.onstart();u.onend({charIndex:0});const r=await p;assert.equal(r.provider,'browser');assert.equal(r.fallbackReason,'worker_unavailable');e.destroy();
""")

    def test_playback_error_never_replays_with_browser_voice_and_releases_blob(self):
        self.node("""
const e=make({synthesize:async()=>result()});const p=e.speak('Partial narration',{engine:'vibevoice',fallback:true});await flush();media[0].fire('playing');media[0].currentTime=1;media[0].fire('error');
const r=await p;assert.equal(r.status,'error');assert.equal(synth.calls.length,0);assert.equal(disposed,1);media[0].fire('ended');assert.equal(disposed,1);e.destroy();
""")

    def test_approximate_timing_uses_media_clock_and_original_offsets(self):
        self.node("""
let sent;const e=make({synthesize:async text=>{sent=text;return result()}}),seen=[];const text='Option<f64> & final word.';
const p=e.speak(text,{engine:'vibevoice',onWord:(w,i,d)=>seen.push([w,i,d])});await flush();const audio=media[0];assert(!/[<>&]/.test(sent));assert.equal(rig.text,text);
assert.equal(e.getStatus().approximate,true);assert.equal(e.getStatus().wordTiming,'audio_duration_approximate');audio.fire('playing');audio.currentTime=3.999;e.update(0);
assert.deepEqual(seen.at(-1).slice(0,2),['word.',text.indexOf('word.')]);assert.equal(seen.at(-1)[2].timing,'audio_duration_approximate');
audio.fire('waiting');const before=rig.calls.length,words=seen.length;clock=1000;e.update(0);assert.equal(rig.calls.length,before);assert.equal(seen.length,words);assert.equal(e.getStatus().status,'buffering');
audio.currentTime=0;audio.fire('seeked');audio.fire('playing');e.update(0);assert.equal(seen.at(-1)[1],0);audio.fire('ended');const r=await p;assert.equal(r.approximate,true);assert.equal(r.wordTiming,'audio_duration_approximate');assert.equal(disposed,1);e.destroy();
""")

    def test_slow_generation_leaves_measured_playback_budget(self):
        self.node("""
let release;const e=make({synthesize:()=>new Promise(r=>release=r)});const p=e.speak('A longer narration',{engine:'vibevoice'});await flush();
clock=170000;const generated=result();generated.job.audio.duration=60;release(generated);await flush();media[0].fire('playing');
assert.equal(e.session.deadline,245000);clock=190000;media[0].currentTime=20;e.update(0);assert.notEqual(e.session,null);
clock=230000;media[0].currentTime=60;media[0].fire('ended');assert.equal((await p).status,'completed');assert.equal(disposed,1);e.destroy();
""")

    def test_explicit_timeout_remains_total_session_budget(self):
        self.node("""
let release,budget;const e=make({synthesize:(_,v,o)=>{budget=o.timeoutMs;return new Promise(r=>release=r)}});
const p=e.speak('Bounded narration',{engine:'vibevoice',timeoutMs:1000});await flush();assert.equal(budget,1000);
clock=900;release(result());await flush();assert.equal(e.session.deadline,1000);media[0].fire('playing');clock=1100;e.update(0);
const receipt=await p;assert.equal(receipt.status,'error');assert.equal(receipt.reason,'speech_timeout');assert.equal(disposed,1);e.destroy();
""")


@unittest.skipUnless(workspace_fixture.sync_playwright, 'Playwright is required for speech selector integration')
class LocalSpeechUI(unittest.TestCase):
    setUpClass = classmethod(workspace_fixture.PhiWorkspaceTests.setUpClass.__func__)
    tearDownClass = classmethod(workspace_fixture.PhiWorkspaceTests.tearDownClass.__func__)
    tearDown = workspace_fixture.PhiWorkspaceTests.tearDown

    def setUp(self):
        workspace_fixture.PhiWorkspaceTests.setUp(self)
        self.page.locator('summary', has_text='Phi’s voice & expression').click()

    def capabilities(self, state='ready'):
        return {'configured': True, 'status': state, 'provider': 'vibevoice_onnx',
                'model': 'elbruno/VibeVoice-Realtime-0.5B-ONNX', 'revision': 'fixture',
                'voices': [{'id': 'Emma', 'name': 'Emma'}, {'id': 'Extra', 'name': 'Additional voice'}],
                'default_voice': 'Emma', 'sample_rate': 24000, 'max_text_chars': 5000,
                'alignment': {'status': 'unavailable'}, 'streaming': False}

    def test_ready_local_default_and_explicit_browser_silent_choices_survive_refresh(self):
        headers = []

        def reply(route):
            headers.append(route.request.headers)
            route.fulfill(json=self.capabilities())

        self.page.route('**/api/speech/capabilities', reply)
        self.page.evaluate('phiApp.refreshSpeechCapabilities()')
        self.assertEqual(self.page.locator('#voice-choice').input_value(), 'vibevoice:Emma')
        self.assertEqual(self.page.locator('#select-tts-engine').input_value(), 'vibevoice:Emma')
        self.assertIn('Additional voice', self.page.locator('#voice-choice').inner_text())
        self.assertTrue(all(item.get('x-selfware-session') == 'fixture-session' for item in headers))
        for value in ['system:default', 'silent', 'vibevoice:Grace']:
            self.page.locator('#voice-choice').select_option(value)
            self.page.evaluate('phiApp.refreshSpeechCapabilities()')
            self.assertEqual(self.page.locator('#voice-choice').input_value(), value)
            self.assertEqual(self.page.locator('#select-tts-engine').input_value(), value)
        self.page.locator('#select-tts-engine').select_option('silent')
        self.assertEqual(self.page.locator('#voice-choice').input_value(), 'silent')
        self.assertEqual(self.page.evaluate('phiApp.speechOptions().engine'), 'silent')
        self.assertIn('mouth timing is approximate', self.page.locator('#local-speech-status').inner_text())

    def test_loading_capability_does_not_select_local_or_erase_user_choice(self):
        self.page.route('**/api/speech/capabilities', lambda route: route.fulfill(json=self.capabilities('loading')))
        self.page.evaluate('phiApp.refreshSpeechCapabilities()')
        self.assertEqual(self.page.locator('#voice-choice').input_value(), 'system:default')
        self.page.locator('#voice-choice').select_option('vibevoice:Emma')
        self.page.evaluate('phiApp.refreshSpeechCapabilities()')
        self.assertEqual(self.page.locator('#voice-choice').input_value(), 'vibevoice:Emma')
        self.assertIn('loading', self.page.locator('#local-speech-status').inner_text())

    def test_actual_mission_path_forwards_auto_local_native_and_silent_options(self):
        self.page.route('**/api/speech/capabilities', lambda route: route.fulfill(json=self.capabilities()))
        self.page.evaluate('phiApp.refreshSpeechCapabilities()')
        self.page.evaluate("""() => {
          window.__forwardedSpeech=[];
          phiApp.viseme.speak=async(text,options)=>{__forwardedSpeech.push({text,engine:options.engine,voice:options.voice,
            useSpeechSynthesis:options.useSpeechSynthesis,fallback:options.fallback});return {status:'completed'}};
        }""")
        self.assertEqual(self.page.evaluate('phiApp.viseme.preferredEngine'), 'vibevoice')
        self.page.evaluate("phiApp.play([{line:1,text:'An actual mission path.'}])")
        first = self.page.evaluate('__forwardedSpeech.at(-1)')
        self.assertEqual(first['engine'], 'vibevoice')
        self.assertEqual(first['voice'], 'Emma')
        self.assertFalse(first['fallback'])
        for choice, expected in [('system:default', 'native'), ('silent', 'silent')]:
            self.page.locator('#select-tts-engine').select_option(choice)
            self.page.evaluate("phiApp.play([{line:1,text:'An actual mission path.'}])")
            forwarded = self.page.evaluate('__forwardedSpeech.at(-1)')
            self.assertEqual(forwarded['engine'], expected)
            self.assertEqual(forwarded['useSpeechSynthesis'], expected == 'native')
        self.assertEqual(self.page.evaluate('__forwardedSpeech.length'), 3)


if __name__ == '__main__':
    unittest.main()
