"""Run the production evidence-only friction monitor in Node with a measured clock.

The older external prototype remains covered by test_phi_friction.py. These
tests deliberately exercise the separate production monitor, including cases
that must never turn a compiler error into a diagnosis of the developer.
"""
import json
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE = ROOT / "src/evolve/web/phi/phi_friction_monitor.js"


class PhiFrictionMonitorTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which("node"):
            self.skipTest("Node is required for the production friction monitor")
        source = f"""
import assert from 'node:assert/strict';
import {{PhiFrictionMonitor,FRICTION_KINDS}} from {json.dumps(MODULE.as_uri())};
let clock=1_000_000, serial=0;
const emitted=[];
const monitor=new PhiFrictionMonitor({{now:()=>clock,cooldownMs:0,onIntervention:x=>emitted.push(x)}});
function send(kind,data={{}},generation='g1',task='task',extra={{}}) {{
 return monitor.ingest({{id:`event-${{++serial}}`,kind,task_id:task,generation_id:generation,
   source:'ide',at_ms:clock,data,...extra}});
}}
function generation(id='g1',data={{}},task='task') {{
 return send('generation_finished',{{status:'staged',added_lines:12,...data}},id,task);
}}
function diagnostic(id='g1',fingerprint='A',code='E0308',data={{}},task='task') {{
 return send('diagnostics_finished',{{success:false,evidence_complete:true,
   diagnostics:[{{code,fingerprint}}],...data}},id,task);
}}
function attempt(id,fingerprint='A',code='E0308',task='task') {{
 clock+=1000;generation(id,{{}},task);return diagnostic(id,fingerprint,code,{{}},task);
}}
""" + body
        result = subprocess.run(["node", "--input-type=module", "-"], input=source,
                                text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_generation_link_and_complete_evidence_are_required(self):
        self.node("""
assert.equal(diagnostic('unknown','A','E0432'),null);
generation();
assert.equal(diagnostic('g1','A','E0432',{evidence_complete:false}),null);
assert.equal(diagnostic('g1','A','E0432',{},'manual-task'),null);
assert.equal(send('diagnostics_finished',{success:false,evidence_complete:true,
 diagnostics:[{code:'E0432',fingerprint:'A'}]},null),null);
assert.equal(emitted.length,0);
assert.equal(diagnostic('g1','A','E0432').kind,'unresolved_api');
""")

    def test_unresolved_api_is_not_claimed_to_be_a_hallucination(self):
        self.node("""
generation();
const result=diagnostic('g1','A','E0599',{diagnostics:[{code:'E0599',fingerprint:'A',symbol:'read_file'}]});
assert.equal(result.motion,'curious');
assert.equal(result.evidence.find(x=>x.label==='Symbol reported').value,'read_file');
assert(result.message.includes('version and feature flags'));
assert(result.evidence.some(x=>String(x.value).includes('not proof')));
assert(!/hallucinat|does not exist|crazy|fatigue|burnout|30 seconds/i.test(result.message));
assert.deepEqual(result.actions.map(x=>x.id),['inspect_diagnostics','narrow_retry']);
""")

    def test_generic_transport_or_test_failure_does_not_invent_diagnostics(self):
        self.node("""
assert.equal(generation('g1',{status:'failed',reason_code:'provider_timeout',
 message:'unresolved import hallucinated_symbol',diagnostics:[{message:'no method named x'}]}),null);
assert.equal(diagnostic('g1','A','test_failed'),null);
assert.equal(emitted.length,0);
""")

    def test_four_distinct_alternating_generations_are_required(self):
        self.node("""
assert.equal(attempt('g1','A'),null);assert.equal(attempt('g2','B'),null);
assert.equal(attempt('g3','A'),null);const result=attempt('g4','B');
assert.equal(result.kind,'circular_spin');assert.equal(result.motion,'walk');
assert.equal(result.evidence.find(x=>x.label==='Distinct generated attempts').value,4);
assert.deepEqual(result.actions.map(x=>x.id),['inspect_diagnostics','narrow_retry']);
""")

    def test_repeated_identical_error_is_not_an_alternating_cycle(self):
        self.node("""
for(let i=0;i<8;i++)assert.equal(attempt(`g${i}`,'same'),null);
assert.equal(emitted.length,0);
""")

    def test_duplicate_polls_do_not_create_attempts_or_rejections(self):
        self.node("""
generation();
for(let i=0;i<20;i++) {
 generation();diagnostic('g1',i%2?'A':'B');
 send('review_closed',{decision:'rejected'});
}
assert.equal(monitor.getSnapshot().generationCount,1);assert.equal(emitted.length,0);
""")

    def test_cycle_uses_exact_sets_without_join_collisions(self):
        self.node("""
const sets=[['a|b','c'],['a','b|c'],['a|b','c'],['a','b|c']];
for(let i=0;i<4;i++) {
 clock+=1000;generation(`g${i}`);
 const r=diagnostic(`g${i}`,'unused','E0308',{
 diagnostics:sets[i].map(fingerprint=>({code:'E0308',fingerprint}))});
 assert.equal(r?.kind??null,i===3?'circular_spin':null);
}
""")

    def test_user_rejection_streak_excludes_validator_rejections(self):
        self.node("""
for(let i=0;i<3;i++) {
 clock+=1000;assert.equal(generation(`v${i}`,{status:'rejected',reason_code:'validation_failed'}),null);
}
for(let i=0;i<3;i++) {
 clock+=1000;generation(`u${i}`);
 const r=send('review_closed',{decision:'rejected'},`u${i}`);
 assert.equal(r?.kind??null,i===2?'review_rejections':null);
}
assert.equal(emitted.length,1);
""")

    def test_explicit_server_developer_rejection_counts_once(self):
        self.node("""
for(let i=0;i<3;i++) {
 clock+=1000;
 const r=generation(`g${i}`,{status:'rejected',reason_code:'developer_rejected'});
 assert.equal(r?.kind??null,i===2?'review_rejections':null);
 assert.equal(send('review_closed',{decision:'rejected'},`g${i}`),null);
}
assert.equal(emitted.length,1);
""")

    def test_accepted_review_breaks_rejection_streak(self):
        self.node("""
for(let i=0;i<2;i++){clock+=1000;generation(`g${i}`);send('review_closed',{decision:'rejected'},`g${i}`);}
clock+=1000;generation('accepted');send('review_closed',{decision:'accepted'},'accepted');
clock+=1000;generation('g3');assert.equal(send('review_closed',{decision:'rejected'},'g3'),null);
assert.equal(emitted.length,0);
""")

    def test_large_patch_uses_actual_lines_not_expected_ratio(self):
        self.node("""
assert.equal(generation('small',{added_lines:349,expected_lines:1}),null);
clock+=1000;const r=generation('large',{added_lines:350,files_changed:4,expected_lines:3000});
assert.equal(r.kind,'large_diff');assert.equal(r.motion,'stretch');
assert(r.message.includes('350 lines'));assert(!r.message.includes('expected'));
assert(!/garbage|boilerplate|exhaust|mental|unnecessary/i.test(r.message));
assert.equal(r.evidence.find(x=>x.label==='Files changed').value,4);
""")

    def test_unknown_negative_or_invented_line_counts_do_not_trigger(self):
        self.node("""
for(const [i,added_lines] of [undefined,null,-1,Infinity,NaN,'400',400.5].entries()) {
 clock+=1000;assert.equal(generation(`g${i}`,{added_lines,expected_lines:1}),null);
}
assert.equal(emitted.length,0);
""")

    def test_review_rate_is_measured_and_not_a_fatigue_diagnosis(self):
        self.node("""
generation('g1',{added_lines:undefined});
const r=send('review_closed',{decision:'closed',added_lines:400,active_review_ms:120000});
assert.equal(r.kind,'large_diff');
assert.equal(r.evidence.find(x=>x.label==='Added lines per active review minute').value,200);
assert(!/fatigue|overload|slop/i.test(r.message));
""")

    def test_undo_requires_three_associated_actions_in_fifteen_seconds(self):
        self.node("""
generation();clock+=1000;
assert.equal(send('editor_undo',{},null),null);
assert.equal(send('editor_undo',{},'other'),null);
assert.equal(send('editor_undo',{},'g1','other-task'),null);
assert.equal(send('editor_undo',{}),null);clock+=1000;
assert.equal(send('editor_undo',{}),null);clock+=1000;
assert.equal(send('editor_undo',{}).kind,'rapid_undo');
assert.equal(emitted.length,1);
""")

    def test_undo_replays_late_arrivals_and_server_counts_are_ignored(self):
        self.node("""
generation();clock+=1000;
const event={id:'undo',kind:'editor_undo',task_id:'task',generation_id:'g1',
 at_ms:clock,source:'ide',data:{count:1}};
assert.equal(monitor.ingest(event),null);assert.equal(monitor.ingest(event),null);
assert.equal(send('editor_undo',{count:3},'g1','task',{source:'server'}),null);
clock+=15001;assert.equal(send('editor_undo',{count:3}),null);
assert.equal(monitor.ingest({...event,id:'late',data:{count:3}}),null);
assert.equal(emitted.length,0);
""")

    def test_success_breaks_cycle_and_dismisses_old_evidence(self):
        self.node("""
attempt('g1','A');attempt('g2','B');attempt('g3','A');
assert.equal(diagnostic('g3','unused','E0308',{success:true,diagnostics:[]}),null);
assert.equal(attempt('g4','B'),null);assert.equal(emitted.length,0);
clock+=1000;generation('g5');diagnostic('g5','missing','E0432');
assert(monitor.getSnapshot().current);
clock+=1000;generation('g6',{status:'applied'});
assert.equal(monitor.getSnapshot().current,null);
""")

    def test_late_diagnostics_cannot_revive_an_older_generation(self):
        self.node("""
generation('old');clock+=1000;generation('current');
assert.equal(diagnostic('old','missing','E0432'),null);
assert.equal(emitted.length,0);
assert.equal(diagnostic('current','missing','E0432').generation_id,'current');
clock+=1000;diagnostic('old','unused','E0308',{success:true,diagnostics:[]});
assert.equal(monitor.getSnapshot().current.generation_id,'current');
""")

    def test_task_changes_and_explicit_context_reset_break_cycles(self):
        self.node("""
attempt('g1','A');attempt('g2','B');attempt('g3','A');
assert.equal(attempt('x1','B','E0308','other'),null);
assert.equal(attempt('g3','B'),null); // observed old generation cannot reclaim the scope
assert.equal(monitor.getSnapshot().taskId,'other');
clock+=1000;send('context_changed',{reason:'manual'},null,'task');
assert.equal(attempt('g5','B'),null);
assert.equal(monitor.getSnapshot().generationCount,1);
assert.equal(emitted.length,0);
""")

    def test_reset_scope_rejects_old_events_with_new_transport_ids(self):
        self.node("""
generation();clock+=1000;send('context_changed',{reason:'agent_reset'});
assert.equal(generation('g1',{added_lines:400}),null); // same-id replay must not become new evidence
""")

    def test_global_cooldown_is_automatic_and_dismiss_does_not_reset_it(self):
        self.node("""
monitor.cooldownMs=120000;
const first=generation('g1',{added_lines:400});assert(first);
assert.equal(monitor.dismiss('not-current'),false);
assert.equal(monitor.dismiss(first.id),true);
clock+=1000;generation('g2');assert.equal(diagnostic('g2','missing','E0432'),null);
clock+=120001;generation('g3');assert.equal(diagnostic('g3','missing','E0432').kind,'unresolved_api');
assert.equal(emitted.length,2);
""")

    def test_snooze_disable_and_destroy_preserve_user_control(self):
        self.node("""
generation('g1',{added_lines:400});assert(monitor.snooze(900000));
assert.equal(monitor.getSnapshot().current,null);
clock+=1000;assert.equal(generation('g2',{added_lines:400}),null);
clock+=900001;assert(generation('g3',{added_lines:400}));
monitor.setEnabled(false);assert.equal(monitor.getSnapshot().historySize,0);
assert.equal(generation('g4',{added_lines:400}),null);
monitor.setEnabled(true);assert(generation('g5',{added_lines:400}));
monitor.destroy();monitor.setEnabled(true);assert.equal(generation('g6',{added_lines:400}),null);
assert.equal(monitor.getSnapshot().enabled,false);
""")

    def test_history_and_generation_storage_are_bounded_and_expire(self):
        self.node("""
const old=clock;
for(let i=0;i<1000;i++){clock+=1;generation(`g${i}`);}
assert(monitor.getSnapshot().historySize<=256);assert(monitor.getSnapshot().generationCount<=256);
clock+=30*60*1000+1;
assert.equal(monitor.getSnapshot().historySize,0);assert.equal(monitor.getSnapshot().generationCount,0);
assert.equal(send('generation_finished',{status:'staged',added_lines:400},'expired','task',{at_ms:old}),null);
assert.equal(monitor.getSnapshot().generationCount,0);
""")

    def test_closed_review_is_not_final_and_rejection_can_be_reconsidered(self):
        self.node("""
for(let i=0;i<3;i++) {
 clock+=1000;generation(`g${i}`);
 assert.equal(send('review_closed',{decision:'closed',active_review_ms:1000},`g${i}`),null);
 clock+=1000;const r=send('review_closed',{decision:'rejected',active_review_ms:2000},`g${i}`);
 assert.equal(r?.kind??null,i===2?'review_rejections':null);
}
clock+=1000;send('review_closed',{decision:'accepted'},'g2');
assert.equal(monitor.getSnapshot().current,null);
clock+=1000;generation('g4');
assert.equal(send('review_closed',{decision:'rejected'},'g4'),null);
assert.equal(emitted.length,1);
""")

    def test_genuinely_new_generation_can_return_to_a_previous_task(self):
        self.node("""
attempt('g1','A','E0308','task');attempt('g2','B','E0308','task');
attempt('other1','A','E0308','other');
assert.equal(attempt('g1','B','E0308','task'),null);
assert.equal(monitor.getSnapshot().taskId,'other');
assert.equal(attempt('new1','A','E0308','task'),null);
assert.equal(monitor.getSnapshot().taskId,'task');
assert.equal(monitor.getSnapshot().generationCount,1);
assert.equal(attempt('new2','B','E0308','task'),null);
assert.equal(attempt('new3','A','E0308','task'),null);
assert.equal(attempt('new4','B','E0308','task').kind,'circular_spin');
""")

    def test_generation_evidence_flag_cannot_be_false_or_malformed(self):
        self.node("""
for(const [i,evidence_complete] of [false,null,'true',0].entries()) {
 clock+=1000;assert.equal(generation(`g${i}`,{evidence_complete,
 diagnostics:[{code:'E0432',fingerprint:'missing'}]}),null);
}
clock+=1000;assert.equal(generation('verified',{evidence_complete:true,
 diagnostics:[{code:'E0432',fingerprint:'missing'}]}).kind,'unresolved_api');
""")

    def test_late_night_is_opt_in_and_requires_measured_active_duration(self):
        self.node("""
for(let i=0;i<212;i++){clock+=60000;send('activity',{active_ms:60000,local_hour:2},null,null);}
generation('g1',{added_lines:400});clock+=1000;
assert.equal(send('activity',{active_ms:1000,local_hour:2},null,null),null);
monitor.setLateNightEnabled(true);
clock+=1000;const r=send('activity',{active_ms:1000,local_hour:2},null,null);
assert.equal(r.kind,'late_night');assert.equal(r.motion,'sleep');
assert(r.evidence.some(x=>String(x.value).includes('no fatigue inference')));
assert(!/tired|fatigue|deplet|sloppy|burnout/i.test(r.message));
clock+=1000;assert.equal(send('activity',{active_ms:1000,local_hour:2},null,null),null);
""")

    def test_idle_time_large_increments_and_night_alone_do_not_count(self):
        self.node("""
monitor.setLateNightEnabled(true);clock+=4*60*60*1000;
generation('g1',{added_lines:400});
assert.equal(send('activity',{active_ms:4*60*60*1000,local_hour:2},null,null),null);
assert(monitor.getSnapshot().activeMs<=60000);
for(let i=0;i<1000;i++)assert.equal(send('activity',{active_ms:60000,local_hour:2},null,null),null);
assert(monitor.getSnapshot().activeMs<=60000);
assert.equal(emitted.filter(x=>x.kind==='late_night').length,0);
""")

    def test_late_night_requires_recent_friction_and_actual_night(self):
        self.node("""
monitor.setLateNightEnabled(true);
for(let i=0;i<212;i++){clock+=60000;send('activity',{active_ms:60000,local_hour:2},null,null);}
assert.equal(emitted.length,0);
generation('g1',{added_lines:400});clock+=1000;
assert.equal(send('activity',{active_ms:1000,local_hour:14},null,null),null);
clock+=5*60*1000+1;
assert.equal(send('activity',{active_ms:1000,local_hour:2},null,null),null);
assert.equal(emitted.filter(x=>x.kind==='late_night').length,0);
""")

    def test_activity_session_survives_context_switch_but_not_long_idle(self):
        self.node("""
for(let i=0;i<10;i++){clock+=60000;send('activity',{active_ms:60000,local_hour:2},null,null);}
const active=monitor.getSnapshot().activeMs;
send('context_changed',{reason:'manual'},null,'another-file');
assert.equal(monitor.getSnapshot().activeMs,active);
clock+=31*60000;send('activity',{active_ms:0,local_hour:2},null,null);
clock+=1000;send('activity',{active_ms:1000,local_hour:2},null,null);
assert.equal(monitor.getSnapshot().activeMs,1000);
""")

    def test_review_duration_accepts_the_backend_full_day_bound(self):
        self.node("""
generation('g1',{added_lines:undefined});
const r=send('review_closed',{decision:'closed',added_lines:400,active_review_ms:3*60*60*1000});
assert.equal(r.evidence.find(x=>x.label==='Active review milliseconds').value,10800000);
assert.equal(r.evidence.find(x=>x.label==='Added lines per active review minute').value,2.2);
""")

    def test_bad_events_and_mutating_presenters_do_not_corrupt_state(self):
        self.node("""
for(const input of [null,{},[],{id:'bad id',kind:'generation_finished'},
 {id:'bad',kind:'generation_finished',source:'server',at_ms:clock+3000,data:{added_lines:400}}])
 assert.equal(monitor.ingest(input),null);
monitor.onIntervention=x=>{x.message='changed';x.evidence.push({label:'leak',value:'raw'});throw Error('presenter');};
const r=generation('g1',{added_lines:400,prompt:'private user code',source:'secret'});
assert(r);assert.notEqual(r.message,'changed');r.message='again';
assert(!JSON.stringify(monitor.getSnapshot()).includes('private user code'));
assert(!JSON.stringify(monitor.getSnapshot()).includes('changed'));
assert(!JSON.stringify(monitor.getSnapshot()).includes('leak'));
clock+=60001;assert.equal(monitor.getSnapshot().current,null);
""")


if __name__ == "__main__":
    unittest.main()
