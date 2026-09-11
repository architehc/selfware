"""Execute the production semantic IDE reporter with controlled time and transport."""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

MODULE = Path(__file__).resolve().parents[2] / 'src/evolve/web/phi_friction_client.js'


class FrictionClientTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required for the production reporter tests')
        setup = f"""
import assert from 'node:assert/strict';
import {{ PhiIdeFrictionClient }} from {json.dumps(MODULE.as_uri())};
let now=0,visible=true,token='fixture-session',preview=true;
const posts=[],statuses=[];
const timers={{setInterval:()=>1,clearInterval:()=>{{}},setTimeout,clearTimeout}};
const c=new PhiIdeFrictionClient({{now:()=>now,clock:()=>Date.UTC(2026,8,10,4),visible:()=>visible,
getToken:()=>token,timers,onStatus:s=>statuses.push(s),fetch:async(path,options)=>{{const body=JSON.parse(options.body);posts.push({{path,options,body}});return new Response(JSON.stringify({{accepted:body.events.length,cursor:1}}),{{headers:{{'content-type':'application/json'}}}})}}}});
const tick=(ms=1000)=>{{now+=ms;c.tick(false)}};
const review=()=>c.beginReview({{generationId:'run-1',addedLines:400,digest:'a'.repeat(64),isVisible:()=>preview}});
"""
        result = subprocess.run(['node', '--input-type=module', '-'], input=setup + body,
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_review_measures_visible_focused_active_intervals(self):
        self.node("""
review();c.reviewActivity();tick();tick();preview=false;tick();preview=true;visible=false;tick();
visible=true;tick(20000);c.reviewActivity();tick();c.endReview('rejected');
const e=c.queue.find(e=>e.kind==='review_closed');assert.equal(e.data.active_review_ms,3000);
assert.equal(e.data.added_lines,400);assert.equal(e.generation_id,'run-1');
c.destroy();
""")

    def test_rejection_is_explicit_and_duplicate_click_does_not_count(self):
        self.node("""
review();assert(c.endReview('rejected'));assert.equal(c.endReview('rejected'),false);
assert.equal(c.queue.filter(e=>e.kind==='review_closed').length,1);
assert(c.endReview('accepted'));assert.equal(c.queue.at(-1).data.decision,'accepted');
assert.equal(c.review,null);c.destroy();
""")

    def test_closing_or_replacing_preview_is_not_rejection(self):
        self.node("""
review();tick();review();assert.equal(c.queue.at(-1).data.decision,'closed');
assert.equal(c.endReview('rejected','wrong-run'),false);
assert.equal(c.queue.length,1);c.destroy();
""")

    def test_undo_never_invents_generation_attribution_or_transmits_content(self):
        self.node("""
review();c.undo();const e=c.queue.at(-1);assert.deepEqual(e.data,{count:1});
assert.equal(e.kind,'editor_undo');assert.equal(e.generation_id,undefined);
assert.deepEqual(Object.keys(e).sort(),['data','id','kind']);c.destroy();
""")

    def test_idle_and_hidden_time_never_accumulate_as_activity(self):
        self.node("""
c.activity();for(let i=0;i<90;i++)tick();
assert.equal(c.queue.filter(e=>e.kind==='activity').reduce((s,e)=>s+e.data.active_ms,0),30000);
const before=c.activeMs;visible=false;for(let i=0;i<30;i++){c.activity();tick()}
assert.equal(c.activeMs,before);c.destroy();
""")

    def test_off_and_destroy_abort_and_clear_transient_events(self):
        self.node("""
c.undo();assert.equal(c.queue.length,1);c.setEnabled(false);c.undo();assert.equal(c.queue.length,0);
c.setEnabled(true);c.undo();assert.equal(c.queue.length,1);c.destroy();c.undo();
assert.equal(c.queue.length,0);await c.flush();assert.equal(posts.length,0);
""")

    def test_transport_is_authenticated_same_origin_and_bounded(self):
        self.node("""
for(let i=0;i<100;i++)c.undo();assert.equal(c.queue.length,32);await c.flush();
assert.equal(posts.length,1);const p=posts[0];assert.equal(p.path,'/api/friction/events');
assert.equal(p.options.headers['x-selfware-session'],'fixture-session');
assert.equal(p.options.redirect,'error');assert.equal(p.options.credentials,'same-origin');
assert.equal(p.body.events.length,32);assert.equal(statuses.at(-1),'connected');
assert.equal(c.queue.length,0);c.destroy();
""")

    def test_unavailable_feed_is_not_replayed_as_new_friction(self):
        self.node("""
c.fetcher=async()=>{throw Error('offline')};c.undo();await c.flush();
assert.equal(statuses.at(-1),'unavailable');assert.equal(c.queue.length,0);
token=null;c.undo();assert.equal(c.queue.length,0);c.destroy();
""")

    def test_invalid_diff_metadata_is_not_a_measurement(self):
        self.node("""
for(const count of [-1,NaN,Infinity,1.5,1000001,'400']){
c.beginReview({generationId:'run-1',addedLines:count,digest:'a'.repeat(64),isVisible:()=>true});
assert.equal(c.review,null)}assert.equal(c.queue.length,0);c.destroy();
""")

    def test_rendering_a_diff_does_not_invent_review_or_editor_activity(self):
        self.node("""
review();for(let i=0;i<20;i++)tick();assert.equal(c.activeMs,0);assert.equal(c.review.activeMs,0);
c.activity();tick();assert.equal(c.activeMs,1000);assert.equal(c.review.activeMs,0);
c.reviewActivity();tick();assert.equal(c.review.activeMs,1000);c.destroy();
""")

    def test_disabled_late_response_cannot_publish_connected(self):
        self.node("""
let release;c.fetcher=()=>new Promise(r=>{release=r});c.undo();const pending=c.flush();
c.setEnabled(false);release(new Response(JSON.stringify({accepted:1,cursor:1}),{headers:{'content-type':'application/json'}}));
await pending;assert.deepEqual(statuses,[]);c.destroy();
""")

    def test_html_or_invalid_ack_is_not_success(self):
        self.node("""
for(const response of [new Response('<html>fallback</html>'),new Response('{}',{headers:{'content-type':'application/json'}}),
new Response('x'.repeat(1025),{headers:{'content-type':'application/json'}})]){
c.fetcher=async()=>response;c.undo();await c.flush();assert.equal(statuses.at(-1),'unavailable');
}c.destroy();
""")

    def test_review_time_starts_at_engagement_and_stops_on_other_editing(self):
        self.node("""
review();now=900;c.reviewActivity();tick(100);assert.equal(c.review.activeMs,100);
c.activity();tick();assert.equal(c.review.activeMs,100);c.destroy();
""")


if __name__ == '__main__':
    unittest.main()
