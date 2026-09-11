#!/usr/bin/env python3
"""Actual mobile workspace: source and captions remain clear of the settled rig."""
import importlib.util
import json
import os
from pathlib import Path
import unittest

_spec = importlib.util.spec_from_file_location('_phi_mobile_workspace_fixture',
                                              Path(__file__).with_name('test_phi_workspace.py'))
fixture = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(fixture)


def overlap(first, second):
    return max(0, min(first['right'], second['right']) - max(first['left'], second['left'])) * max(
        0, min(first['bottom'], second['bottom']) - max(first['top'], second['top']))


@unittest.skipUnless(fixture.sync_playwright, 'Playwright is required for actual mobile layout checks')
class PhiMobileFocusTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        fixture.PhiWorkspaceTests.setUpClass.__func__(cls)

    @classmethod
    def tearDownClass(cls):
        fixture.PhiWorkspaceTests.tearDownClass.__func__(cls)

    def setUp(self):
        fixture.PhiWorkspaceTests.setUp(self)

    def tearDown(self):
        fixture.PhiWorkspaceTests.tearDown(self)

    def test_selected_reading_keeps_source_and_narration_clear_at_390px(self):
        self.page.set_viewport_size({'width': 390, 'height': 1000})
        self.page.locator('#speech-rate').select_option('0.8')
        self.page.evaluate('''() => {
          const code=document.querySelector('[data-line="2"] .line-code');
          const walker=document.createTreeWalker(code,NodeFilter.SHOW_TEXT);
          const phrase='Hello from your local workspace';let node;
          while ((node=walker.nextNode())) {
            const start=node.textContent.indexOf(phrase);
            if(start<0)continue;
            const range=document.createRange();range.setStart(node,start);range.setEnd(node,start+phrase.length);
            getSelection().removeAllRanges();getSelection().addRange(range);return;
          }
          throw new Error('Fixture phrase was not found in the actual rendered source');
        }''')
        self.page.locator('#btn-read-selection').click()
        self.page.wait_for_function('''phiApp.focus.active?.target?.descriptor?.line===2 && phiApp.focus.parkingPosition &&
          Math.hypot(phiApp.rig.x-phiApp.rig.targetX,phiApp.rig.y-phiApp.rig.targetY)<0.5''')
        frames = self.page.evaluate('''async () => {
          const frames=[];
          for(let i=0;i<60;i++) {
            await new Promise(resolve=>requestAnimationFrame(resolve));
            const source=phiApp.focus.active&&phiApp.focus.measure(phiApp.focus.active.target);
            frames.push({time:performance.now(),rig:phiApp.rig.getBounds(),
              source:source?.visible?source.rect:null,
              narration:document.querySelector('.narration-panel').getBoundingClientRect().toJSON(),
              caption:document.getElementById('transcript').getBoundingClientRect().toJSON(),
              park:{...phiApp.focus.parkingPosition}});
          }
          return frames;
        }''')
        output = os.environ.get('PHI_FOCUS_MOTION_ARTIFACTS')
        if output:
            destination = Path(output)
            destination.mkdir(parents=True, exist_ok=True)
            (destination / 'caption-clearance-trajectory.json').write_text(json.dumps({
                'backend': 'fixture workspace; production browser modules; silent reading',
                'viewport': {'width': 390, 'height': 1000}, 'frames': frames,
                'browser_errors': self.errors}, indent=2))
            self.page.screenshot(path=str(destination / '10-caption-clearance-390.png'))
        self.assertEqual(len(frames), 60)
        self.assertEqual(self.page.locator('#transcript').inner_text(), 'Hello from your local workspace')
        self.assertFalse(self.page.evaluate('document.documentElement.scrollWidth>innerWidth'))
        self.assertTrue(self.page.locator('.narration-panel').is_visible())
        for frame in frames:
            self.assertIsNotNone(frame['source'], frame)
            self.assertGreater(frame['caption']['height'], 0)
            self.assertGreaterEqual(frame['caption']['top'], 0)
            self.assertLessEqual(frame['caption']['bottom'], 1000)
            self.assertEqual(overlap(frame['rig'], frame['source']), 0, frame)
            self.assertEqual(overlap(frame['rig'], frame['narration']), 0, frame)
            self.assertEqual(overlap(frame['rig'], frame['caption']), 0, frame)
            self.assertGreaterEqual(frame['rig']['left'], 0, frame)
            self.assertGreaterEqual(frame['rig']['top'], 0, frame)
            self.assertLessEqual(frame['rig']['right'], 390, frame)
            self.assertLessEqual(frame['rig']['bottom'], 1000, frame)
        # Do not solve caption overlap by bouncing between clear slots on words.
        self.assertLess(max(frame['park']['parkY'] for frame in frames) -
                        min(frame['park']['parkY'] for frame in frames), 4)
        self.page.locator('#btn-stop-mission').click()


if __name__ == '__main__':
    unittest.main()
