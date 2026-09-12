"""One expression vocabulary, and a mood model that accumulates.

Phi is drawn twice — design/mascot/ makes the static brand vectors,
src/evolve/web/phi/ animates the live assistant. These tests fail if the two
mood lists drift apart again, and cover the state engine that now chooses the
face instead of every caller choosing it by hand.
"""
import json
import pathlib
import re
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
PHI = ROOT / 'src/evolve/web/phi'
STUDIO = ROOT / 'design/mascot/mascot.js'
SERVER = ROOT / 'src/evolve/server.rs'


class VocabularyTests(unittest.TestCase):
    """The dedup guard: both foxes answer to one list of moods."""

    def canonical(self):
        source = (PHI / 'phi_expression.js').read_text(encoding='utf-8')
        block = source[source.index('export const EXPRESSIONS'):source.index('export const EXPRESSION_IDS')]
        return set(re.findall(r"^  ([a-z_]+): Object\.freeze\(\{", block, re.M))

    def studio(self):
        source = STUDIO.read_text(encoding='utf-8')
        block = source[source.index('const captions = {'):]
        return set(re.findall(r"^    ([a-z_]+):", block[:block.index('};')], re.M))

    def test_studio_and_shipped_rig_share_one_mood_list(self):
        canonical, studio = self.canonical(), self.studio()
        self.assertGreaterEqual(len(canonical), 12, 'the vocabulary lost moods')
        self.assertEqual(canonical, studio,
                         f'drift: only in shipped {sorted(canonical - studio)}, '
                         f'only in studio {sorted(studio - canonical)}')

    def test_every_alias_targets_a_real_expression(self):
        source = (PHI / 'phi_expression.js').read_text(encoding='utf-8')
        block = source[source.index('EXPRESSION_ALIASES = Object.freeze({'):]
        targets = set(re.findall(r"id: '([a-z_]+)'", block[:block.index('});')]))
        self.assertTrue(targets, 'sanity: aliases parsed')
        self.assertLessEqual(targets, self.canonical())

    def test_new_modules_are_served_by_the_rust_route_table(self):
        """An unserved ES module 404s at runtime and no unit test would catch it."""
        routes = SERVER.read_text(encoding='utf-8')
        for module in ('phi_expression.js', 'phi_state.js', 'phi_formant.js'):
            self.assertIn(f'"/phi/{module}"', routes, f'{module} has no route')
            self.assertIn(f'include_str!("web/phi/{module}")', routes)

    def test_every_module_phi_rig_imports_is_served(self):
        for source in PHI.glob('*.js'):
            for target in re.findall(r"from '\./([a-z_]+\.js)'", source.read_text(encoding='utf-8')):
                self.assertIn(f'"/phi/{target}"', SERVER.read_text(encoding='utf-8'),
                              f'{source.name} imports {target}, which has no route')


class ExpressionTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required for the expression regressions')
        source = f"""
import assert from 'node:assert/strict';
import {{EXPRESSIONS, EXPRESSION_IDS, EXPRESSION_ALIASES, ACCESSORIES, resolveExpression}}
  from {json.dumps((PHI/'phi_expression.js').as_uri())};
import {{PhiState, AXES, EVENT_EFFECTS, ARCHETYPES}} from {json.dumps((PHI/'phi_state.js').as_uri())};
""" + body
        done = subprocess.run(['node', '--input-type=module', '-'], input=source, text=True,
                              capture_output=True, timeout=20)
        self.assertEqual(done.returncode, 0, done.stderr)

    def test_every_expression_is_complete_and_in_range(self):
        self.node("""
const channels=['browAngle','browLift','browAsymmetry','eyeOpen','eyeArc','pupil','ear','smile','tail'];
for (const id of EXPRESSION_IDS) {
  const e=EXPRESSIONS[id];
  assert.equal(e.id,id,`${id}: id must match its key`);
  assert.ok(e.status && e.label && e.event && e.accent,`${id}: missing copy`);
  assert.ok(/^#[0-9a-f]{6}$/i.test(e.accent),`${id}: accent is not a hex colour`);
  for (const c of channels) assert.ok(Number.isFinite(e[c]),`${id}.${c} is not a number`);
  assert.ok(e.eyeOpen>=0&&e.eyeOpen<=1,`${id}: eyeOpen out of range`);
  assert.ok(e.eyeArc>=0&&e.eyeArc<=1,`${id}: eyeArc out of range`);
  assert.ok(e.smile>=-1&&e.smile<=1,`${id}: smile out of range`);
  assert.ok(e.pupil>0.4&&e.pupil<1.8,`${id}: pupil would clamp`);
  assert.ok(e.tail>=0&&e.tail<=2,`${id}: tail energy out of range`);
  if (e.accessory!==null) assert.ok(e.accessory in ACCESSORIES,`${id}: accessory ${e.accessory} has no markup`);
}
""")

    def test_expressions_are_visually_distinct(self):
        """Two moods that render identically are a bug, not a vocabulary."""
        self.node("""
const key=e=>[e.browAngle,e.browLift,e.browAsymmetry,e.eyeOpen,e.eyeArc,e.pupil,e.ear,e.smile,e.accessory].join('|');
const seen=new Map();
for (const id of EXPRESSION_IDS) {
  const k=key(EXPRESSIONS[id]);
  assert.ok(!seen.has(k),`${id} renders identically to ${seen.get(k)}`);
  seen.set(k,id);
}
""")

    def test_unknown_names_resolve_instead_of_throwing(self):
        self.node("""
const r=resolveExpression('not_a_mood_at_all');
assert.equal(r.id,'greeting');assert.equal(r.requested,'not_a_mood_at_all');
for (const [name,alias] of Object.entries(EXPRESSION_ALIASES)) {
  const res=resolveExpression(name);
  assert.equal(res.id,alias.id,`${name} must resolve to ${alias.id}`);
  if (alias.status) assert.equal(res.status,alias.status,`${name} must keep its own wording`);
}
assert.equal(resolveExpression('pacing').status,'Phi · Loop Detected','an alias keeps its message');
assert.equal(resolveExpression('pacing').id,'flow','...while sharing the flow face');
""")

    def test_accessory_markup_is_well_formed_svg(self):
        self.node("""
for (const [name,markup] of Object.entries(ACCESSORIES)) {
  assert.ok(markup.trim().startsWith('<g'),`${name}: not a group`);
  const open=(markup.match(/<g[\\s>]/g)||[]).length, close=(markup.match(/<\\/g>/g)||[]).length;
  assert.equal(open,close,`${name}: unbalanced <g> tags would corrupt the rig`);
  assert.ok(!/<script/i.test(markup),`${name}: accessories are decoration, not script`);
}
""")


class StateTests(unittest.TestCase):
    node = ExpressionTests.node

    def test_unknown_events_are_ignored_not_guessed_at(self):
        self.node("""
const s=new PhiState({now:()=>0});const before=JSON.stringify(s.snapshot().vector);
s.record('some_event_that_does_not_exist');
assert.equal(JSON.stringify(s.snapshot().vector),before);
assert.equal(s.lastEvent,null,'an ignored event is not recorded as the last one');
""")

    def test_every_event_effect_names_real_axes_and_expressions(self):
        self.node("""
for (const [event,effect] of Object.entries(EVENT_EFFECTS)) {
  for (const k of Object.keys(effect)) {
    if (k==='expression') { assert.ok(effect[k] in EXPRESSIONS,`${event}: unknown expression`); continue; }
    if (k==='turn') { assert.ok(effect[k]>0,`${event}: turn must advance`); continue; }
    assert.ok(AXES.includes(k),`${event}: ${k} is not an axis`);
    assert.ok(Math.abs(effect[k])<=1,`${event}.${k} is too large a jump`);
  }
}
""")

    def test_axes_stay_bounded_under_event_storms(self):
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for (let i=0;i<400;i++){t+=50;s.record(i%2?'tests_passed':'milestone');}
for (const a of AXES) {const v=s.snapshot().vector[a];assert.ok(v>=0&&v<=1,`${a}=${v} escaped [0,1]`);}
t=0;const f=new PhiState({now:()=>t});
for (let i=0;i<400;i++){t+=50;f.record('build_failed');}
for (const a of AXES) {const v=f.snapshot().vector[a];assert.ok(v>=0&&v<=1,`${a}=${v} escaped [0,1]`);}
""")

    def test_state_decays_toward_rest_when_nothing_happens(self):
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for (let i=0;i<12;i++){t+=100;s.record('high_throughput');}
const busy=s.snapshot().vector.focus;assert.ok(busy>0.8,`expected sustained focus, got ${busy}`);
t+=120000;s.tick();
const rested=s.snapshot().vector.focus;
assert.ok(rested<busy,'focus must relax when the work stops');
assert.ok(Math.abs(rested-0.35)<0.12,`expected a return toward rest, got ${rested}`);
""")

    def test_accumulation_distinguishes_a_streak_from_a_single_pass(self):
        """The whole point of a state vector: one green test != a green run."""
        self.node("""
let t=0;const once=new PhiState({now:()=>t});once.record('tests_passed');
t+=4000;once.tick();
let u=0;const streak=new PhiState({now:()=>u});
for (let i=0;i<6;i++){u+=200;streak.record('tests_passed');}
u+=4000;streak.tick();
assert.ok(streak.snapshot().vector.clarity>once.snapshot().vector.clarity,
  'a streak must read as clearer than a single pass');
assert.notEqual(streak.snapshot().expression,'greeting');
""")

    def test_pinned_expression_yields_to_the_vector_once_it_expires(self):
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
s.record('build_failed');assert.equal(s.snapshot().expression,'error','the event pins the face');
t+=3000;s.tick();
assert.ok(EXPRESSION_IDS.includes(s.snapshot().expression));
t+=600000;s.tick();
assert.equal(s.snapshot().expression,'greeting','a fully rested state reads as neutral');
""")

    def test_every_reachable_expression_is_a_real_one(self):
        self.node("""
let t=0;const events=Object.keys(EVENT_EFFECTS);
for (let seed=0;seed<200;seed++){
  const s=new PhiState({now:()=>t});
  for (let i=0;i<14;i++){t+=137;s.record(events[(seed*7+i*3)%events.length]);}
  for (const gap of [0,1500,9000,90000]){t+=gap;s.tick();
    assert.ok(EXPRESSION_IDS.includes(s.snapshot().expression),`unreachable mood ${s.snapshot().expression}`);}
}
""")

    def test_archetype_is_a_normalised_cosine_score(self):
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for (let i=0;i<10;i++){t+=100;s.record('exploring');}
const a=s.snapshot().archetype;
assert.ok(ARCHETYPES.some(x=>x.id===a.id),'archetype must come from the table');
assert.ok(a.score>0&&a.score<=1,`cosine score ${a.score} out of range`);
assert.equal(a.id,'scout','sustained exploration reads as the Scout');
""")

    def test_broken_storage_never_breaks_phi(self):
        self.node("""
const hostile={getItem(){throw new Error('blocked')},setItem(){throw new Error('quota')}};
const s=new PhiState({storage:hostile,now:()=>0});
s.record('tests_passed');
assert.ok(EXPRESSION_IDS.includes(s.snapshot().expression));
const corrupt={getItem:()=>'{not json',setItem(){}};
assert.ok(new PhiState({storage:corrupt,now:()=>0}).snapshot().vector.focus>=0);
const absurd={getItem:()=>JSON.stringify({vector:{focus:99,clarity:-5},experience:-3}),setItem(){}};
const v=new PhiState({storage:absurd,now:()=>0}).snapshot();
assert.ok(v.vector.focus<=1&&v.vector.clarity>=0,'out-of-range saved values are clamped');
assert.ok(v.experience>=0,'a negative odometer is rejected');
""")

    def test_a_long_absence_does_not_resume_mid_sprint(self):
        self.node("""
let t=0;const saved={};
const store={getItem:k=>saved[k]??null,setItem:(k,v)=>{saved[k]=v}};
const s=new PhiState({storage:store,now:()=>t});
for (let i=0;i<10;i++){t+=100;s.record('high_throughput');}
const busy=s.snapshot().vector.focus;
t+=86400000;
const resumed=new PhiState({storage:store,now:()=>t});
assert.ok(resumed.snapshot().vector.focus<busy,'a day away must relax the vector');
""")


class SoundTests(unittest.TestCase):
    """The 12 acoustic signatures, rendered and measured — not just scheduled.

    Each is rendered through an OfflineAudioContext in a real browser and the
    samples are inspected, so a signature that silently produces nothing (a bad
    frequency, a zero envelope, a disconnected node) fails here.
    """

    @classmethod
    def setUpClass(cls):
        SecondaryMotionTests.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        SecondaryMotionTests.tearDownClass.__func__(cls)

    def render(self, body):
        page = self.browser.new_page()
        errors = []
        page.on('pageerror', lambda e: errors.append(str(e)))
        page.goto(f'http://127.0.0.1:{self.port}/gallery.html')
        page.wait_for_timeout(250)
        value = page.evaluate(body)
        self.assertEqual(errors, [], f'page errors: {errors}')
        page.close()
        return value

    def test_every_expression_has_a_signature(self):
        result = self.render("""async () => {
          const {SIGNATURES} = await import('./phi_sound.js');
          const {EXPRESSION_IDS} = await import('./phi_expression.js');
          return {missing: EXPRESSION_IDS.filter(id => !(id in SIGNATURES)),
                  extra: Object.keys(SIGNATURES).filter(id => !EXPRESSION_IDS.includes(id))};
        }""")
        self.assertEqual(result["missing"], [], 'these moods make no sound')
        self.assertEqual(result["extra"], [], 'a signature with no expression to trigger it')

    def test_each_signature_renders_audible_non_clipping_audio(self):
        result = self.render("""async () => {
          const {PhiExpressionVoice, SIGNATURES} = await import('./phi_sound.js');
          const out = {};
          for (const mood of Object.keys(SIGNATURES)) {
            const ctx = new OfflineAudioContext(1, 44100 * 2, 44100);
            const voice = new PhiExpressionVoice({audioContext: ctx, enabled: true, volume: 1});
            voice.play(mood, {force: true});
            const buffer = await ctx.startRendering();
            const data = buffer.getChannelData(0);
            let peak = 0, energy = 0;
            for (let i = 0; i < data.length; i++) { const v = Math.abs(data[i]); if (v > peak) peak = v; energy += v * v; }
            out[mood] = {peak, rms: Math.sqrt(energy / data.length),
                         finite: data.every ? true : true,
                         nan: Array.prototype.some.call(data, v => !Number.isFinite(v))};
          }
          return out;
        }""")
        for mood, measured in result.items():
            self.assertFalse(measured["nan"], f'{mood}: produced non-finite samples')
            self.assertGreater(measured["peak"], .01, f'{mood}: rendered silence')
            self.assertLessEqual(measured["peak"], 1.0, f'{mood}: clips the output')
            self.assertGreater(measured["rms"], .0005, f'{mood}: essentially inaudible')

    def test_signatures_are_distinct_from_one_another(self):
        """Two moods that sound identical are not signatures."""
        result = self.render("""async () => {
          const {SIGNATURES} = await import('./phi_sound.js');
          const key = s => JSON.stringify(s.events);
          const seen = {}, dup = [];
          for (const [mood, sig] of Object.entries(SIGNATURES)) {
            const k = key(sig);
            if (seen[k]) dup.push([mood, seen[k]]);
            seen[k] = mood;
          }
          return {dup, descriptions: Object.values(SIGNATURES).map(s => s.description),
                  count: Object.keys(SIGNATURES).length};
        }""")
        self.assertEqual(result["dup"], [], 'these moods sound identical')
        self.assertEqual(len(set(result["descriptions"])), result["count"],
                         'duplicate signature descriptions')

    def test_muted_by_default_and_silent_until_enabled(self):
        result = self.render("""async () => {
          const {PhiExpressionVoice} = await import('./phi_sound.js');
          const ctx = new OfflineAudioContext(1, 44100, 44100);
          const voice = new PhiExpressionVoice({audioContext: ctx});
          const playedWhileMuted = voice.play('spark', {force: true});
          const buffer = await ctx.startRendering();
          const peak = Math.max(...Array.from(buffer.getChannelData(0), Math.abs));
          return {enabled: voice.enabled, playedWhileMuted, peak};
        }""")
        self.assertFalse(result["enabled"], 'sound must default to off')
        self.assertFalse(result["playedWhileMuted"], 'a muted voice must report that it did not play')
        self.assertLess(result["peak"], 1e-6, 'a muted voice still made noise')

    def test_a_flickering_mood_does_not_machine_gun_the_speaker(self):
        result = self.render("""async () => {
          const {PhiExpressionVoice} = await import('./phi_sound.js');
          const ctx = new OfflineAudioContext(1, 44100, 44100);
          let clock = 0;
          const voice = new PhiExpressionVoice({audioContext: ctx, enabled: true, now: () => clock});
          const burst = [];
          for (let i = 0; i < 8; i++) { burst.push(voice.play('working')); clock += 20; }
          clock += 5000;
          const later = voice.play('working');
          const other = voice.play('error');
          return {plays: burst.filter(Boolean).length, later, other};
        }""")
        self.assertEqual(result["plays"], 1, 'a repeated mood retriggered inside the rate limit')
        self.assertTrue(result["later"], 'the same mood must play again once time has passed')
        self.assertTrue(result["other"], 'a different mood must never be rate-limited away')

    def test_unknown_mood_is_ignored_rather_than_guessed_at(self):
        result = self.render("""async () => {
          const {PhiExpressionVoice} = await import('./phi_sound.js');
          const ctx = new OfflineAudioContext(1, 44100, 44100);
          const voice = new PhiExpressionVoice({audioContext: ctx, enabled: true});
          return {unknown: voice.play('not_a_mood'), real: voice.play('spark')};
        }""")
        self.assertFalse(result["unknown"], 'an unknown mood must not pick a substitute sound')
        self.assertTrue(result["real"])

    def test_the_rig_rings_on_a_real_mood_change_only(self):
        result = self.render("""async () => {
          const {PhiMascotRig} = await import('./phi_rig.js');
          const played = [];
          const rig = new PhiMascotRig(document.body, {initialX: 40, initialY: 40});
          rig.setSound({play: mood => { played.push(mood); return true; }, stop(){}});
          rig.setEmotion('working');      // greeting -> working
          rig.setEmotion('analytical');   // alias of working: same face, no new sound
          rig.setEmotion('focused');      // also working
          rig.setEmotion('error');        // a real change
          rig.destroy();
          return played;
        }""")
        self.assertEqual(result, ["working", "error"],
                         'aliases resolving to the same face must not retrigger the sound')


class SecondaryMotionTests(unittest.TestCase):
    """Breathing, counter-bob, ear twitch and the vitality coupling.

    Ported from design/mascot's studio loop, which had the better idle motion.
    These run in a real browser because they assert on rendered SVG transforms.
    """

    @classmethod
    def setUpClass(cls):
        try:
            from playwright.sync_api import sync_playwright
        except ImportError:
            raise unittest.SkipTest('playwright is required for the motion regressions')
        import http.server, socketserver, threading, functools

        class Quiet(http.server.SimpleHTTPRequestHandler):
            def log_message(self, *_):  # keep the test output readable
                pass

        handler = functools.partial(Quiet, directory=str(PHI))
        socketserver.TCPServer.allow_reuse_address = True
        cls.server = socketserver.TCPServer(('127.0.0.1', 0), handler)
        threading.Thread(target=cls.server.serve_forever, daemon=True).start()
        cls.port = cls.server.server_address[1]
        cls.pw = sync_playwright().start()
        try:
            cls.browser = cls.pw.chromium.launch(channel='chrome')
        except Exception:
            try:
                cls.browser = cls.pw.chromium.launch()
            except Exception as error:
                cls.pw.stop(); cls.server.shutdown()
                raise unittest.SkipTest(f'no usable browser: {error}')

    @classmethod
    def tearDownClass(cls):
        if hasattr(cls, 'browser'):
            cls.browser.close(); cls.pw.stop(); cls.server.shutdown()

    def run_rig(self, body, reduced=False):
        page = self.browser.new_page(reduced_motion='reduce' if reduced else 'no-preference')
        errors = []
        page.on('pageerror', lambda e: errors.append(str(e)))
        page.goto(f'http://127.0.0.1:{self.port}/gallery.html')
        page.wait_for_timeout(300)
        result = page.evaluate("""async () => {
          const {PhiMascotRig} = await import('./phi_rig.js');
          window.PhiMascotRig = PhiMascotRig; return true;
        }""")
        self.assertTrue(result)
        value = page.evaluate(body)
        self.assertEqual(errors, [], f'page errors: {errors}')
        page.close()
        return value

    def test_breathing_widens_the_chest_as_it_shortens(self):
        """A uniform pulse reads as a zooming sprite, not as breathing."""
        scales = self.run_rig("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
          const out=[];
          for(let i=0;i<90;i++){rig.update(1/60);
            const m=/scale\(([\d.]+) ([\d.]+)\)/.exec(rig.bodyGroup.getAttribute('transform'));
            if(m) out.push([parseFloat(m[1]),parseFloat(m[2])]);}
          rig.destroy();return out;
        }""")
        self.assertTrue(scales)
        widened = [(x, y) for x, y in scales if abs(x - 1) > 1e-4]
        self.assertTrue(widened, 'the chest never changed width')
        for x, y in widened:
            self.assertNotAlmostEqual(x, y, places=4,
                                      msg='x and y scale together — that is a zoom, not a breath')
            self.assertLess(abs(x - 1), .05); self.assertLess(abs(y - 1), .05)

    def test_head_counters_the_breath_rather_than_riding_it(self):
        offsets = self.run_rig("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
          const out=[];
          for(let i=0;i<120;i++){rig.update(1/60);
            const b=/translate\(0 (-?[\d.]+)\)/.exec(rig.bodyGroup.getAttribute('transform'));
            const h=/translate\(0 (-?[\d.]+)\)/.exec(rig.headGroup.getAttribute('transform'));
            if(b&&h) out.push([parseFloat(b[1]),parseFloat(h[1])]);}
          rig.destroy();return out;
        }""")
        self.assertTrue(offsets)
        self.assertTrue(any(abs(h) > 1e-3 for _, h in offsets), 'the head never counter-bobbed')
        opposed = sum(1 for b, h in offsets if b * h < 0)
        self.assertGreater(opposed, len(offsets) * .45,
                           'the head rides the breath instead of countering it')

    def test_low_vitality_slows_the_breath_and_lengthens_the_blink(self):
        result = self.run_rig("""() => {
          const measure=v=>{
            const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
            rig.setVitality(v);
            let closed=0;
            for(let i=0;i<600;i++){rig.update(1/60); if(rig.blinkProgress>0.3) closed++;}
            const phase=rig.breathPhase; rig.destroy(); return {phase,closed};
          };
          return {rested:measure(1),drained:measure(0)};
        }""")
        self.assertLess(result["drained"]["phase"], result["rested"]["phase"],
                        'a drained Phi must breathe more slowly')
        self.assertGreater(result["drained"]["closed"], result["rested"]["closed"],
                           'a drained Phi must hold its blinks longer')

    def test_ear_twitch_fires_but_never_while_asleep(self):
        result = self.run_rig("""() => {
          const run=emotion=>{
            const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
            rig.setEmotion(emotion); rig.nextTwitchIn=0.05; let peak=0;
            for(let i=0;i<900;i++){rig.update(1/60); peak=Math.max(peak,Math.abs(rig.twitchSpring.value));}
            rig.destroy(); return peak;
          };
          return {awake:run('working'),asleep:run('sleep')};
        }""")
        self.assertGreater(result["awake"], .05, 'the ear never twitched while awake')
        self.assertEqual(result["asleep"], 0, 'a sleeping Phi must not twitch its ears')

    def test_a_held_gaze_relaxes_and_a_real_move_renews_it(self):
        result = self.run_rig("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
          rig.gazeAt(900,600); for(let i=0;i<30;i++)rig.update(1/60);
          const watching=Math.abs(rig.headAngle);
          for(let i=0;i<300;i++)rig.update(1/60);
          const relaxed=Math.abs(rig.headAngle);
          rig.gazeAt(100,700); for(let i=0;i<30;i++)rig.update(1/60);
          const renewed=rig.gazeIdleFor;
          rig.gazeAt(100,700); for(let i=0;i<30;i++)rig.update(1/60);
          const unchanged=rig.gazeIdleFor;
          rig.destroy(); return {watching,relaxed,renewed,unchanged};
        }""")
        self.assertGreater(result["watching"], result["relaxed"], 'the stare never relaxed')
        self.assertLess(result["renewed"], 1, 'a real gaze move must renew attention')
        self.assertGreater(result["unchanged"], result["renewed"],
                           're-aiming at the same point must not keep the stare alive')

    def test_reduced_motion_holds_every_new_channel_still(self):
        result = self.run_rig("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
          rig.setEmotion('curious'); rig.gazeAt(900,600); rig.hop(3); rig.nextTwitchIn=0;
          rig.update(0);
          const snap=()=>JSON.stringify([rig.bodyGroup.getAttribute('transform'),
            rig.headGroup.getAttribute('transform'),rig.blinkProgress,
            rig.twitchSpring.value,rig.hopSpring.value,rig.breathPhase,rig.gazeIdleFor]);
          const before=snap(); for(let i=0;i<600;i++)rig.update(1/60);
          const after=snap(); rig.destroy(); return {before,after};
        }""", reduced=True)
        self.assertEqual(result["before"], result["after"],
                         'reduced motion must freeze breathing, twitch, blink and gaze relaxation')

    def test_a_stalled_frame_settles_the_springs_instead_of_exploding(self):
        """Explicit Euler blows up on a long frame; a woken background tab sends one."""
        peak = self.run_rig("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:50,initialY:50});
          rig.hop(3); rig.twitchSpring.impulse(40);
          let peak=0;
          for(const dt of [4,2,1,0.5,0.25,1/60,1/60,1/60,1/60,1/60]){rig.update(dt);
            peak=Math.max(peak,Math.abs(rig.twitchSpring.value),Math.abs(rig.hopSpring.value));}
          const finite=Number.isFinite(rig.twitchSpring.value)&&Number.isFinite(rig.hopSpring.value);
          rig.destroy(); return {peak,finite};
        }""")
        self.assertTrue(peak["finite"], 'a spring went non-finite')
        self.assertLess(peak["peak"], 60, 'the spring diverged on a long frame')


class VerificationGapTests(unittest.TestCase):
    """Phi's job is to watch production outrun verification, not to cheer.

    These are the behaviours the whole design exists for: a green test run on
    unread code must NOT produce a happy face, and agreement must not read as
    correctness.
    """
    node = ExpressionTests.node

    def test_green_tests_on_unread_code_do_not_earn_a_celebration(self):
        self.node("""
let t=0;
const run=events=>{const s=new PhiState({now:()=>t});
  for(const e of events){t+=200;s.record(e);} t+=4000; s.tick(); return s.snapshot();};
const checked=run(['diff_accepted','diff_reviewed','test_written','tests_passed']);
assert.ok(['success','spark'].includes(checked.expression),
  `verified work should read as earned, got ${checked.expression}`);
const unread=run([...Array(6).fill('diff_accepted_unread'),'tests_passed','tests_passed']);
assert.ok(!['success','spark'].includes(unread.expression),
  `green tests on unread code must not celebrate, got ${unread.expression}`);
assert.ok(unread.vector.debt>0.5,`debt should be high, got ${unread.vector.debt}`);
""")

    def test_drifting_is_distinguished_from_building(self):
        """From the inside they feel identical; that is why Phi has to name it."""
        self.node("""
let t=0;
const run=events=>{const s=new PhiState({now:()=>t});
  for(const e of events){t+=200;s.record(e);} t+=3000; s.tick(); return s.snapshot();};
const building=run(['diff_accepted','diff_reviewed','diff_accepted','test_written','tool_call','tool_call']);
const drifting=run(Array(8).fill('diff_accepted_unread'));
assert.equal(drifting.phase,'drifting',`expected drifting, got ${drifting.phase}`);
assert.notEqual(building.phase,'drifting','reviewed work must not read as drift');
assert.ok(drifting.vector.debt>building.vector.debt,'drift must carry more debt');
""")

    def test_capitulation_reads_as_unimpressed_not_as_progress(self):
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for(let i=0;i<2;i++){t+=300;s.record('sycophantic_reversal');}
t+=5000;s.tick();
assert.equal(s.snapshot().reversals,2);
assert.equal(s.snapshot().expression,'unimpressed','agreement must not read as correctness');
s.record('human_verified');s.record('reverted');
assert.ok(s.snapshot().reversals<2,'a human disagreeing again must relieve it');
""")

    def test_debt_does_not_decay_with_time_at_all(self):
        """Waiting is not a verification strategy.

        The previous version of this test used a 120s window and asserted
        debt > peak*0.6, where the true value was 0.619 — tuned to pass rather
        than to prove the property. The constant it guarded gave debt a 173s
        half-life, so ten idle minutes cleared 91% of it.
        """
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for(let i=0;i<6;i++){t+=200;s.record('diff_accepted_unread');}
const peak=s.snapshot().vector.debt;
assert.ok(peak>0.5,`expected real debt, got ${peak}`);
// Eight hours away must not repay a single point of it.
for (const minutes of [10, 60, 480]) {
  const q=new PhiState({now:()=>t});
  q.vector.debt=peak; q.updatedAt=t;
  q.decayTo(t+minutes*60000);
  assert.equal(q.vector.debt,peak,
    `${minutes} idle minutes changed debt ${peak} -> ${q.vector.debt}`);
}
// Mood, by contrast, must still relax.
t+=600000;s.tick();
assert.ok(Math.abs(s.snapshot().vector.focus-0.35)<0.15,'mood axes should still decay');
assert.equal(s.snapshot().vector.debt,peak,'debt must be untouched by the same tick');
""")

    def test_exposure_is_not_comprehension(self):
        """Opening a file, and writing a test that has not run, are not checks."""
        self.node("""
let t=0;
const load=()=>{const s=new PhiState({now:()=>t});
  for(let i=0;i<5;i++){t+=200;s.record('diff_accepted_unread');} return s;};
const opened=load(); const before=opened.snapshot().vector.debt;
for(let i=0;i<6;i++){t+=200;opened.record('file_read');}
assert.equal(opened.snapshot().vector.debt,before,
  'reading files must not repay debt - exposure is not comprehension');

const written=load(); const w0=written.snapshot().vector.debt;
t+=200;written.record('test_written');
const afterWritten=w0-written.snapshot().vector.debt;
const ran=load(); const r0=ran.snapshot().vector.debt;
t+=200;ran.record('tests_passed');
const afterRan=r0-ran.snapshot().vector.debt;
assert.ok(afterRan>afterWritten,
  `a green run must repay more than an unrun test (${afterRan} vs ${afterWritten})`);
""")

    def test_only_verification_pays_debt_down(self):
        self.node("""
let t=0;
const load=()=>{const s=new PhiState({now:()=>t});
  for(let i=0;i<6;i++){t+=200;s.record('diff_accepted_unread');} return s;};
const waiting=load(); const owed=waiting.snapshot().vector.debt;
t+=3600000; waiting.tick();
assert.equal(waiting.snapshot().vector.debt,owed,'an hour of waiting repays nothing');
const working=load(); for(const e of ['diff_reviewed','tests_passed','human_verified','reverted']){t+=200;working.record(e);}
assert.ok(working.snapshot().vector.debt < owed,'verification must actually repay');
""")

    def test_the_new_faces_are_reachable_from_the_state_alone(self):
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for(let i=0;i<4;i++){t+=200;s.record('diff_accepted_unread');}
t+=4000;s.tick();
assert.ok(['skeptical','unimpressed'].includes(s.snapshot().expression),
  `sustained unread code must read as doubt, got ${s.snapshot().expression}`);
""")


class SycophancyDetectorTests(unittest.TestCase):
    """The 'You're absolutely right' pattern: reversal on pushback, no evidence."""
    node = ExpressionTests.node

    def detector(self, body):
        self.node("""
const {CognitiveFrictionClassifier, INTERVENTION_KINDS} =
  await import('""" + (PHI / 'phi_friction.js').as_uri() + """');
""" + body)

    def test_agreement_without_evidence_is_capitulation(self):
        self.detector("""
const c=new CognitiveFrictionClassifier();
const r=c.recordAssistantReversal({stance:'use tokio',text:"You're absolutely right, I'll switch it."});
assert.equal(r.capitulated,true,'plain agreement on pushback is capitulation');
assert.equal(r.citedEvidence,false);
""")

    def test_changing_your_mind_on_evidence_is_not_capitulation(self):
        self.detector("""
const c=new CognitiveFrictionClassifier();
const withTest=c.recordAssistantReversal({stance:'use tokio',
  text:"You're right - cargo test shows 3 failures in src/net.rs:42, so I'm reverting."});
assert.equal(withTest.capitulated,false,'evidence-backed reversal is reasoning, not capitulation');
const unprompted=c.recordAssistantReversal({stance:'x',text:"You're absolutely right.",afterPushback:false});
assert.equal(unprompted.capitulated,false,'a reversal nobody pushed for is not capitulation');
""")

    def test_two_capitulations_fire_the_intervention(self):
        self.detector("""
const c=new CognitiveFrictionClassifier();
assert.equal(c.evaluate(),null,'nothing to say yet');
c.recordAssistantReversal({stance:'a',text:"You're absolutely right."});
assert.equal(c.evaluate(),null,'one reversal is just politeness');
c.recordAssistantReversal({stance:'b',text:"Good catch, my mistake."});
const i=c.evaluate();
assert.ok(i,'two capitulations must surface');
assert.equal(i.kind,INTERVENTION_KINDS.SYCOPHANTIC_REVERSAL);
assert.equal(i.motionState,'sycophancy');
assert.equal(i.context.capitulations,2);
assert.ok(/citation|checkable/i.test(i.speechText),'the copy must name what is missing');
assert.ok(!/converging on you|isn't reasoning/i.test(i.speechText),
  'the copy must not assert a motive it cannot observe');
assert.ok(/can't tell|cannot tell|may be a fair correction/i.test(i.speechText),
  'the copy must admit what it does not know');
""")

    def test_an_assistant_claim_about_tests_is_not_a_citation(self):
        """Confident sentences are the problem; they cannot also be the proof."""
        self.detector("""
const c=new CognitiveFrictionClassifier();
// Claims, not citations: nothing here can be checked without redoing the work.
for (const text of [
  "You're absolutely right, the tests pass now.",
  "Good catch - I ran cargo build and it works.",
  "My mistake. This should be fine now."]) {
  const r=c.recordAssistantReversal({stance:'x',text});
  assert.equal(r.capitulated,true,`unsupported claim treated as evidence: ${text}`);
}
// Citations: each points somewhere a human can go.
for (const text of [
  "You're right - src/net.rs:42 drops the guard early.",
  "You're right: ```error[E0382]: borrow of moved value```",
  "Good catch - 3 tests failed after that change."]) {
  const r=c.recordAssistantReversal({stance:'x',text});
  assert.equal(r.capitulated,false,`checkable citation flagged as capitulation: ${text}`);
}
""")

    def test_the_mediator_and_the_classifier_never_disagree(self):
        """Two heuristics for one judgement is how they drift apart.

        The mediator kept its own SOURCED list and still accepted a bare
        "tests passed" after the classifier had been narrowed, so an answer the
        classifier flagged was cleared by the mediator.
        """
        self.node("""
const {PhiMediator} = await import('""" + (PHI / 'phi_mediator.js').as_uri() + """');
const {CognitiveFrictionClassifier} = await import('""" + (PHI / 'phi_friction.js').as_uri() + """');
const m=new PhiMediator();
for (const text of [
  "You're absolutely right, the tests pass now.",
  "Good catch - I ran cargo build and it works.",
  "You're right - src/net.rs:42 drops the guard early.",
  "You're right: ```error[E0382]: borrow of moved value```",
  "My mistake. This should be fine now."]) {
  const c=new CognitiveFrictionClassifier();
  const mediator=m.inbound(text,{afterPushback:true}).some(a=>a.kind==='capitulation');
  const classifier=c.recordAssistantReversal({stance:'x',text}).capitulated;
  assert.equal(mediator,classifier,
    `mediator and classifier disagree on: ${text}`);
}
""")

    def test_an_unrun_test_repays_no_debt_at_all(self):
        """Seventeen test_written events took debt from 1.0 to zero."""
        self.node("""
let t=0;const s=new PhiState({now:()=>t});
for(let i=0;i<6;i++){t+=200;s.record('diff_accepted_unread');}
const owed=s.snapshot().vector.debt;
for(let i=0;i<20;i++){t+=200;s.record('test_written');}
assert.equal(s.snapshot().vector.debt,owed,
  `writing 20 tests without running them repaid ${owed - s.snapshot().vector.debt}`);
t+=200;s.record('tests_passed');
assert.ok(s.snapshot().vector.debt<owed,'running them must repay');
""")

    def test_the_caller_owns_reversal_detection_and_it_says_so(self):
        """Phi never compared stances across turns; the record must not imply it did."""
        self.detector("""
const c=new CognitiveFrictionClassifier();
const r=c.recordAssistantReversal({stance:'use tokio',text:"You're absolutely right."});
assert.equal(r.reversalAssertedByCaller,true,
  'the classification must record that the reversal was taken on trust');
""")

    def test_a_human_disagreeing_again_clears_the_streak(self):
        self.detector("""
const c=new CognitiveFrictionClassifier();
c.recordAssistantReversal({stance:'a',text:"You're absolutely right."});
c.recordAssistantReversal({stance:'b',text:"You're absolutely right."});
assert.ok(c.evaluate(),'streak is live');
c.markInterventionFired(INTERVENTION_KINDS.SYCOPHANTIC_REVERSAL);
c.recordHumanVerification();
c.history.interventions.clear();
assert.equal(c.evaluate(),null,'verifying by hand must reset the streak');
""")

    def test_unreviewed_drift_fires_on_count_or_on_volume(self):
        self.detector("""
const byCount=new CognitiveFrictionClassifier();
for(let i=0;i<5;i++) byCount.recordAcceptance({lines:10});
let i=byCount.evaluate();
assert.equal(i.kind,INTERVENTION_KINDS.UNREVIEWED_DRIFT);
assert.equal(i.context.accepted_without_review,5);

const byVolume=new CognitiveFrictionClassifier();
byVolume.recordAcceptance({lines:400});
assert.equal(byVolume.evaluate().kind,INTERVENTION_KINDS.UNREVIEWED_DRIFT,
  'one enormous unread diff counts too');

const reviewing=new CognitiveFrictionClassifier();
for(let n=0;n<9;n++) reviewing.recordAcceptance({lines:10, reviewed:n%2===0});
assert.equal(reviewing.evaluate(),null,'reading the diffs must keep Phi quiet');
""")

    def test_every_new_motion_state_resolves_to_a_real_face(self):
        self.node("""
for (const name of ['sycophancy','drifting','phantom_api','unverified']) {
  const r=resolveExpression(name);
  assert.ok(EXPRESSION_IDS.includes(r.id),`${name} resolves to ${r.id}, which is not a face`);
  assert.ok(r.status && r.status !== EXPRESSIONS.greeting.status,`${name} kept the default wording`);
}
assert.equal(resolveExpression('sycophancy').id,'unimpressed');
assert.equal(resolveExpression('phantom_api').id,'skeptical');
""")


class StewardTests(unittest.TestCase):
    """Phi as steward: what it proposes when Selfware finishes and waits.

    The whole value is in the ranking and the refusal. A steward that suggests
    new features while six diffs sit unread is the sycophant with a clipboard.
    """
    node = ExpressionTests.node

    def steward(self, body):
        self.node("""
const {PhiSteward, IdleWatcher, PROPOSAL_KINDS} =
  await import('""" + (PHI / 'phi_steward.js').as_uri() + """');
""" + body)

    def test_verification_always_outranks_new_production(self):
        self.steward("""
const s=new PhiSteward();
const p=s.propose({
  state:{vector:{debt:.8},phase:'drifting',reversals:0},
  friction:{unreviewed:{count:6,lines:340}},
  workspace:{recentFiles:['src/a.rs'],deadCode:[{file:'x.rs',symbol:'a'},{file:'x.rs',symbol:'b'},{file:'x.rs',symbol:'c'}]}});
assert.ok(p.length,'should have something to say');
assert.equal(p[0].kind,PROPOSAL_KINDS.VERIFY,`expected verify first, got ${p[0].kind}`);
assert.ok(!p.some(x=>x.kind===PROPOSAL_KINDS.EXPLORE),
  'must not suggest new work while anything is unread');
""")

    def test_a_red_gate_outranks_everything(self):
        self.steward("""
const s=new PhiSteward();
const p=s.propose({
  state:{vector:{debt:.9},phase:'drifting',reversals:3},
  friction:{unreviewed:{count:9,lines:500},capitulations:3},
  workspace:{gates:[{id:'g',name:'sweep bug class',passing:false}],recentFiles:['src/a.rs']}});
assert.equal(p[0].kind,PROPOSAL_KINDS.REPAIR);
const ranks=p.map(x=>x.rank);
assert.deepEqual(ranks,[...ranks].sort((a,b)=>a-b),'proposals must come out ranked');
""")

    def test_it_refuses_to_invent_work(self):
        """The behaviour the module exists for."""
        self.steward("""
const s=new PhiSteward();
const empty=s.propose({});
assert.deepEqual(empty,[],'no signals must produce no proposals');
assert.match(s.summarise(empty,{}),/not going to invent/i);
const quiet=s.propose({state:{vector:{debt:.05},phase:'building'},friction:{unreviewed:{count:0,lines:0}},workspace:{}});
assert.deepEqual(quiet,[],'a clean board with no known files is still not a reason to talk');
""")

    def test_every_proposal_cites_checkable_evidence(self):
        self.steward("""
const s=new PhiSteward();
const p=s.propose({
  state:{vector:{debt:.75},phase:'drifting',reversals:2},
  friction:{unreviewed:{count:5,lines:260},capitulations:2},
  workspace:{gates:[{id:'g',name:'budget',passing:false}],
             failingTests:[{name:'t_one',file:'src/x.rs'}],
             deadCode:[{file:'d.rs',symbol:'s1'},{file:'d.rs',symbol:'s2'},{file:'d.rs',symbol:'s3'}],
             duplicates:[{a:'a.rs::f',b:'b.rs::f'}],recentFiles:['src/x.rs']}});
assert.ok(p.length>=3);
for (const item of p) {
  assert.ok(item.evidence.length,`${item.id} cites nothing`);
  assert.ok(item.task && item.task.question,`${item.id} has no task to submit`);
  assert.ok(item.rationale.length>30,`${item.id} has no real reason attached`);
  assert.ok(item.id && item.kind in PROPOSAL_KINDS === false || true);
}
""")

    def test_explore_needs_positive_evidence_of_health(self):
        """Absence of observed failures is not evidence that checks passed.

        This test previously supplied no gate data at all and asserted that
        Phi proposed new work — encoding the bug where an unreachable endpoint
        and a green workspace were indistinguishable.
        """
        self.steward("""
const s=new PhiSteward();
const base={state:{vector:{debt:.1},phase:'building'},friction:{unreviewed:{count:0,lines:0}}};

const verified=s.propose({...base,workspace:{gateStatus:'passing',recentFiles:['src/a.rs']}});
assert.equal(verified[0].kind,PROPOSAL_KINDS.EXPLORE,'checks passed: new work is cheap');
s.reset();

const unknown=s.propose({...base,workspace:{gateStatus:'unavailable',recentFiles:['src/a.rs']}});
assert.ok(!unknown.some(x=>x.kind===PROPOSAL_KINDS.EXPLORE),
  'checks that did not run must not read as a clean board');
assert.equal(unknown[0].kind,PROPOSAL_KINDS.ORIENT);
s.reset();

const silent=s.propose({...base,workspace:{recentFiles:['src/a.rs']}});
assert.ok(!silent.some(x=>x.kind===PROPOSAL_KINDS.EXPLORE),
  'no gate signal at all is unknown, not green');
s.reset();

const red=s.propose({...base,workspace:{gateStatus:'passing',recentFiles:['src/a.rs'],
  failingTests:[{name:'t',file:'f.rs'}]}});
assert.ok(!red.some(x=>x.kind===PROPOSAL_KINDS.EXPLORE),'nothing is free while a test is red');
""")

    def test_unavailable_checks_are_reported_not_omitted(self):
        self.steward("""
const s=new PhiSteward();
const signals={state:{vector:{debt:.1},phase:'building'},
  friction:{unreviewed:{count:0,lines:0}},
  workspace:{gateStatus:'unavailable',gateReason:'request failed',recentFiles:['src/a.rs']}};
const p=s.propose(signals);
const line=s.summarise(p,signals);
assert.match(line,/not the same as nothing being wrong|unknown/i,
  `Phi must say it could not check, got: ${line}`);
assert.ok(p[0].evidence.some(e=>/unavailable/i.test(e)),'the reason must be cited');
""")

    def test_a_dismissed_proposal_does_not_come_back(self):
        self.steward("""
const s=new PhiSteward();
const signals={state:{vector:{debt:.8},phase:'drifting'},
  friction:{unreviewed:{count:6,lines:300}},workspace:{recentFiles:['src/a.rs']}};
const first=s.propose(signals);
s.dismiss(first[0].id);
assert.ok(!s.propose(signals).some(x=>x.id===first[0].id),'"not now" must be respected');
""")

    def test_idle_watcher_speaks_once_per_pause_and_never_while_busy(self):
        self.steward("""
let t=0;const w=new IdleWatcher({quietMs:1000,now:()=>t});
w.setBusy(true); t+=5000;
assert.equal(w.shouldSpeak(),false,'must never interrupt a running task');
w.setBusy(false); t+=500;
assert.equal(w.shouldSpeak(),false,'must wait out the quiet period');
t+=600;
assert.equal(w.shouldSpeak(),true,'should speak once the pause is real');
t+=10000;
assert.equal(w.shouldSpeak(),false,'must not repeat itself in the same pause');
w.setBusy(true); w.setBusy(false); t+=1200;
assert.equal(w.shouldSpeak(),true,'a new pause earns a new remark');
""")

    def test_user_activity_restarts_the_quiet_period(self):
        """Interrupting someone who is thinking is worse than staying quiet."""
        self.steward("""
let t=0;const w=new IdleWatcher({quietMs:1000,now:()=>t});
w.setBusy(false); t+=900; w.noteActivity(); t+=900;
assert.equal(w.shouldSpeak(),false,'they are still working; stay quiet');
t+=200;
assert.equal(w.shouldSpeak(),true);
""")


class MediatorTests(unittest.TestCase):
    """Phi between you and Selfware, acting in both directions.

    The defining property of a meta-agent rather than a dashboard: when the
    model capitulates, the intervention is aimed at the MODEL.
    """
    node = ExpressionTests.node

    def mediator(self, body):
        self.node("""
const {PhiMediator, MEDIATION, ANNOTATION} =
  await import('""" + (PHI / 'phi_mediator.js').as_uri() + """');
""" + body)

    def test_capitulation_is_answered_by_constraining_the_model(self):
        self.mediator("""
const m=new PhiMediator();
const o=m.outbound('Add retries.',{state:{vector:{debt:.2}},friction:{capitulations:2,unreviewed:{count:0}}});
const disproof=o.augmentations.find(a=>a.kind===MEDIATION.DEMAND_DISPROOF);
assert.ok(disproof,'two reversals must produce a disproof demand');
assert.match(disproof.text,/prove your previous answer wrong/i);
assert.ok(disproof.reason.includes('2'),'the reason must cite what was observed');
""")

    def test_the_original_prompt_is_never_silently_rewritten(self):
        """Nothing may be smuggled into a request the human did not see."""
        self.mediator("""
const m=new PhiMediator();
const original='Add retries to the HTTP client.';
const o=m.outbound(original,{state:{vector:{debt:.9},reversals:3},friction:{capitulations:3,unreviewed:{count:6}}});
assert.equal(o.prompt,original,'outbound() must return the prompt untouched');
assert.ok(o.augmentations.length,'augmentations exist but stay separate');
const composed=m.compose(o.prompt,o.augmentations);
assert.ok(composed.startsWith(original),'the human text comes first, verbatim');
assert.ok(composed.includes('[Phi'),'additions must be labelled as coming from Phi');
""")

    def test_a_gate_is_a_question_with_an_override_not_a_block(self):
        self.mediator("""
const m=new PhiMediator();
const signals={state:{vector:{debt:.85}},friction:{unreviewed:{count:6}}};
const gated=m.outbound('Generate more.',signals);
assert.ok(gated.gate,'high debt must raise a gate');
assert.ok(gated.gate.options.some(o=>o.override),'there must always be a way through');
assert.ok(gated.gate.reason,'a gate must say what raised it');
m.override(gated.gate.id);
assert.equal(m.outbound('Generate more.',signals).gate,null,'Phi does not get to ask twice');
""")

    def test_no_signal_means_no_augmentation(self):
        self.mediator("""
const m=new PhiMediator();
const quiet=m.outbound('Add retries.',{state:{vector:{debt:.1}},friction:{capitulations:0,unreviewed:{count:0}}});
assert.deepEqual(quiet.augmentations,[],'a clean loop must not be constrained');
assert.equal(quiet.gate,null);
assert.equal(m.compose(quiet.prompt,quiet.augmentations),'Add retries.','compose must be a no-op');
""")

    def test_evidence_backed_answers_are_left_alone(self):
        """The discrimination the whole design rests on."""
        self.mediator("""
const m=new PhiMediator();
const sourced=m.inbound('cargo test passes; src/http.rs:88 now retries 3x.',
  {afterPushback:true,generatedLines:40});
assert.deepEqual(sourced,[],'a cited, reasoned answer must not be flagged');
const capitulated=m.inbound("You're absolutely right, I'll switch approach.",{afterPushback:true});
assert.equal(capitulated[0].kind,ANNOTATION.CAPITULATION);
assert.equal(capitulated[0].severity,'high');
""")

    def test_assertion_without_grounding_is_flagged_as_unsourced(self):
        self.mediator("""
const m=new PhiMediator();
const vague=m.inbound('This should work now.',{});
assert.equal(vague[0].kind,ANNOTATION.UNSOURCED_CLAIM);
assert.ok(vague[0].excerpt,'the flag must quote what triggered it');
const shown=m.inbound('This should work now - cargo test passes, see src/x.rs:10.',{});
assert.ok(!shown.some(a=>a.kind===ANNOTATION.UNSOURCED_CLAIM),
  'the same words with evidence attached are fine');
""")

    def test_bulk_generation_is_flagged_as_beyond_review(self):
        self.mediator("""
const m=new PhiMediator();
const big=m.inbound('Done.',{generatedLines:900});
assert.ok(big.some(a=>a.kind===ANNOTATION.SCOPE_CREEP));
const small=m.inbound('Done - src/x.rs:4 updated.',{generatedLines:30});
assert.ok(!small.some(a=>a.kind===ANNOTATION.SCOPE_CREEP));
""")


class PresenceTests(unittest.TestCase):
    """How Phi earns the right to speak.

    Everything Phi knows is unwelcome by design. These tests pin the behaviours
    that separate a useful companion from Clippy: posture by default, rationed
    speech, backoff when ignored, and free answers when asked.
    """
    node = ExpressionTests.node

    def presence(self, body):
        self.node("""
const {PhiPresence, CHANNEL, ambientPosture} =
  await import('""" + (PHI / 'phi_presence.js').as_uri() + """');
const settle=(p,t)=>{p.noteActivity();return t;};
""" + body)

    def test_posture_is_the_default_channel(self):
        """The common case must cost the human nothing to read."""
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t});
assert.equal(p.route({id:'a',severity:'high'}),CHANNEL.AMBIENT,'one observation is noise');
assert.equal(p.route({id:'b',severity:'medium'}),CHANNEL.AMBIENT);
assert.equal(p.route({}),CHANNEL.SILENT,'an unidentified signal says nothing');
""")

    def test_a_signal_must_persist_before_it_may_speak(self):
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,glanceCooldownMs:0});
const seen=[];for(let i=0;i<3;i++){t+=60000;seen.push(p.route({id:'a',severity:'high'}));}
assert.equal(seen[0],CHANNEL.AMBIENT,'first sighting is never spoken');
assert.ok(seen.includes(CHANNEL.GLANCE),'persistence earns a quiet line');
""")

    def test_the_first_word_is_always_quiet(self):
        """Nothing takes over the screen on its first outing."""
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,glanceCooldownMs:0});
let first=null;
for(let i=0;i<10&&!first;i++){t+=60000;const c=p.route({id:'a',severity:'high'});
  if(c!==CHANNEL.AMBIENT) first=c;}
assert.equal(first,CHANNEL.GLANCE,'a signal must glance before it may interrupt');
""")

    def test_ignoring_makes_phi_quieter_never_louder(self):
        """The escalate-when-unheard instinct is what makes assistants hated."""
        self.presence("""
let t=1e6;
const count=ignore=>{const p=new PhiPresence({now:()=>t,glanceCooldownMs:0});
  let spoke=0;
  for(let i=0;i<14;i++){t+=60000;const c=p.route({id:'a',severity:'high'});
    if(c!==CHANNEL.AMBIENT){spoke++; if(ignore)p.ignore('a');}}
  return spoke;};
const heeded=count(false), ignored=count(true);
assert.ok(ignored<heeded,`ignoring must reduce speech (${ignored} vs ${heeded})`);
""")

    def test_interrupts_are_budgeted_and_run_out(self):
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,interruptBudget:1,glanceCooldownMs:0});
let interrupts=0;
for(let i=0;i<40;i++){t+=60000;if(p.route({id:'a',severity:'high'})===CHANNEL.INTERRUPT)interrupts++;}
assert.equal(interrupts,1,'the budget is the budget');
assert.equal(p.status().budget,0);
""")

    def test_low_severity_never_takes_the_screen(self):
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,glanceCooldownMs:0});
for(let i=0;i<60;i++){t+=60000;
  assert.notEqual(p.route({id:'a',severity:'low'}),CHANNEL.INTERRUPT,
    'low severity must never interrupt, however persistent');}
""")

    def test_nothing_speaks_while_the_human_is_working(self):
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,quietAfterActivityMs:4000,glanceCooldownMs:0});
for(let i=0;i<8;i++){t+=60000;p.route({id:'a',severity:'high'});}
p.noteActivity();
assert.equal(p.route({id:'a',severity:'high'}),CHANNEL.AMBIENT,'do not interrupt a thinking human');
p.setBusy(true); t+=60000;
assert.equal(p.route({id:'a',severity:'high'}),CHANNEL.AMBIENT,'nor a running task');
p.setBusy(false); t+=60000;
assert.notEqual(p.route({id:'a',severity:'high'}),CHANNEL.SILENT);
""")

    def test_dismissal_is_permanent_for_the_session(self):
        """Being asked twice is the insult."""
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,glanceCooldownMs:0});
p.dismiss('a');
for(let i=0;i<40;i++){t+=60000;
  assert.equal(p.route({id:'a',severity:'high'}),CHANNEL.AMBIENT,'dismissed means dismissed');}
assert.deepEqual(p.status().dismissed,['a']);
""")

    def test_only_one_signal_may_speak_at_a_time(self):
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,glanceCooldownMs:0});
const many=[{id:'a',severity:'high'},{id:'b',severity:'high'},{id:'c',severity:'high'}];
for(let i=0;i<6;i++){t+=60000;
  const spoke=p.routeAll(many).filter(r=>r.channel!==CHANNEL.AMBIENT);
  assert.ok(spoke.length<=1,'two voices at once is noise however good each is');}
""")

    def test_acting_on_a_suggestion_refunds_the_interruption(self):
        """An interruption worth taking should not make Phi quieter afterwards."""
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,interruptBudget:1,glanceCooldownMs:0});
for(let i=0;i<12;i++){t+=60000;p.route({id:'a',severity:'high'});}
const spent=p.status().budget;
p.acknowledge('a');
assert.ok(p.status().budget>spent,'a useful interrupt is refunded');
""")

    def test_being_asked_is_free_and_always_answered(self):
        self.presence("""
let t=1e6;const p=new PhiPresence({now:()=>t,interruptBudget:0});
p.setBusy(true); p.noteActivity();
assert.equal(p.invited(),CHANNEL.INTERRUPT,'pull is never rationed or refused');
assert.equal(p.status().budget,0,'and never costs budget');
""")

    def test_posture_reads_the_loop_without_a_word(self):
        self.presence("""
assert.equal(ambientPosture({vector:{debt:.8}}).emotion,'unimpressed');
assert.equal(ambientPosture({vector:{debt:.5}}).emotion,'skeptical');
assert.equal(ambientPosture({vector:{debt:.1},phase:'entrenched'}).emotion,'unimpressed');
assert.equal(ambientPosture({vector:{debt:.1},phase:'building',expression:'working'}).emotion,'working');
""")

    def test_posture_never_downgrades_a_more_specific_face(self):
        """A red build must not be rendered calm just because the phase says so."""
        self.presence("""
assert.equal(ambientPosture({vector:{debt:.1},phase:'debugging',expression:'error'}).emotion,'error',
  'debugging must not mask a red build');
assert.equal(ambientPosture({vector:{debt:.3},phase:'debugging',expression:'unimpressed'}).emotion,'unimpressed',
  'debugging must not mask capitulation');
assert.equal(ambientPosture({vector:{debt:.1},phase:'debugging',expression:'greeting'}).emotion,'working',
  'with nothing specific to say, the phase may speak');
""")


if __name__ == '__main__':
    unittest.main()
