#!/usr/bin/env python3
"""Browser trajectory tests for Phi's SVG rig (requires Playwright and Chrome).

These exercise the real SVG geometry/CTM, not a mocked DOM. Run with:
    python3 -m unittest discover -s scripts/tests -p test_phi_rig.py -v
"""
from functools import partial
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread
import unittest

try:
    from playwright.sync_api import sync_playwright
except ImportError:
    sync_playwright = None

PHI = Path(__file__).resolve().parents[2] / "src/evolve/web/phi"


class RigHandler(SimpleHTTPRequestHandler):
    def do_GET(self):
        if self.path == "/rig-test.html":
            body = b'''<!doctype html><html><head><meta charset="utf-8">
              <link rel="stylesheet" href="/style.css"></head>
              <body style="margin:0;background:#090f1e"><button id="underlying"
              style="position:fixed;inset:0;width:100vw;height:100vh">Editor</button>
              <script type="module">
                import { PhiMascotRig, VISEMES } from '/phi_rig.js';
                window.PhiMascotRig=PhiMascotRig;window.VISEMES=VISEMES;
                window.ready=true;
              </script></body></html>'''
            self.send_response(200)
            self.send_header("Content-Type", "text/html; charset=utf-8")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
        else:
            super().do_GET()

    def log_message(self, *args):
        pass


@unittest.skipUnless(sync_playwright, "Playwright is required for real SVG trajectory tests")
class PhiRigTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), partial(RigHandler, directory=str(PHI)))
        cls.thread = Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.playwright = sync_playwright().start()
        try:
            cls.browser = cls.playwright.chromium.launch(channel="chrome", headless=True)
        except Exception:
            try:
                cls.browser = cls.playwright.chromium.launch(headless=True)
            except Exception:
                cls.playwright.stop()
                cls.server.shutdown()
                cls.server.server_close()
                raise

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=2)

    def setUp(self):
        self.page = self.browser.new_page(viewport={"width": 1280, "height": 800}, device_scale_factor=2)
        self.errors = []
        self.page.on("pageerror", lambda error: self.errors.append(str(error)))
        self.page.goto(f"http://127.0.0.1:{self.server.server_port}/rig-test.html")
        self.page.wait_for_function("window.ready === true")

    def tearDown(self):
        self.page.close()
        self.assertEqual(self.errors, [], "Browser must not produce JavaScript errors")

    def test_flight_is_frame_rate_independent_and_interruptible(self):
        result = self.page.evaluate("""() => {
          const travel = hz => {
            const rig = new PhiMascotRig(document.body, {initialX:180, initialY:250});
            rig.flyTo(900, 450);
            for(let i=0;i<hz/2;i++) rig.update(1/hz);
            const midpoint={x:rig.x,y:rig.y,vx:rig.vx};
            rig.flyTo(350, 280);
            const before=rig.x;
            rig.update(1/hz);
            const first=rig.x;
            for(let i=1;i<hz;i++) rig.update(1/hz);
            const final={x:rig.x,y:rig.y,targetX:rig.targetX};
            rig.destroy();
            return {midpoint,before,first,final};
          };
          return [travel(30), travel(60), travel(120)];
        }""")
        for trajectory in result:
            self.assertGreater(trajectory["midpoint"]["x"], 700)
            self.assertGreater(trajectory["midpoint"]["vx"], 0)
            self.assertLess(abs(trajectory["first"] - trajectory["before"]), 35)
            self.assertLess(abs(trajectory["final"]["x"] - trajectory["final"]["targetX"]), 2)
        for trajectory in result[1:]:
            for axis in ("x", "y", "vx"):
                self.assertAlmostEqual(trajectory["midpoint"][axis], result[0]["midpoint"][axis], places=7)
            self.assertAlmostEqual(trajectory["final"]["x"], result[0]["final"]["x"], places=7)

    def test_all_ten_mouths_morph_with_openness_and_no_history_leaks(self):
        result = self.page.evaluate("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:400,initialY:300});
          const shape=()=>({d:rig.mouthCavity.getAttribute('d'),
            tongue:rig.mouthTongue.getAttribute('d'), teeth:rig.mouthTeeth.getAttribute('d'),
            opacity:rig.mouthTongue.style.opacity});
          const rest=shape();
          rig.setViseme('ai',1); const queued=shape(); rig.update(.02);const mid=shape();
          for(let i=0;i<30;i++)rig.update(.02);const open=shape();
          rig.setViseme('o',1);const beforeInterrupt=shape();rig.update(.02);const interrupted=shape();
          const settle = name => {rig.setViseme(name,1);for(let i=0;i<40;i++)rig.update(.025);return shape();};
          const poses=Object.values(VISEMES).map(settle);
          settle('ai');const eAfterAi=settle('e');settle('u');const eAfterU=settle('e');
          rig.setViseme('ai',0);for(let i=0;i<40;i++)rig.update(.025);
          const closedPose=[...rig.mouthPose];
          const tails=rig.tailElements.length;
          rig.destroy();
          return {rest,queued,mid,open,beforeInterrupt,interrupted,poses,eAfterAi,eAfterU,closedPose,tails};
        }""")
        self.assertEqual(result["rest"], result["queued"], "setViseme must queue, not snap")
        self.assertNotEqual(result["mid"]["d"], result["rest"]["d"])
        self.assertNotEqual(result["mid"]["d"], result["open"]["d"])
        self.assertEqual(result["beforeInterrupt"], result["open"])
        self.assertNotEqual(result["interrupted"]["d"], result["open"]["d"])
        self.assertEqual(len({pose["d"] for pose in result["poses"]}), 10)
        self.assertEqual(result["tails"], 9)
        # Numeric convergence permits subpixel float differences, but neither a
        # previous tongue shape nor an old teeth bar may remain visible.
        for key in ("tongue", "teeth"):
            import re
            first = [float(x) for x in re.findall(r"-?\d+(?:\.\d+)?(?:e[+-]?\d+)?", result["eAfterAi"][key])]
            second = [float(x) for x in re.findall(r"-?\d+(?:\.\d+)?(?:e[+-]?\d+)?", result["eAfterU"][key])]
            self.assertEqual(len(first), len(second))
            for a, b in zip(first, second):
                self.assertAlmostEqual(a, b, places=7)
        self.assertEqual(result["eAfterAi"]["opacity"], result["eAfterU"]["opacity"])
        self.assertAlmostEqual(result["closedPose"][2], 103, places=7)
        self.assertAlmostEqual(result["closedPose"][3], 103, places=7)
        self.assertAlmostEqual(result["closedPose"][5], 0, places=7)

    def test_laser_tracks_actual_monocle_ctm_during_banked_flight(self):
        result = self.page.evaluate("""() => {
          const rig=new PhiMascotRig(document.body,{width:300,height:300,initialX:200,initialY:280});
          const moves=[],lines=[];
          const move=rig.laserCtx.moveTo.bind(rig.laserCtx),line=rig.laserCtx.lineTo.bind(rig.laserCtx);
          rig.laserCtx.moveTo=(x,y)=>{moves.push({x,y});move(x,y)};
          rig.laserCtx.lineTo=(x,y)=>{lines.push({x,y});line(x,y)};
          rig.flyTo(850,420);rig.fireLaser(1100,100);
          const errors=[];
          for(let i=0;i<50;i++){
            rig.update(1/60);
            const actual=new DOMPoint(120,80).matrixTransform(rig.headGroup.getScreenCTM());
            const drawn=moves.at(-1);errors.push(Math.hypot(actual.x-drawn.x,actual.y-drawn.y));
          }
          const result={errors,last:lines.at(-1),canvasWidth:rig.laserCanvas.width,
            scale:rig.scaleX,wrapperTransform:rig.wrapper.style.transform,bank:rig.rotation};
          rig.stopLaser();result.stopped=!rig.laserActive&&rig.laserTarget===null;
          rig.destroy();return result;
        }""")
        self.assertLess(max(result["errors"]), 0.001)
        self.assertEqual(result["last"], {"x": 1100, "y": 100})
        self.assertEqual(result["canvasWidth"], 2560)
        self.assertGreater(result["scale"], 0.9)
        self.assertNotIn("scaleX", result["wrapperTransform"])
        self.assertNotIn("rotate", result["wrapperTransform"])
        self.assertNotEqual(result["bank"], 0)
        self.assertTrue(result["stopped"])

    def test_reduced_motion_static_god_mode_and_preference_changes(self):
        self.page.emulate_media(reduced_motion="reduce")
        result = self.page.evaluate("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:250,initialY:250,godMode:true});
          rig.flyTo(600,350);rig.fireLaser(1000,500);rig.setViseme('ai',1);rig.update(0);
          const snapshot=()=>JSON.stringify({x:rig.x,y:rig.y,transform:rig.wrapper.style.transform,
            head:rig.headGroup.getAttribute('transform'),tails:rig.tailElements.map(x=>x.getAttribute('d')),
            rings:rig.godRings.getAttribute('transform'),mouth:rig.mouthCavity.getAttribute('d')});
          const before=snapshot();for(let i=0;i<300;i++)rig.update(1/60);
          const after=snapshot();window.rig=rig;
          return {before,after,particles:rig.particles.length,mode:rig.godMode,
            visible:rig.godRings.style.opacity,arrived:rig.x===rig.targetX&&rig.y===rig.targetY};
        }""")
        self.assertEqual(result["before"], result["after"])
        self.assertEqual(result["particles"], 0)
        self.assertTrue(result["mode"])
        self.assertEqual(result["visible"], "1")
        self.assertTrue(result["arrived"])
        self.page.emulate_media(reduced_motion="no-preference")
        self.page.wait_for_function("rig.reducedMotion === false")
        self.assertGreater(self.page.evaluate("() => {rig.update(.1);return rig.time}"), 0)
        self.page.evaluate("rig.destroy()")

    def test_selected_god_mode_survives_attention_changes_until_disabled(self):
        result = self.page.evaluate("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:250,initialY:250});
          rig.setGodMode(true);
          const modes=['focused','analytical','curious','alert'].map(emotion=>{
            rig.setEmotion(emotion);rig.update(0);
            return {enabled:rig.godMode,rings:rig.godRings.style.opacity};
          });
          rig.setGodMode(false);rig.update(0);
          const disabled={enabled:rig.godMode,rings:rig.godRings.style.opacity};
          rig.destroy();return {modes,disabled};
        }""")
        self.assertEqual(result["modes"], [{"enabled": True, "rings": "1"}] * 4)
        self.assertEqual(result["disabled"], {"enabled": False, "rings": "0"})

    def test_mobile_layout_including_speech_stays_visible(self):
        self.page.set_viewport_size({"width": 390, "height": 844})
        result = self.page.evaluate("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:1000,initialY:-300,reducedMotion:true});
          rig.setSpeechText('A long code explanation '.repeat(60));rig.flyTo(900,900);
          rig.setEmotion('god_mode');rig.update(0);
          const bounds=rig.getBounds(),layout=rig.getLayoutSize();
          const underlying=document.elementFromPoint(5,5)?.id;
          const result={bounds,layout,underlying,tails:rig.tailElements.length};
          rig.destroy();return result;
        }""")
        bounds = result["bounds"]
        self.assertGreaterEqual(bounds["left"], 0)
        self.assertGreaterEqual(bounds["top"], 0)
        self.assertLessEqual(bounds["right"], 390)
        self.assertLessEqual(bounds["bottom"], 844)
        self.assertGreater(result["layout"]["topInset"], 100)
        self.assertEqual(result["underlying"], "underlying")
        self.assertEqual(result["tails"], 9)

    def test_destroy_releases_owned_dom_listeners_and_never_owns_raf(self):
        result = self.page.evaluate("""() => {
          let requests=0;const originalRAF=window.requestAnimationFrame;
          window.requestAnimationFrame=()=>{requests++;return 1};
          const host=document.createElement('section');document.body.append(host);
          const rig=new PhiMascotRig(host,{initialX:250,initialY:250});
          const second=new PhiMascotRig(document.body,{initialX:650,initialY:250});
          const ids=[...document.querySelectorAll('svg [id]')].map(x=>x.id);
          const inside=host.contains(rig.wrapper)&&host.contains(rig.laserCanvas);
          for(let i=0;i<30;i++)rig.update(1/60);
          rig.destroy();rig.destroy();const oldX=rig.x;
          window.dispatchEvent(new Event('resize'));
          rig.flyTo(900,600);rig.setViseme('ai');rig.update(1);rig.setSpeechText('stale');
          const result={inside,requests,remaining:host.children.length,listeners:rig.listeners.length,
            unchanged:oldX===rig.x,other:second.wrapper.isConnected&&second.laserCanvas.isConnected,
            unique:ids.length===new Set(ids).size};
          second.destroy();host.remove();window.requestAnimationFrame=originalRAF;return result;
        }""")
        self.assertTrue(result["inside"])
        self.assertEqual(result["requests"], 0)
        self.assertEqual(result["remaining"], 0)
        self.assertEqual(result["listeners"], 0)
        self.assertTrue(result["unchanged"])
        self.assertTrue(result["other"])
        self.assertTrue(result["unique"])

    def test_pointer_drag_and_cancel_release_capture_without_blocking_empty_space(self):
        point = self.page.evaluate("""() => {
          window.rig=new PhiMascotRig(document.body,{initialX:400,initialY:300,reducedMotion:true});
          const point=rig.screenPoint(100,130);
          const empty=new DOMPoint(190,190).matrixTransform(rig.svg.getScreenCTM());
          return {x:point.x,y:point.y,emptyHit:document.elementFromPoint(empty.x,empty.y)?.id};
        }""")
        self.assertEqual(point["emptyHit"], "underlying")
        self.page.mouse.move(point["x"], point["y"])
        self.page.mouse.down()
        self.assertTrue(self.page.evaluate("!!rig.drag && rig.svg.hasPointerCapture(rig.drag.id)"))
        self.page.mouse.move(point["x"] + 70, point["y"] + 45)
        self.assertAlmostEqual(self.page.evaluate("rig.x"), 470, places=5)
        self.assertAlmostEqual(self.page.evaluate("rig.y"), 345, places=5)
        self.page.evaluate("window.dispatchEvent(new Event('blur'))")
        self.assertIsNone(self.page.evaluate("rig.drag"))
        self.page.mouse.move(point["x"] + 130, point["y"] + 90)
        self.assertAlmostEqual(self.page.evaluate("rig.x"), 470, places=5)
        self.page.mouse.up()
        self.page.evaluate("rig.destroy()")

    def test_invalid_inputs_and_long_frame_do_not_poison_rendering(self):
        result = self.page.evaluate("""() => {
          const rig=new PhiMascotRig(document.body,{initialX:400,initialY:300});
          rig.flyTo(NaN,Infinity);rig.gazeAt(Infinity,NaN);rig.fireLaser(NaN,0);
          rig.setAudioVolume(NaN);rig.setViseme('unknown',Infinity);
          rig.update(NaN);rig.update(-1);rig.update(3600);
          const result={finite:[rig.x,rig.y,rig.vx,rig.vy,rig.time,rig.audioVolume].every(Number.isFinite),
            bounded:rig.time<=.1,laser:rig.laserActive,viseme:rig.currentViseme,
            silent:[...rig.equalizerBars].every(x=>x.style.opacity==='0.25'),
            noNaN:!rig.svg.outerHTML.includes('NaN')&&!rig.svg.outerHTML.includes('Infinity')};
          rig.destroy();return result;
        }""")
        self.assertTrue(result["finite"])
        self.assertTrue(result["bounded"])
        self.assertFalse(result["laser"])
        self.assertEqual(result["viseme"], "rest")
        self.assertTrue(result["silent"])
        self.assertTrue(result["noNaN"])


if __name__ == "__main__":
    unittest.main()
