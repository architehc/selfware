"""Live Phi activity contracts. HTTP fixtures test the shipped browser, not inference."""
import json
import pathlib
import shutil
import subprocess
import unittest

import test_phi_workspace as fixture

ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE = ROOT / 'src/evolve/web/phi/phi_activity.js'

JS_FIXTURE = """
const evidence = {outstanding:0,unreviewed_lines:0,untested_lines:0,unknown_size_obligations:0,
  unattributed_mutations:0,possible_unrecorded_mutations:0,observed_runs:1,passed_runs:1,failed_runs:0,unknown_runs:0};
const row = (id='a', changes={}) => ({agent_id:id,task_id:'task_'+id,session_id:'session',status:'available',
  phase:'running',recorded_at_ms:1000,age_ms:100,evidence:{...evidence},...changes});
const capture = (agents=[row()], changes={}) => ({status:'available',reason:null,observed_at_ms:1100,
  agents,truncated:false,...changes});
"""


class ActivityContracts(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required')
        source = ("import assert from 'node:assert/strict';\n"
                  f"import {{parseActivity, activityMood, PhiActivity}} from {json.dumps(MODULE.as_uri())};\n"
                  + JS_FIXTURE + body)
        result = subprocess.run(['node', '--input-type=module', '-'], input=source,
                                text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_sixteen_agent_states_and_counts_are_preserved(self):
        self.node("""
const result=parseActivity(capture(Array.from({length:16},(_,i)=>row('agent_'+i))));
assert.equal(result.agents.length,16);assert.equal(activityMood(result),'working');
assert.equal(new Set(result.agents.map(r=>r.agent_id)).size,16);
assert.equal(result.agents.reduce((n,r)=>n+r.evidence.passed_runs,0),16);
""")

    def test_task_completion_and_passing_runs_never_claim_full_verification(self):
        self.node("""
assert.equal(activityMood(parseActivity(capture([row('a',{phase:'completed'})]))),'idle');
for(const phase of ['running','completed','failed','partial','abandoned']) {
  assert.ok(!['success','spark'].includes(activityMood(parseActivity(capture([row('a',{phase})])))));
}
""")

    def test_failed_unknown_missing_and_stale_are_distinct(self):
        self.node("""
assert.equal(activityMood(parseActivity(capture([row('a',{phase:'failed'})]))),'error');
assert.equal(activityMood(parseActivity(capture([row('a',{evidence:{...evidence,passed_runs:0,failed_runs:1}})]))),'error');
assert.equal(activityMood(parseActivity(capture([row('a',{evidence:{...evidence,passed_runs:0,unknown_runs:1}})]))),'guard');
assert.equal(activityMood(parseActivity(capture([row('a',{status:'incomplete',evidence:null})],{status:'incomplete'}))),'guard');
assert.equal(activityMood(parseActivity(capture([],{status:'unavailable'}))),null);
const stale=parseActivity(capture([row('a',{status:'stale',phase:'failed'})],{status:'incomplete'}));
assert.notEqual(activityMood(stale),'error','an expired failure is not a current failure');
const historicalFailurePassed=parseActivity(capture([row('a',{phase:'completed',evidence:{...evidence,observed_runs:2,passed_runs:1,failed_runs:1,latest_run:'passed'}})]));
assert.notEqual(activityMood(historicalFailurePassed),'error','a historical failure followed by a passing run is not an error');
const historicalPassFailed=parseActivity(capture([row('a',{phase:'running',evidence:{...evidence,observed_runs:2,passed_runs:1,failed_runs:1,latest_run:'failed'}})]));
assert.equal(activityMood(historicalPassFailed),'error','a latest run failure is an error');
""")

    def test_backend_available_envelope_retains_stale_and_incomplete_task_rows(self):
        self.node("""
const result=parseActivity(capture([
  row('same',{task_id:'first',status:'stale',phase:'completed'}),
  row('same',{task_id:'second',status:'incomplete',evidence:null})]));
assert.equal(result.status,'available');assert.equal(result.agents.length,2);
assert.equal(result.agents[0].status,'stale');assert.equal(result.agents[1].evidence,null);
assert.equal(activityMood(result),'guard');
""")

    def test_malformed_evidence_never_becomes_zero_or_green(self):
        self.node("""
for(const change of [{status:'unknown'},{observed_at_ms:null},{agents:Array(65).fill(row())},
  {agents:[row(),row()]},{agents:[row('<img>')]},{agents:[row('a',{evidence:{}})]},
  {agents:[row('a',{evidence:{...evidence,failed_runs:-1}})]},
  {agents:[row('a',{evidence:{...evidence,observed_runs:12}})]},{agents:[]}]) {
  assert.throws(()=>parseActivity(capture(undefined,change)));
}
""")

    def test_late_response_cannot_repopulate_disconnected_workspace(self):
        self.node("""
let release;
const feed=new PhiActivity({getToken:()=> 'session',fetch:()=>new Promise(resolve=>release=resolve)});
feed.connect(); feed.disconnect('example_mode');
release(new Response(JSON.stringify(capture()),{headers:{'content-type':'application/json'}}));
await new Promise(resolve=>setTimeout(resolve,20));
assert.equal(feed.snapshot.reason,'example_mode'); assert.deepEqual(feed.snapshot.agents,[]);
feed.destroy();
""")

    def test_stalled_body_and_oversized_response_are_unavailable(self):
        self.node("""
const feed=new PhiActivity({getToken:()=> 'session',timeoutMs:15,pollIntervalMs:10000,
fetch:async()=>new Response(new ReadableStream({start(){}}),{headers:{'content-type':'application/json'}})});
feed.connect();await new Promise(resolve=>setTimeout(resolve,50));
assert.equal(feed.snapshot.status,'unavailable');assert.equal(feed.snapshot.reason,'request_failed');feed.destroy();
const huge=new PhiActivity({getToken:()=> 'session',pollIntervalMs:10000,
fetch:async()=>new Response('x'.repeat(262145),{headers:{'content-type':'application/json'}})});
huge.connect();await new Promise(resolve=>setTimeout(resolve,30));
assert.equal(huge.snapshot.reason,'request_failed');huge.destroy();
""")


@unittest.skipUnless(fixture.sync_playwright, 'Playwright is required')
class ActivityBrowserContracts(unittest.TestCase):
    setUpClass = classmethod(fixture.PhiWorkspaceTests.setUpClass.__func__)
    tearDownClass = classmethod(fixture.PhiWorkspaceTests.tearDownClass.__func__)
    setUp = fixture.PhiWorkspaceTests.setUp
    tearDown = fixture.PhiWorkspaceTests.tearDown

    def observe(self, expression):
        self.page.evaluate("""async expression => {
          const payload=Function(%s+'return '+expression)();
          phiApp.activity.disconnect();
          phiApp.activity.fetch=async (url,options)=>{
            if(url!='/api/phi/activity'||options.headers['x-selfware-session']!=='fixture-session') throw Error('bad request');
            return new Response(JSON.stringify(payload),{headers:{'content-type':'application/json'}});
          };
          phiApp.activity.connect();
        }""" % json.dumps(JS_FIXTURE), expression)
        self.page.wait_for_function("phiApp.activity.snapshot.status !== 'unavailable'")

    def test_sixteen_real_rows_and_readable_mobile_layout(self):
        self.observe("capture(Array.from({length:16},(_,i)=>row('agent_'+i)))")
        self.assertEqual(self.page.locator('.phi-activity-agent').count(), 16)
        self.assertIn('16 observed running', self.page.locator('.phi-activity-status').inner_text())
        self.assertIn('1 passed · 0 failed · 0 unknown', self.page.locator('.phi-activity-evidence').nth(1).inner_text())
        self.assertTrue(self.page.evaluate("""() => {
          const list=document.querySelector('.phi-activity-agents');list.scrollTop=100;list.focus();
          phiApp.activity.publish(phiApp.activity.snapshot);
          const updated=document.querySelector('.phi-activity-agents');
          return updated.scrollTop===100 && document.activeElement===updated;
        }"""))
        self.page.set_viewport_size({'width': 390, 'height': 844})
        self.assertTrue(self.page.evaluate('document.documentElement.scrollWidth <= innerWidth'))

    def test_runtime_pose_preserves_narration_and_heuristic_debt(self):
        self.page.evaluate("phiApp.orchestrator.isRunning=true; phiApp.rig.setEmotion('curious'); window.beforeDebt=phiApp.state.snapshot().vector.debt")
        self.observe("capture([row('a',{phase:'failed'})])")
        self.assertTrue(self.page.evaluate("phiApp.activityEmotion==='error'"))
        # Invoke the pose boundary with a spy so this assertion is independent
        # of the rig's spring timing and expression aliases.
        self.assertEqual(self.page.evaluate("""() => {
          const calls=[];const original=phiApp.rig.setEmotion.bind(phiApp.rig);
          phiApp.rig.setEmotion=value=>{calls.push(value);original(value);};
          phiApp.restoreAmbientPose();phiApp.orchestrator.isRunning=false;phiApp.restoreAmbientPose();
          phiApp.rig.setEmotion=original;return calls;
        }"""), ['error'])
        self.assertTrue(self.page.evaluate('phiApp.state.snapshot().vector.debt===beforeDebt'))

    def test_examples_disconnect_live_captures(self):
        self.observe("capture([row('a',{phase:'completed'})])")
        self.assertIn('Task completed', self.page.locator('.phi-activity-agent').inner_text())
        self.assertEqual(self.page.evaluate('phiApp.activityEmotion'), 'idle')
        self.page.locator('#btn-examples').click()
        self.assertEqual(self.page.locator('.phi-activity-agent').count(), 0)
        self.assertIn('Example mode', self.page.locator('.phi-activity-status').inner_text())

    def test_failed_poll_removes_old_running_status(self):
        self.observe('capture()')
        self.page.evaluate("phiApp.activity.fetch=async()=>{throw Error('offline')}; phiApp.activity.connect()")
        self.page.wait_for_function("phiApp.activity.snapshot.reason==='request_failed'")
        self.assertEqual(self.page.locator('.phi-activity-agent').count(), 0)
        self.assertIn('unknown', self.page.locator('.phi-activity-status').inner_text())

    def test_asked_and_idle_steward_receive_same_runtime_evidence(self):
        self.observe("capture([row('a',{phase:'completed',status:'incomplete',evidence:{...evidence,outstanding:2,untested_lines:7}})])")
        result = self.page.evaluate("""async () => {
          const calls=[];phiApp.refreshWorkspaceSignals=async()=>{};
          phiApp.idle.shouldSpeak=()=>true;
          phiApp.steward.propose=signals=>{calls.push(signals);return [];};
          await phiApp.askPhi();await phiApp.considerNextSteps();
          return {calls:calls.length,bothSame:calls.every(s=>s.activity===phiApp.activity.snapshot),
            counts:calls.map(s=>s.activity.agents[0].evidence.untested_lines),
            inventedUnread:calls.some(s=>s.friction.unreviewed!==undefined)};
        }""")
        self.assertEqual(result, {'calls': 2, 'bothSame': True, 'counts': [7, 7], 'inventedUnread': False})

    def test_idle_suppresses_running_agents_before_and_after_workspace_refresh(self):
        self.observe('capture()')
        self.assertEqual(self.page.evaluate("""async () => {
          let refreshes=0,proposals=0;phiApp.idle.shouldSpeak=()=>true;
          phiApp.refreshWorkspaceSignals=async()=>{refreshes++;};
          phiApp.steward.propose=()=>{proposals++;return [];};
          await phiApp.considerNextSteps();return {refreshes,proposals};
        }"""), {'refreshes': 0, 'proposals': 0})
        self.assertEqual(self.page.evaluate("""async () => {
          let proposals=0;phiApp.activity.snapshot.agents[0].phase='completed';
          phiApp.refreshWorkspaceSignals=async()=>{phiApp.activity.snapshot.agents[0].phase='running';};
          phiApp.steward.propose=()=>{proposals++;return [];};
          await phiApp.considerNextSteps();return proposals;
        }"""), 0)
        self.assertFalse(self.page.evaluate("""() => {
          phiApp.activity.snapshot.agents[0].status='stale';return phiApp.observedAgentsRunning();
        }"""))

    def test_inspect_activity_focuses_observed_agent_without_model_or_narration_changes(self):
        self.observe("capture([row('a',{phase:'completed',status:'incomplete',evidence:null})])")
        self.page.evaluate("""() => {
          window.inspectionCalls={model:0,speech:0};
          phiApp.prepareReading=async()=>{inspectionCalls.model++;};
          phiApp.rig.setSpeechText=()=>{inspectionCalls.speech++;};
          phiApp.orchestrator.isRunning=true;
          phiApp.renderProposals([{id:'inspect:a',kind:'verify',title:'Inspect captured work',
            rationale:'Evidence incomplete.',evidence:['Agent a'],task:{kind:'inspect_activity',target:'a',session_id:'session',task_id:'task_a'}}]);
        }""")
        self.page.get_by_role('button', name='Inspect activity', exact=True).click()
        self.assertEqual(self.page.evaluate('inspectionCalls'), {'model': 0, 'speech': 0})
        self.assertEqual(self.page.evaluate('document.activeElement.dataset.agentId'), 'a')
        self.assertEqual(self.page.evaluate("""() => {
          phiApp.activity.publish(phiApp.activity.snapshot);return document.activeElement.dataset.agentId;
        }"""), 'a')
        self.assertTrue(self.page.evaluate("""() => {
          phiApp.activity.publish({...phiApp.activity.snapshot,status:'unavailable',agents:[]});
          return document.activeElement.classList.contains('phi-activity-agents');
        }"""))
        self.assertTrue(self.page.evaluate('phiApp.orchestrator.isRunning'))
        self.assertEqual(self.server.state['posts'], [])
        self.assertTrue(self.page.locator('#phi-proposals').is_hidden())

    def test_envelope_inspection_focuses_empty_activity_without_model_request(self):
        result = self.page.evaluate("""async () => {
          let model=0;phiApp.prepareReading=async()=>{model++;};
          phiApp.activity.disconnect();
          const accepted=await phiApp.acceptProposal({id:'inspect:unknown',task:{kind:'inspect_activity',target:''}});
          return {accepted,model,focused:document.activeElement.classList.contains('phi-activity-agents')};
        }""")
        self.assertEqual(result, {'accepted': True, 'model': 0, 'focused': True})
        self.assertIn('observations and their limits', self.page.locator('#workspace-status').inner_text())
        self.assertEqual(self.server.state['posts'], [])

    def test_mobile_inspection_reveals_target_without_dismissing_other_proposals(self):
        self.observe("capture([row('a',{phase:'completed'}),row('b',{phase:'completed'})])")
        self.page.set_viewport_size({'width': 390, 'height': 844})
        self.page.evaluate("""() => {
          window.inspectionChoices=['a','b'].map(id=>({id:'inspect:'+id,kind:'verify',title:'Inspect '+id,
            rationale:'Check recorded work.',evidence:['Agent '+id],task:{kind:'inspect_activity',
              target:id,session_id:'session',task_id:'task_'+id}}));
          phiApp.refreshWorkspaceSignals=async()=>{};
          phiApp.steward.propose=()=>inspectionChoices.filter(item=>!phiApp.steward.dismissed.has(item.id));
          phiApp.renderProposals(inspectionChoices);
        }""")
        self.page.locator('[data-proposal-id="inspect:a"]').get_by_role('button', name='Inspect activity', exact=True).click()
        self.assertTrue(self.page.locator('#phi-proposals').is_hidden())
        self.assertTrue(self.page.evaluate("""() => {
          const target=document.activeElement;
          if(target.dataset.agentId!=='a')return false;
          const r=target.getBoundingClientRect();
          const hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
          return hit===target || target.contains(hit);
        }"""))
        self.assertEqual(self.page.evaluate("({selected:phiApp.steward.dismissed.has('inspect:a'),other:phiApp.steward.dismissed.has('inspect:b')})"),
                         {'selected': True, 'other': False})
        self.page.evaluate('phiApp.askPhi()')
        self.assertEqual(self.page.locator('.phi-proposal').count(), 1)
        self.assertEqual(self.page.locator('.phi-proposal').get_attribute('data-proposal-id'), 'inspect:b')
        self.assertEqual(self.server.state['posts'], [])

    def test_mobile_asked_proposals_stay_above_real_mascot_and_speech_bubble(self):
        self.page.set_viewport_size({'width': 390, 'height': 844})
        self.page.reload()
        self.page.wait_for_function('!!window.phiApp?.document && !phiApp.examples')
        self.observe("capture(Array.from({length:4},(_,i)=>row('agent_'+i,{status:'stale',phase:'completed'})))")
        proposals = self.page.evaluate('phiApp.askPhi()')
        self.assertEqual(len(proposals), 4)
        self.assertTrue(self.page.locator('.phi-speech-bubble').is_visible())
        self.assertTrue(self.page.evaluate("""() => {
          const a=phiApp.rig.wrapper.getBoundingClientRect();
          const b=document.getElementById('phi-proposals').getBoundingClientRect();
          return a.left<b.right && a.right>b.left && a.top<b.bottom && a.bottom>b.top;
        }"""))
        buttons = self.page.locator('#phi-proposals').get_by_role('button', name='Inspect activity', exact=True)
        for button in (buttons.first, buttons.last):
            button.scroll_into_view_if_needed()
            self.assertTrue(button.evaluate("""button => {
              const r=button.getBoundingClientRect();
              const hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
              return hit===button || button.contains(hit);
            }"""))
        self.assertEqual(self.server.state['posts'], [])

    def test_mobile_long_proposals_scroll_inside_visible_viewport(self):
        self.page.set_viewport_size({'width': 390, 'height': 844})
        self.page.evaluate("""() => {
          const long='Task completion and passing commands do not establish that all changes were reviewed or covered. ';
          phiApp.renderProposals(Array.from({length:4},(_,i)=>({id:'long:'+i,kind:'verify',
            title:'Inspect incomplete agent verification evidence',rationale:long.repeat(3),
            evidence:['Agent '+('a'.repeat(24)),'Task '+('b'.repeat(24)),'2 unreviewed lines, 2 lines without confirmed coverage'],
            task:{kind:'inspect_activity',target:'agent_'+i,session_id:'session',task_id:'task_'+i}})));
        }""")
        host = self.page.locator('#phi-proposals')
        bounds = host.bounding_box()
        self.assertGreaterEqual(bounds['y'], 0)
        self.assertLessEqual(bounds['y'] + bounds['height'], 844)
        self.assertTrue(self.page.evaluate("""() => {
          const host=document.getElementById('phi-proposals');return host.scrollHeight>host.clientHeight;
        }"""))
        buttons = host.get_by_role('button', name='Inspect activity', exact=True)
        for button in (buttons.first, buttons.last):
            button.scroll_into_view_if_needed()
            self.assertTrue(button.evaluate("""button => {
              const r=button.getBoundingClientRect(),host=document.getElementById('phi-proposals').getBoundingClientRect();
              const hit=document.elementFromPoint((r.left+r.right)/2,(r.top+r.bottom)/2);
              return r.top>=host.top && r.bottom<=host.bottom && (hit===button || button.contains(hit));
            }"""))
        self.assertTrue(self.page.evaluate("document.getElementById('phi-proposals').scrollTop>0"))
        self.page.evaluate("document.getElementById('phi-proposals').hidden=true")
        self.assertTrue(host.is_hidden())

    def test_inspection_and_focus_bind_same_agent_to_exact_session_and_task(self):
        self.observe("capture([row('a',{task_id:'first'}),row('a',{task_id:'second'})])")
        self.assertEqual(self.page.evaluate("""async () => {
          const accepted=await phiApp.acceptProposal({id:'inspect:second',task:{kind:'inspect_activity',
            target:'a',session_id:'session',task_id:'second'}});
          phiApp.activity.publish(phiApp.activity.snapshot);
          return {accepted,task:document.activeElement.dataset.taskId,session:document.activeElement.dataset.sessionId};
        }"""), {'accepted': True, 'task': 'second', 'session': 'session'})
        self.assertEqual(self.page.evaluate("""async () => {
          let model=0;phiApp.prepareReading=async()=>{model++;};
          const replacement={...phiApp.activity.snapshot.agents[0],task_id:'third'};
          phiApp.activity.publish({...phiApp.activity.snapshot,agents:[replacement]});
          const listFocused=document.activeElement.classList.contains('phi-activity-agents');
          const oldAccepted=await phiApp.acceptProposal({id:'inspect:second',task:{kind:'inspect_activity',
            target:'a',session_id:'session',task_id:'second'}});
          const incompleteIdentity=await phiApp.acceptProposal({id:'inspect:legacy',task:{kind:'inspect_activity',target:'a'}});
          const wrongSession=await phiApp.acceptProposal({id:'inspect:session',task:{kind:'inspect_activity',
            target:'a',session_id:'other',task_id:'third'}});
          return {model,listFocused,oldAccepted,incompleteIdentity,wrongSession,
            stillListFocused:document.activeElement.classList.contains('phi-activity-agents')};
        }"""), {'model': 0, 'listFocused': True, 'oldAccepted': False, 'incompleteIdentity': False,
                 'wrongSession': False, 'stillListFocused': True})
        self.assertEqual(self.server.state['posts'], [])

    def test_asked_proposals_do_not_replace_active_narration_and_missing_capture_is_not_actioned(self):
        self.observe("capture([row('a',{phase:'completed'})])")
        result = self.page.evaluate("""async () => {
          let speech=0,model=0;
          phiApp.refreshWorkspaceSignals=async()=>{};phiApp.orchestrator.isRunning=true;
          phiApp.rig.setSpeechText=()=>{speech++;};phiApp.prepareReading=async()=>{model++;};
          phiApp.steward.propose=()=>[];
          await phiApp.askPhi();
          const accepted=await phiApp.acceptProposal({id:'old',task:{kind:'inspect_activity',target:'missing'}});
          return {speech,model,accepted,running:phiApp.orchestrator.isRunning};
        }""")
        self.assertEqual(result, {'speech': 0, 'model': 0, 'accepted': False, 'running': True})
        self.assertIn('no longer visible', self.page.locator('#workspace-status').inner_text())

    def test_file_proposal_loads_its_target_and_acknowledges_only_accepted_job(self):
        target = fixture.document(fixture.SOURCE)
        target['path'] = 'src/a.rs'
        self.page.route('**/api/ide/document?path=src%2Fa.rs', lambda route: route.fulfill(
            status=200, content_type='application/json', body=json.dumps(target)))
        self.server.state['release'] = True
        result = self.page.evaluate("""async () => {
          phiApp.document={...phiApp.document,path:'src/b.rs'};
          let acknowledged=0,dismissed=0;
          phiApp.presence.acknowledge=()=>{acknowledged++;};phiApp.steward.dismiss=()=>{dismissed++;};
          const result=await phiApp.acceptProposal({id:'read:a',task:{kind:'review',target:'src/a.rs',question:'Explain A'}});
          return {status:result.status,job:result.job?.id,path:phiApp.document.path,acknowledged,dismissed};
        }""")
        self.assertEqual(result, {'status': 'accepted', 'job': 'fixture-reading-1', 'path': 'src/a.rs', 'acknowledged': 1, 'dismissed': 1})
        requests = [item for item in self.server.state['posts'] if item['path'] == '/api/assistant/review']
        self.assertEqual(len(requests), 1)
        self.assertEqual(requests[0]['body']['path'], 'src/a.rs')

    def test_dirty_busy_and_unreadable_proposals_never_dismiss_or_call_model(self):
        self.page.route('**/api/ide/document?path=missing%3A%3Asymbol', lambda route: route.fulfill(
            status=404, content_type='application/json', body='{"error":"document_not_found"}'))
        result = self.page.evaluate("""async () => {
          let acknowledged=0,dismissed=0;
          phiApp.presence.acknowledge=()=>{acknowledged++;};phiApp.steward.dismiss=()=>{dismissed++;};
          const proposal={id:'repair:a',task:{kind:'debug',target:'missing::symbol',question:'Inspect target'}};
          phiApp.dirty=true;document.getElementById('code-buffer').value='unsaved user work';
          const dirty=await phiApp.acceptProposal(proposal);
          const buffer=document.getElementById('code-buffer').value;
          phiApp.dirty=false;phiApp.polling=true;const busy=await phiApp.acceptProposal(proposal);
          phiApp.polling=false;const missing=await phiApp.acceptProposal(proposal);
          return {acknowledged,dismissed,dirty,busy,missing,buffer,path:phiApp.document.path};
        }""")
        self.assertEqual(result, {'acknowledged': 0, 'dismissed': 0, 'dirty': False, 'busy': False,
                                  'missing': False, 'buffer': 'unsaved user work', 'path': 'src/lib.rs'})
        self.assertEqual(self.server.state['posts'], [])

    def test_rejected_request_keeps_proposal_and_accepted_poll_failure_keeps_job(self):
        self.page.route('**/api/assistant/review', lambda route: route.fulfill(
            status=503, content_type='application/json', body='{"error":"temporarily_unavailable"}'))
        self.assertEqual(self.page.evaluate("""async () => {
          window.proposalAck=0;window.proposalDismiss=0;
          phiApp.presence.acknowledge=()=>{proposalAck++;};phiApp.steward.dismiss=()=>{proposalDismiss++;};
          const result=await phiApp.acceptProposal({id:'review:a',task:{kind:'review',target:'src/lib.rs',question:'Explain'}});
          return {status:result.status,ack:proposalAck,dismiss:proposalDismiss};
        }"""), {'status': 'failed', 'ack': 0, 'dismiss': 0})
        self.page.unroute('**/api/assistant/review')
        self.page.route('**/api/assistant/review/status?*', lambda route: route.fulfill(
            status=503, content_type='application/json', body='{"error":"poll_unavailable"}'))
        self.assertEqual(self.page.evaluate("""async () => {
          const result=await phiApp.acceptProposal({id:'review:a',task:{kind:'review',target:'src/lib.rs',question:'Explain'}});
          const second=await phiApp.acceptProposal({id:'review:a',task:{kind:'review',target:'src/lib.rs',question:'Explain'}});
          return {status:result.status,job:phiApp.workspace.pending()?.id,ack:proposalAck,dismiss:proposalDismiss,second};
        }"""), {'status': 'accepted', 'job': 'fixture-reading-1', 'ack': 1, 'dismiss': 1, 'second': False})
        self.assertEqual(len(self.server.state['posts']), 1)

    def test_expression_sounds_are_explicit_and_independent_of_narration(self):
        button = self.page.get_by_role('button', name='Phi expression sounds', exact=True)
        self.assertEqual(button.get_attribute('aria-pressed'), 'false')
        self.assertFalse(self.page.evaluate('phiApp.expressionVoice.enabled'))
        self.page.evaluate("""() => {
          window.soundToggleCalls=[];window.soundBefore={source:phiApp.document.content,
            voice:phiApp.speechEnabled,question:document.getElementById('reading-question').value};
          const set=phiApp.expressionVoice.setEnabled.bind(phiApp.expressionVoice);
          phiApp.expressionVoice.setEnabled=value=>{soundToggleCalls.push(value);return set(value);};
        }""")
        button.click()
        self.assertEqual(button.inner_text(), 'Sounds on')
        self.assertEqual(button.get_attribute('aria-pressed'), 'true')
        self.assertTrue(self.page.evaluate('phiApp.expressionVoice.enabled'))
        button.click()
        self.assertEqual(button.inner_text(), 'Sounds off')
        self.assertFalse(self.page.evaluate('phiApp.expressionVoice.enabled'))
        self.assertEqual(self.page.evaluate('soundToggleCalls'), [True, False])
        self.assertTrue(self.page.evaluate("""phiApp.document.content===soundBefore.source &&
          phiApp.speechEnabled===soundBefore.voice && document.getElementById('reading-question').value===soundBefore.question"""))
        self.assertEqual(self.server.state['posts'], [])
        self.page.set_viewport_size({'width': 390, 'height': 844})
        self.assertTrue(self.page.evaluate('document.documentElement.scrollWidth <= innerWidth'))
        self.assertTrue(button.is_visible())


if __name__ == '__main__':
    unittest.main()
