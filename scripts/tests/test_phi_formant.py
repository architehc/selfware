"""The procedural formant voice: audible offline, and silent when it must be.

Phi's mute `local_voice_unavailable` path leaves the mouth moving with no sound.
phi_formant.js fills that with a two-formant vocal tract driven by the SAME
viseme schedule the renderer uses, so lips and sound cannot drift. These tests
use a recording AudioContext fixture — they assert scheduling and teardown, not
audible output, which no unit test can verify.
"""
import json
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
PHI = ROOT / 'src/evolve/web/phi'


class FormantTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required for the formant voice regressions')
        source = f"""
import assert from 'node:assert/strict';
import {{PhiFormantVoice, FORMANTS}} from {json.dumps((PHI/'phi_formant.js').as_uri())};
import {{PhiVisemeEngine}} from {json.dumps((PHI/'phi_viseme.js').as_uri())};
import {{VISEMES}} from {json.dumps((PHI/'phi_rig.js').as_uri())};
const flush=()=>new Promise(resolve=>setImmediate(resolve));
// Recording AudioContext: every node logs what it was asked to do and whether
// it was disconnected, so leaks and stray oscillators are visible.
let ctxTime=0;const started=[],stopped=[],disconnects=[];
const param=()=>({{value:0,setValueAtTime(){{return this}},exponentialRampToValueAtTime(){{return this}}}});
function node(kind,extra={{}}){{return {{kind,connect(){{}},disconnect(){{disconnects.push(kind)}},...extra}}}}
class ContextFixture{{
  constructor(){{this.state='running';this.destination=node('destination');this.closed=0}}
  get currentTime(){{return ctxTime}}
  createOscillator(){{const o=node('osc',{{frequency:param(),onended:null,
    start(t){{started.push(t)}},stop(t){{stopped.push(t)}}}});return o}}
  createBiquadFilter(){{return node('biquad',{{frequency:param(),Q:param()}})}}
  createGain(){{return node('gain',{{gain:param()}})}}
  resume(){{this.state='running'}}
  close(){{this.closed++}}
}}
globalThis.window={{speechSynthesis:null}};
const rig={{setSpeechText(){{}},setViseme(){{}},setAudioVolume(){{}}}};
const schedule=[
  {{viseme:VISEMES.AI,duration:120}},{{viseme:VISEMES.REST,duration:80}},
  {{viseme:VISEMES.E,duration:110}},{{viseme:VISEMES.MBP,duration:70}},
  {{viseme:VISEMES.O,duration:130}}];
""" + body
        done = subprocess.run(['node', '--input-type=module', '-'], input=source, text=True,
                              capture_output=True, timeout=20)
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_every_renderer_viseme_has_a_formant_or_explicit_silence(self):
        """A missing key would drop that mouth shape out of the audio silently."""
        self.node("""
for (const v of Object.values(VISEMES)) assert.ok(v in FORMANTS, `no formant entry for ${v}`);
assert.equal(FORMANTS[VISEMES.REST],null,'rest must be explicit silence, not a tone');
for (const [name,f] of Object.entries(FORMANTS)) {
  if (f===null) continue;
  assert.ok(f.f1>0 && f.f2>0 && f.f2>f.f1, `${name}: F2 must sit above F1`);
  assert.ok(f.gain>0 && f.gain<=1, `${name}: gain out of range`);
}
""")

    def test_rest_frames_schedule_no_oscillator(self):
        self.node("""
const ctx=new ContextFixture();const voice=new PhiFormantVoice({audioContext:ctx});
assert.equal(voice.speak([{viseme:VISEMES.REST,duration:500}]),false,'an all-rest line is silence');
assert.equal(started.length,0);
assert.equal(voice.speak(schedule),true);
assert.equal(voice.plan.length,4,'the rest frame must not become a note');
voice.destroy();
""")

    def test_notes_are_scheduled_within_a_bounded_lookahead(self):
        self.node("""
const ctx=new ContextFixture();const voice=new PhiFormantVoice({audioContext:ctx});
const long=Array.from({length:200},(_,i)=>({viseme:i%2?VISEMES.AI:VISEMES.E,duration:100}));
voice.speak(long);
const first=started.length;
assert.ok(first>0,'the head of the line starts immediately');
assert.ok(first<long.length,'the whole line must not be allocated up front');
ctxTime=5;voice.pump();
assert.ok(started.length>first,'advancing the clock schedules more');
voice.destroy();
""")

    def test_stop_silences_and_disconnects_every_live_node(self):
        self.node("""
const ctx=new ContextFixture();const voice=new PhiFormantVoice({audioContext:ctx});
voice.speak(schedule);
const live=voice.live.length;assert.ok(live>0);
const before=stopped.length;voice.stop();
assert.equal(stopped.length,before+live,'every sounding oscillator is stopped');
assert.equal(voice.live.length,0);assert.equal(voice.timer,null);
assert.ok(disconnects.length>=live*4,'osc, both filters and the gain are released');
voice.destroy();assert.equal(ctx.closed,0,'an injected context is not ours to close');
""")

    def test_offset_resumes_mid_line_without_replaying_spoken_shapes(self):
        self.node("""
const ctx=new ContextFixture();const voice=new PhiFormantVoice({audioContext:ctx});
voice.speak(schedule);const full=voice.plan.length;
voice.speak(schedule,{offsetMs:300});
assert.ok(voice.plan.length<full,'a resume skips what was already spoken');
assert.ok(voice.plan.every(n=>n.at>=0),'offsets never go negative');
voice.destroy();
""")

    def test_engine_stays_mute_unless_the_formant_tier_is_opted_into(self):
        """Existing callers must keep getting silence when no voice is installed."""
        self.node("""
const e=new PhiVisemeEngine(rig,{loadDictionary:false,now:()=>0,
  formantFactory:()=>{throw new Error('the formant voice must not be built unasked')}});
const p=e.speak('offline narration');await flush();
const s=await Promise.race([p,Promise.resolve('pending')]);
assert.equal(e.getStatus().reason,'local_voice_unavailable');
assert.equal(e.getStatus().audible,false);
e.destroy();
""")

    def test_opted_in_engine_becomes_audible_on_the_shared_schedule(self):
        self.node("""
let built=0,spoken=null;
const stub={setVolume(){},speak(sch){spoken=sch;return true},stop(){},destroy(){}};
const e=new PhiVisemeEngine(rig,{loadDictionary:false,now:()=>0,
  formantFactory:()=>{built++;return stub}});
const p=e.speak('offline narration',{engine:'formant'});await flush();
assert.equal(built,1);
assert.equal(e.getStatus().mode,'formant_approximate');
assert.equal(e.getStatus().audible,true);
assert.equal(e.getStatus().reason,'formant_requested');
assert.ok(spoken && spoken.length>0,'the voice is driven by a viseme schedule');
assert.equal(spoken,e.session.schedule,'sound and lips share one timeline');
e.stop();await p;e.destroy();
""")

    def test_a_refusing_voice_falls_through_to_silence_not_a_dead_session(self):
        self.node("""
const stub={setVolume(){},speak:()=>false,stop(){},destroy(){}};
const e=new PhiVisemeEngine(rig,{loadDictionary:false,now:()=>0,formantFactory:()=>stub});
const p=e.speak('offline narration',{engine:'formant'});await flush();
assert.equal(e.getStatus().mode,'silent_approximate');
assert.equal(e.getStatus().audible,false);
e.stop();await p;e.destroy();
""")


if __name__ == '__main__':
    unittest.main()
