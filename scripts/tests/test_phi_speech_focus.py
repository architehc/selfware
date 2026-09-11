"""Actual Phi modules: speech lifecycle/timing and measured browser source ranges."""
import functools
import hashlib
import http.server
import importlib.util
import json
import math
import os
import pathlib
import shutil
import subprocess
import threading
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
PHI = ROOT / 'src/evolve/web/phi'


class SpeechTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required for browser-module regressions')
        source = f"""
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {{PhiVisemeEngine,buildNativeNarration}} from {json.dumps((PHI / 'phi_viseme.js').as_uri())};
const flush=()=>new Promise(resolve=>setImmediate(resolve));
globalThis.SpeechSynthesisUtterance=class {{constructor(text){{this.text=text}}}};
function make(voices=[{{name:'Samantha',lang:'en-US',localService:true,voiceURI:'local'}}]){{
 let clock=0;
 const rig={{calls:[],setSpeechText(text){{this.speechText=text}},setViseme(v,o){{this.calls.push([v,o])}},setAudioVolume(v){{assert.equal(v,0)}}}};
 const synth={{calls:[],cancelled:0,getVoices:()=>voices,addEventListener(){{}},removeEventListener(){{}},
 speak(u){{this.calls.push(u);this.current=u}},cancel(){{this.cancelled++;this.current?.onerror?.({{error:'canceled'}})}},pause(){{}},resume(){{}}}};
 globalThis.window={{speechSynthesis:synth,location:{{href:'http://localhost/phi/',origin:'http://localhost'}}}};
 const e=new PhiVisemeEngine(rig,{{loadDictionary:false,now:()=>clock}});
 return {{e,rig,synth,time:t=>{{clock=t}}}};
}}
""" + body
        result = subprocess.run(['node', '--input-type=module', '-'], input=source,
                                text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_audio_off_is_silent_and_character_offsets_are_exact(self):
        self.node("""
const {e,synth,time}=make();e.setAudioEnabled(false);const seen=[];
const p=e.speak('Hi,  world!',{onWord:(word,index)=>seen.push([word,index])});await flush();
assert.equal(synth.calls.length,0);time(0);e.update(0);time(400);e.update(0);
assert.deepEqual(seen,[['Hi,',0],['world!',5]]);time(1000);e.update(0);
const r=await p;assert.equal(r.status,'completed');assert.equal(r.mode,'silent_approximate');assert.equal(r.audible,false);assert.equal(r.approximate,true);e.destroy();
""")

    def test_stop_settles_and_stale_speech_events_cannot_touch_replacement(self):
        self.node("""
const {e,synth}=make();const first=e.speak('First');await flush();const old=synth.current;
old.onstart();const staleEnd=old.onend,staleError=old.onerror,staleWord=old.onboundary;
const next=e.speak('Second');assert.equal((await first).status,'cancelled');await flush();
synth.current.onstart();staleEnd();staleError({error:'synthesis-failed'});staleWord({name:'word',charIndex:0});
assert.equal(e.session.text,'Second');assert.equal(e.isPlaying,true);e.stop();assert.equal((await next).status,'cancelled');e.destroy();
""")

    def test_mute_cancels_active_audible_speech(self):
        self.node("""
const {e,synth}=make();const p=e.speak('A long utterance');await flush();synth.current.onstart();
e.setAudioEnabled(false);const r=await p;assert.equal(r.status,'cancelled');assert.equal(r.reason,'audio_disabled');assert.equal(e.isPlaying,false);assert.equal(e.session,null);e.destroy();
""")

    def test_pause_holds_clock_and_resume_does_not_expire_silent_sentence(self):
        self.node("""
const {e,time}=make();const p=e.speak('one two',{useSpeechSynthesis:false});await flush();
time(100);e.update(0);e.pause();time(60000);e.update(0);assert.equal(e.getStatus().status,'paused');
e.resume();assert.equal(e.session!==null,true);time(60100);e.update(0);assert.equal(e.session!==null,true);
time(61000);e.update(0);assert.equal((await p).status,'completed');e.destroy();
""")

    def test_native_animation_waits_for_start_and_uses_boundary_offsets(self):
        self.node("""
const {e,rig,synth,time}=make();const seen=[];const p=e.speak('Hi,  world!',{onWord:(w,i,d)=>seen.push([w,i,d.timing])});await flush();
const before=rig.calls.length;time(200);e.update(0);assert.equal(rig.calls.length,before);
synth.current.onstart();synth.current.onboundary({name:'word',charIndex:5});
assert.deepEqual(seen,[['world!',5,'speech_boundary']]);assert.equal(e.getStatus().approximate,true);
e.stop();await p;e.destroy();
""")

    def test_no_local_voice_never_silently_uses_remote_voice(self):
        self.node("""
const {e,synth,time}=make([{name:'Google Natural',lang:'en-US',localService:false}]);
const p=e.speak('hello');await flush();assert.equal(synth.calls.length,0);assert.equal(e.getStatus().reason,'local_voice_unavailable');
time(1000);e.update(0);assert.equal((await p).audible,false);e.destroy();
""")

    def test_native_without_boundaries_is_approximate_then_real_boundary_wins(self):
        self.node("""
const {e,synth,time}=make(),seen=[];const p=e.speak('one two three',{onWord:(w,i,d)=>seen.push([w,i,d.timing])});await flush();synth.current.onstart();
time(800);e.update(0);assert.deepEqual(seen,[['three',8,'approximate']]);assert.equal(e.getStatus().wordTiming,'approximate');
synth.current.onboundary({name:'word',charIndex:4});assert.deepEqual(seen.at(-1),['two',4,'speech_boundary']);assert.equal(e.getStatus().wordTiming,'speech_boundary');
e.stop();await p;e.destroy();
""")

    def test_native_start_while_paused_keeps_paused_status(self):
        self.node("""
const {e,synth,time}=make();const p=e.speak('hello');await flush();time(100);e.pause();time(1000);synth.current.onstart();
assert.equal(e.getStatus().status,'paused');assert.equal(e.isPlaying,false);e.resume();assert.equal(e.getStatus().status,'playing');e.stop();await p;e.destroy();
""")

    def test_native_code_delimiters_are_spoken_without_changing_source_offsets(self):
        self.node("""
const {e,synth,rig}=make(),seen=[],text='The function returns Option<f64>, accepts &values, and finishes with unusable.';
const p=e.speak(text,{onWord:(word,index)=>seen.push([word,index])});await flush();const u=synth.current;u.onstart();
assert.equal(rig.speechText,text);assert(!/[<>&]/.test(u.text));assert(u.text.includes('less than'));assert(u.text.includes('greater than'));assert(u.text.includes('ampersand'));
for(const word of ['Option','less','f64','greater','ampersand','values','unusable'])u.onboundary({name:'word',charIndex:u.text.indexOf(word),charLength:word.length});
assert.deepEqual(seen.at(-1),['unusable.',text.indexOf('unusable')]);assert.deepEqual(seen[1],['Option<f64>,',text.indexOf('Option')]);
u.onend({charIndex:0});const r=await p;assert.equal(r.status,'completed');assert.equal(r.nativeTextNormalized,true);assert.equal(r.nativeCompletion.coverage,'final_word_boundary');assert.equal(r.nativeCompletion.finalWordBoundaryReached,true);
const unicode='🦊 Option<f64> & β',plan=buildNativeNarration(unicode);assert.equal(plan.originalOffsets[plan.text.indexOf('β')],unicode.indexOf('β'));assert.equal(plan.originalOffsets.at(-1),unicode.length);e.destroy();
""")

    def test_sparse_native_boundaries_report_coverage_without_false_failure(self):
        self.node("""
const {e,synth}=make();const p=e.speak('hello world');await flush();const u=synth.current;u.onstart();u.onboundary({name:'word',charIndex:0,charLength:5});u.onend({charIndex:0});
const r=await p;assert.equal(r.status,'completed');assert.equal(r.nativeCompletion.coverage,'partial_word_boundaries');assert.equal(r.nativeCompletion.finalWordBoundaryReached,false);assert.equal(r.nativeTextNormalized,false);e.destroy();
""")

    def test_missing_native_completion_times_out_instead_of_hanging(self):
        self.node("""
const {e,time}=make();const p=e.speak('hello',{timeoutMs:100});await flush();time(101);e.update(0);
const r=await p;assert.equal(r.status,'error');assert.equal(r.reason,'speech_timeout');e.destroy();
""")

    def test_dictionary_integrity_and_irregular_english_pronunciations(self):
        data = (PHI / 'assets/cmudict.dict').read_bytes()
        self.assertEqual(hashlib.sha256(data).hexdigest(), '81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22')
        self.node(f"""
const {{e}}=make();assert(e.wordToVisemes('constructor',300).length>0);e.loadDictionaryText(fs.readFileSync({json.dumps(str(PHI / 'assets/cmudict.dict'))},'utf8'));
assert(e.dictionary.size>=120000);
for(const [word,phones] of Object.entries({{enough:['ih','n','ah','f'],though:['dh','ow'],through:['th','r','uw'],queue:['k','y','uw'],knowledge:['n','aa','l','ah','jh']}})){{
 const v=e.wordToVisemes(word,320);assert.deepEqual(v.map(x=>x.phoneme),phones);assert(v.every(x=>x.source==='cmudict'));assert(Math.abs(v.reduce((a,b)=>a+b.duration,0)-320)<1e-7);
}}
assert(e.wordToVisemes('constructor',300).length>0);assert(e.wordToVisemes('readFile',300).length>0);e.destroy();
""")

    def test_audio_timestamps_follow_media_clock_and_stop_resolves(self):
        self.node("""
class Media extends EventTarget {constructor(){super();this.currentTime=0}play(){this.dispatchEvent(new Event('playing'));return Promise.resolve()}pause(){}}
const {e,rig}=make(),audio=new Media(),words=[];
const p=e.speakAudio({audio,text:'Hello world',words:[{start:0,end:.5,charStart:0,charEnd:5},{start:.8,end:1.4,charStart:6,charEnd:11}],phonemes:[{start:0,end:.4,viseme:'ai'},{start:.8,end:1.2,viseme:'wq'}]},{onWord:(w,i,d)=>words.push([w,i,d.timing])});
audio.currentTime=.9;e.update(0);assert.deepEqual(words,[['world',6,'audio_timestamp']]);assert.equal(rig.calls.at(-1)[0],'wq');
assert.equal(e.getStatus().approximate,false);e.pause();audio.currentTime=1.3;e.update(0);assert.equal(e.getStatus().status,'paused');e.resume();e.stop();assert.equal((await p).status,'cancelled');e.destroy();
""")

    def test_invalid_alignment_is_rejected_before_playback(self):
        self.node("""
const {e}=make();let played=0;const audio={play(){played++},addEventListener(){}};
const r=await e.speakAudio({audio,text:'word',words:[{start:1,end:.5,charStart:0,charEnd:4}]});
assert.equal(r.status,'error');assert.equal(played,0);e.destroy();
""")

    def test_synchronous_media_play_failure_cleans_up_both_start_and_resume(self):
        self.node("""
class Media extends EventTarget {constructor(){super();this.currentTime=0;this.fail=true}play(){if(this.fail)throw new Error('blocked');this.dispatchEvent(new Event('playing'));return Promise.resolve()}pause(){}}
const {e}=make(),audio=new Media();
const failed=await e.speakAudio({audio,text:'hello'});assert.equal(failed.status,'error');assert.equal(failed.reason,'blocked');assert.equal(e.session,null);assert.equal(e.isPlaying,false);
audio.fail=false;const resumed=e.speakAudio({audio,text:'hello'});e.pause();audio.fail=true;e.resume();
assert.equal((await resumed).status,'error');assert.equal(e.session,null);assert.equal(e.getStatus().status,'error');e.destroy();
""")

    def test_waiting_observer_cancellation_never_starts_audio(self):
        self.node("""
const {e,synth}=make();e.onStateChange=s=>{if(s.status==='waiting')e.stop('ui_cancelled')};
const native=await e.speak('hello');assert.equal(native.status,'cancelled');assert.equal(synth.calls.length,0);
let played=0;class Media extends EventTarget {play(){played++;return Promise.resolve()}pause(){}}
const timed=await e.speakAudio({audio:new Media(),text:'hello'});assert.equal(timed.status,'cancelled');assert.equal(played,0);assert.equal(e.session,null);e.destroy();
""")

    def test_numeric_and_punctuation_tokens_preserve_silent_word_timing(self):
        self.node("""
const {e,time}=make();const seen=[];const duration=60000/155;
const schedule=e.sentenceToVisemes('42 + hello');assert(Math.abs(schedule.reduce((s,p)=>s+p.duration,0)-duration*3)<1e-7);
const p=e.speak('42 + hello',{useSpeechSynthesis:false,onWord:(w,i)=>seen.push([w,i])});await flush();
time(duration*2+1);e.update(0);assert.deepEqual(seen,[['hello',5]]);assert.equal(e.isPlaying,true);
time(duration*3+1);e.update(0);assert.equal((await p).status,'completed');e.destroy();
""")

    def test_invalid_timeout_cannot_create_unbounded_speech(self):
        self.node("""
const {e,synth}=make();for(const timeoutMs of [Infinity,NaN,0,-1,180001]){
assert.equal((await e.speak('hello',{timeoutMs})).reason,'invalid_timeout');
assert.equal((await e.speakAudio({text:'hello'},{timeoutMs})).reason,'invalid_timeout');}
assert.equal(synth.calls.length,0);assert.equal(e.session,null);e.destroy();
""")


class FocusBrowserTests(unittest.TestCase):
    def test_range_follows_scroll_and_new_focus_cancels_old_request(self):
        chrome = pathlib.Path('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome')
        if not chrome.exists() or not importlib.util.find_spec('playwright'):
            self.skipTest('Optional Chrome/Playwright integration check unavailable')
        from playwright.sync_api import sync_playwright
        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *args):
                pass
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(PHI)))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        url = f'http://127.0.0.1:{server.server_port}'
        fixture = '''<style>body{margin:30px}.editor{height:180px;width:700px;overflow:auto}.code-line{height:24px;white-space:pre;font:16px/24px monospace}.line-num{display:inline-block;width:40px}</style><div class="editor"></div><script type="module">
import {PhiFocusCoordinator} from '/phi_focus.js';
const editor=document.querySelector('.editor');
for(let i=1;i<=80;i++){const row=document.createElement('div');row.className='code-line';row.dataset.line=i;row.innerHTML=`<span class="line-num">${i}</span><span class="line-code"><span>alpha </span><b>beta</b> gamma ${i}</span>`;editor.append(row)}
const rig={target:null,setEmotion(){},flyTo(x,y){this.park=[x,y]},gazeAt(x,y){this.target=[x,y]},fireLaser(x,y){this.target=[x,y]},stopLaser(){},setSpeechText(){}};
window.focusTest=new PhiFocusCoordinator(rig,{stop(){},speak(){return Promise.resolve({status:'completed'})}},editor);window.testRig=rig;
</script>'''
        with sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=str(chrome), headless=True)
            try:
                page = browser.new_page(viewport={'width':1100,'height':750}, reduced_motion='reduce')
                page.route(url + '/fixture', lambda route: route.fulfill(body=fixture, content_type='text/html'))
                page.goto(url + '/fixture')
                page.wait_for_function('window.focusTest !== undefined')
                outcome = page.evaluate("focusTest.focusRange({line:35,start:6,end:10},{dwellMs:0})")
                self.assertEqual(outcome['status'], 'completed')
                measured = page.evaluate('''() => {const a=focusTest.active,m=focusTest.measure(a.target),o=focusTest.spotlightOverlay;return {text:a.target.range.toString(),rectTop:m.rect.top,overlayTop:parseFloat(o.style.top),rectWidth:m.rect.width,overlayWidth:parseFloat(o.style.width)}}''')
                self.assertEqual(measured['text'], 'beta')
                self.assertAlmostEqual(measured['rectTop'], measured['overlayTop'], delta=.1)
                self.assertAlmostEqual(measured['rectWidth'], measured['overlayWidth'], delta=.1)
                page.evaluate("document.querySelector('.editor').scrollTop-=48")
                page.wait_for_timeout(80)
                after = page.evaluate('''() => ({actual:focusTest.measure(focusTest.active.target).rect.top,overlay:parseFloat(focusTest.spotlightOverlay.style.top)})''')
                self.assertAlmostEqual(after['actual'], after['overlay'], delta=.1)
                self.assertAlmostEqual(after['actual'] - measured['rectTop'], 48, delta=.1)
                races = page.evaluate('''async()=>{const first=focusTest.focusLine(70,{dwellMs:0});const second=focusTest.focusLine(4,{dwellMs:0});return {first:await first,second:await second,active:focusTest.active.target.descriptor.line}}''')
                self.assertEqual(races['first']['status'], 'cancelled')
                self.assertEqual(races['second']['status'], 'completed')
                self.assertEqual(races['active'], 4)
                whole_selection = page.evaluate('''async()=>{testRig.setAvoidRect=rect=>testRig.avoid=rect;const original=focusTest.viseme.speak;focusTest.viseme.speak=async(text,options)=>{options.onWord('alpha',0,{charStart:0,charEnd:5});return {status:'completed'}};await focusTest.focusRange({line:4},{scrollIntoView:false,spokenText:'alpha beta gamma 4'});focusTest.viseme.speak=original;const anchor=focusTest.measure(focusTest.active.target).rect,word=focusTest.measure(focusTest.active.wordTarget).rect,[x,y]=testRig.park;return {anchorWidth:anchor.width,wordWidth:word.width,avoidWidth:testRig.avoid.width,spotlightWidth:parseFloat(focusTest.spotlightOverlay.style.width),wholeSourceClear:x>=anchor.right||x+240<=anchor.left||y>=anchor.bottom||y+240<=anchor.top}}''')
                self.assertGreater(whole_selection['anchorWidth'], whole_selection['wordWidth'])
                self.assertAlmostEqual(whole_selection['avoidWidth'], whole_selection['anchorWidth'], delta=.1)
                self.assertAlmostEqual(whole_selection['spotlightWidth'], whole_selection['wordWidth'], delta=.1)
                self.assertTrue(whole_selection['wholeSourceClear'])
                page.evaluate("document.querySelector('.editor').style.width='100px';document.querySelector('.editor').scrollLeft=110")
                page.wait_for_timeout(80)
                hidden_word = page.evaluate('''()=>({anchorVisible:focusTest.measure(focusTest.active.target).visible,wordVisible:focusTest.measure(focusTest.active.wordTarget).visible,sourceStillProtected:testRig.avoid!==null,spotlightDisplay:focusTest.spotlightOverlay.style.display})''')
                self.assertTrue(hidden_word['anchorVisible'])
                self.assertFalse(hidden_word['wordVisible'])
                self.assertTrue(hidden_word['sourceStillProtected'])
                self.assertEqual(hidden_word['spotlightDisplay'], 'none')
                page.evaluate("document.querySelector('.editor').style.width='700px';document.querySelector('.editor').scrollLeft=0")
                page.wait_for_timeout(80)
                block = page.evaluate('''async()=>{await focusTest.focusRange({line:4,start:6,endLine:6,end:10},{scrollIntoView:false,dwellMs:0});const a=focusTest.active;return {text:a.target.text,lines:a.target.elements.length,rects:focusTest.measure(a.target).rects.length}}''')
                self.assertEqual(block['text'], 'beta gamma 4\nalpha beta gamma 5\nalpha beta')
                self.assertEqual(block['lines'], 3)
                self.assertGreaterEqual(block['rects'], 3)
                parking = page.evaluate('''async()=>{const perch=document.createElement('div');perch.id='phi-perch';perch.style.cssText='position:fixed;left:800px;top:30px;width:270px;height:340px';document.body.append(perch);const control=document.createElement('textarea');control.style.cssText='position:fixed;left:800px;top:420px;width:250px;height:150px';document.body.append(control);testRig.getLayoutSize=()=>({width:160,height:180,leftInset:20,rightInset:20,topInset:70,bottomInset:10});await focusTest.focusRange({line:5},{scrollIntoView:false,dwellMs:0});const [x,y]=testRig.park,p=perch.getBoundingClientRect(),c=control.getBoundingClientRect();const r={left:x-20,right:x+180,top:y-70,bottom:y+190};return {inPerch:r.left>=p.left&&r.right<=p.right&&r.top>=p.top&&r.bottom<=p.bottom,avoidsControl:r.right<=c.left||r.left>=c.right||r.bottom<=c.top||r.top>=c.bottom}}''')
                self.assertTrue(parking['inPerch'])
                self.assertTrue(parking['avoidsControl'])
                page.evaluate("document.querySelector('.editor').scrollTop=1500")
                page.wait_for_timeout(80)
                clipped = page.evaluate('''()=>({active:focusTest.active!==null,visible:focusTest.measure(focusTest.active.target).visible,display:focusTest.spotlightOverlay.style.display})''')
                self.assertTrue(clipped['active'])
                self.assertFalse(clipped['visible'])
                self.assertEqual(clipped['display'], 'none')
                page.evaluate("document.querySelector('.editor').replaceChildren()")
                page.wait_for_timeout(80)
                self.assertTrue(page.evaluate('focusTest.active===null'))
                unavailable = page.evaluate('''async()=>{const row=document.createElement('div');row.dataset.line=1;row.className='code-line';row.style.display='none';row.textContent='hidden source';document.querySelector('.editor').append(row);return await focusTest.focusLine(1,{dwellMs:0})}''')
                self.assertEqual(unavailable['status'], 'error')
                self.assertEqual(unavailable['reason'], 'source_range_not_visible')
                self.assertTrue(page.evaluate('focusTest.active===null'))
                page.evaluate('focusTest.destroy()')
            finally:
                browser.close()

    def test_entire_mobile_motion_retains_clear_parking_across_word_changes(self):
        chrome = pathlib.Path('/Applications/Google Chrome.app/Contents/MacOS/Google Chrome')
        if not chrome.exists() or not importlib.util.find_spec('playwright'):
            self.skipTest('Optional Chrome/Playwright integration check unavailable')
        from playwright.sync_api import sync_playwright
        class Handler(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *args):
                pass
        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(Handler, directory=str(PHI)))
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        url = f'http://127.0.0.1:{server.server_port}'
        fixture = '''<link rel="stylesheet" href="/style.css"><style>body{margin:0;background:#090e17}.editor{position:fixed;top:480px;left:30px;width:350px;height:40px;overflow:auto}.line-code{font:12px/20px monospace;white-space:pre}#phi-perch{position:fixed;left:20px;top:30px;width:350px;height:360px}#blocker{position:fixed;left:70px;top:280px;width:280px;height:130px}</style><div id="phi-perch"></div><textarea id="blocker">An interactive control temporarily occupies the upper parking region.</textarea><div class="editor"><div class="code-line" data-line="1"><span class="line-code">Hello from your local workspace</span></div></div><script type="module">
import {PhiMascotRig} from '/phi_rig.js';import {PhiVisemeEngine} from '/phi_viseme.js';import {PhiFocusCoordinator} from '/phi_focus.js';
const rig=new PhiMascotRig(document.body,{width:220,height:220,initialX:65,initialY:620});const viseme=new PhiVisemeEngine(rig,{loadDictionary:false,audioEnabled:false});const focus=new PhiFocusCoordinator(rig,viseme,document.querySelector('.editor'));
window.motionFrames=[];window.motionWords=[];window.motionFocus=focus;let previous=performance.now(),raf;
function frame(now){rig.update(Math.min(.05,(now-previous)/1000));previous=now;const source=focus.active&&focus.measure(focus.active.target);if(source?.visible&&focus.parkingPosition)motionFrames.push({t:now,rig:rig.getBounds(),source:source.rect,park:{...focus.parkingPosition}});raf=requestAnimationFrame(frame)}raf=requestAnimationFrame(frame);
window.runMotion=()=>focus.focusRange({line:1},{scrollIntoView:false,spokenText:'Hello from your local workspace',speechRate:.6,onWord:(word,index)=>{motionWords.push({word,index});if(index>0)document.getElementById('blocker')?.remove()}}).then(result=>{window.motionResult=result});
window.destroyMotion=()=>{cancelAnimationFrame(raf);focus.destroy();viseme.destroy();rig.destroy()};window.motionReady=true;
</script>'''
        with sync_playwright() as pw:
            browser = pw.chromium.launch(executable_path=str(chrome), headless=True)
            try:
                page = browser.new_page(viewport={'width':390,'height':1000})
                page.route(url + '/motion', lambda route: route.fulfill(body=fixture, content_type='text/html'))
                page.goto(url + '/motion')
                page.wait_for_function('window.motionReady')
                page.evaluate('runMotion()')
                page.wait_for_function('window.motionResult')
                receipt = page.evaluate('({frames:motionFrames,words:motionWords,result:motionResult})')
                self.assertEqual(receipt['result']['status'], 'completed')
                self.assertGreaterEqual(len(receipt['words']), 4)
                self.assertGreaterEqual(len(receipt['frames']), 30)
                self.assertGreater(receipt['frames'][-1]['t'] - receipt['frames'][0]['t'], 2500)
                for frame in receipt['frames']:
                    self.assertTrue(all(isinstance(value, (int, float)) and math.isfinite(value) for value in frame['rig'].values()))
                    body, source = frame['rig'], frame['source']
                    self.assertGreaterEqual(body['left'], 0, frame)
                    self.assertGreaterEqual(body['top'], 0, frame)
                    self.assertLessEqual(body['right'], 390, frame)
                    self.assertLessEqual(body['bottom'], 1000, frame)
                    overlap = max(0, min(body['right'], source['right']) - max(body['left'], source['left'])) * max(0, min(body['bottom'], source['bottom']) - max(body['top'], source['top']))
                    self.assertEqual(overlap, 0, frame)
                    self.assertGreater(frame['park']['parkY'], source['bottom'], frame)
                if os.environ.get('PHI_FOCUS_MOTION_ARTIFACTS'):
                    output = pathlib.Path(os.environ['PHI_FOCUS_MOTION_ARTIFACTS'])
                    output.mkdir(parents=True, exist_ok=True)
                    (output / 'hysteresis-trajectory.json').write_text(json.dumps(receipt, indent=2))
                    page.screenshot(path=str(output / '08-hysteresis-trajectory.png'))
                page.evaluate('motionFocus.cancel();window.cancelReset=motionFocus.parkingPosition===null')
                self.assertTrue(page.evaluate('window.cancelReset'))
                page.evaluate('destroyMotion()')
            finally:
                browser.close()


if __name__ == '__main__':
    unittest.main()
