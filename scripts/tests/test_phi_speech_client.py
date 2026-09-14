"""Real browser-module transport against controlled Fetch responses; no model calls."""
import json
import pathlib
import shutil
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
MODULE = ROOT / 'src/evolve/web/phi/phi_speech_client.js'


class SpeechClientTests(unittest.TestCase):
    def node(self, body):
        if not shutil.which('node'):
            self.skipTest('Node is required for the actual speech client regressions')
        source = f"""
import assert from 'node:assert/strict';
import {{PhiSpeechClient,SpeechClientError}} from {json.dumps(MODULE.as_uri())};
const id='a'.repeat(32), text='Read Option<f64> to the final word.', voice='Emma';
crypto.randomUUID=()=> 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa';
const flush=()=>new Promise(resolve=>setImmediate(resolve));
const sleep=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const json=(value,status=200)=>new Response(JSON.stringify(value),{{status,headers:{{'content-type':'application/json'}}}});
const wav=new Uint8Array(60), view=new DataView(wav.buffer), put=(at,s)=>wav.set(new TextEncoder().encode(s),at);
put(0,'RIFF');view.setUint32(4,52,true);put(8,'WAVE');put(12,'fmt ');view.setUint32(16,16,true);
view.setUint16(20,1,true);view.setUint16(22,1,true);view.setUint32(24,24000,true);view.setUint32(28,48000,true);
view.setUint16(32,2,true);view.setUint16(34,16,true);put(36,'data');view.setUint32(40,16,true);
const sha=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',wav)),b=>b.toString(16).padStart(2,'0')).join('');
const job=(status='done',extra={{}})=>({{id,status,text,voice,alignment:{{status:'unavailable'}},
 ...(status==='done'?{{audio:{{url:`/api/speech/jobs/${{id}}/audio`,sample_rate:24000,duration:8/24000,channels:1,bytes:wav.length,sha256:sha}}}}:{{}}),...extra}});
const caps={{configured:true,status:'ready',provider:'vibevoice_onnx',model:'elbruno/VibeVoice-Realtime-0.5B-ONNX',revision:'pinned',
 voices:[{{id:'Emma',name:'Emma'}}],default_voice:'Emma',sample_rate:24000,max_text_chars:5000,alignment:{{status:'unavailable'}},streaming:false}};
const audio=()=>new Response(wav,{{headers:{{'content-type':'audio/wav','content-length':String(wav.length)}}}});
const client=fetch=>new PhiSpeechClient({{getToken:()=> 'fixture-session',fetch,pollIntervalMs:10}});
""" + body
        result = subprocess.run(['node', '--input-type=module', '-'], input=source,
                                text=True, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_authenticated_complete_job_download_and_owned_url(self):
        self.node("""
const calls=[],states=[];let polls=0,created=0,revoked=0;
URL.createObjectURL=blob=>{created++;assert.equal(blob.size,wav.length);return 'blob:fixture'};
URL.revokeObjectURL=url=>{assert.equal(url,'blob:fixture');revoked++};
const c=client(async(path,options)=>{calls.push([path,options]);
 if(path.endsWith('capabilities'))return json(caps);
 if(path.endsWith('/audio'))return audio();
 if(options.method==='POST')return json(job('queued'),202);
 return json(job(++polls===1?'running':'done'));
});
assert.equal((await c.capabilities()).status,'ready');
const result=await c.synthesize(text,voice,{onStatus:s=>states.push(s.status)});
assert.deepEqual(states,['submitting','queued','running','done','downloading']);assert.equal(result.job.text,text);
assert.equal(result.job.alignment.status,'unavailable');assert.equal(created,1);assert.equal(revoked,0);result.dispose();result.dispose();assert.equal(revoked,1);
assert(calls.every(([,o])=>o.headers['x-selfware-session']==='fixture-session'&&o.redirect==='error'&&o.credentials==='same-origin'));
assert.deepEqual(JSON.parse(calls.find(([,o])=>o.method==='POST')[1].body),{text,voice,request_id:id});
""")

    def test_missing_session_and_malicious_paths_never_fetch(self):
        self.node("""
let calls=0;const fetch=async()=>{calls++;throw Error('must not fetch')};
await assert.rejects(new PhiSpeechClient({fetch}).capabilities(),e=>e.code==='session_required');
const c=client(fetch);for(const path of ['https://evil.invalid/api/speech/jobs','//evil.invalid/x','/api/speech/jobs/../audio','/api/speech/capabilities?token=x'])
 await assert.rejects(c.request(path),e=>e.code==='invalid_path');
await assert.rejects(c.status('../capabilities'),e=>e.code==='invalid_job');assert.equal(calls,0);
""")

    def test_job_owned_audio_path_and_identity_must_match(self):
        self.node("""
let calls=0;const c=client(async()=>{calls++;return json(job('done',{id:'b'.repeat(32)}))});
await assert.rejects(c.status(id),e=>e.code==='invalid_job');
for(const url of ['https://example.invalid/private.wav','/api/ide/document','/api/speech/jobs/'+ 'b'.repeat(32)+'/audio']){
 const bad=job();bad.audio.url=url;await assert.rejects(c.audio(bad),e=>e.code==='invalid_audio');
}assert.equal(calls,1);
""")

    def test_redirect_and_error_body_are_not_accepted_as_audio(self):
        self.node("""
const redirect=json(caps);Object.defineProperty(redirect,'redirected',{value:true});
await assert.rejects(client(async()=>redirect).capabilities(),e=>e.code==='redirect_rejected');
await assert.rejects(client(async()=>json({error:'not audio'})).audio(job()),e=>e.code==='invalid_response');
await assert.rejects(client(async()=>json({error:{code:'busy',message:'Speech queue is full.'}},429)).create(text,voice),e=>e.code==='busy'&&e.status===429);
""")

    def test_real_http_redirect_does_not_forward_session_header(self):
        self.node("""
const {createServer}=await import('node:http');let targetCalls=0,sourceToken=null;
const target=createServer((request,response)=>{targetCalls++;response.end('unexpected')});
await new Promise(resolve=>target.listen(0,'127.0.0.1',resolve));
const source=createServer((request,response)=>{sourceToken=request.headers['x-selfware-session'];response.writeHead(302,{Location:`http://127.0.0.1:${target.address().port}/leak`});response.end()});
await new Promise(resolve=>source.listen(0,'127.0.0.1',resolve));
try{
 const c=client((path,options)=>fetch(`http://127.0.0.1:${source.address().port}${path}`,options));
 await assert.rejects(c.capabilities(),e=>e.code==='network_error');assert.equal(sourceToken,'fixture-session');assert.equal(targetCalls,0);
}finally{source.closeAllConnections();target.closeAllConnections();await Promise.all([new Promise(r=>source.close(r)),new Promise(r=>target.close(r))])}
""")

    def test_audio_integrity_duration_and_truncation_are_checked(self):
        self.node("""
const c=client(async()=>audio()),wrongHash=job();wrongHash.audio.sha256='0'.repeat(64);
await assert.rejects(c.audio(wrongHash),e=>e.code==='audio_integrity');
const wrongDuration=job();wrongDuration.audio.duration=20;
await assert.rejects(c.audio(wrongDuration),e=>e.code==='invalid_audio');
await assert.rejects(client(async()=>new Response(wav.slice(0,50),{headers:{'content-type':'audio/wav','content-length':'60'}})).audio(job()),e=>e.code==='truncated_response');
const corrupt=wav.slice();corrupt[0]=0;
await assert.rejects(client(async()=>new Response(corrupt,{headers:{'content-type':'audio/wav'}})).audio(job()),e=>e.code==='invalid_audio');
""")

    def test_stream_response_limit_applies_without_content_length(self):
        self.node("""
let cancelled=false;const body=new ReadableStream({start(c){c.enqueue(new Uint8Array(61))},cancel(){cancelled=true}});
await assert.rejects(client(async()=>new Response(body,{headers:{'content-type':'audio/wav'}})).audio(job()),e=>e.code==='response_too_large');
assert.equal(cancelled,true);
""")

    def test_stalled_body_has_absolute_deadline(self):
        self.node("""
let cancelled=false;const body=new ReadableStream({start(c){c.enqueue(new TextEncoder().encode('{'))},cancel(){cancelled=true}});
const before=performance.now();await assert.rejects(client(async()=>new Response(body,{headers:{'content-type':'application/json'}})).capabilities({timeoutMs:25}),e=>e.code==='timeout');
assert(performance.now()-before<500);assert.equal(cancelled,true);
""")

    def test_abort_poll_cancels_job_with_fresh_signal(self):
        self.node("""
const controller=new AbortController(),calls=[];let canceled=false;
const c=client(async(path,o)=>{calls.push(path);if(path.endsWith('/cancel')){assert.equal(o.signal.aborted,false);canceled=true;return json(job('cancelled'))}return json(job('queued'),202)});
const p=c.synthesize(text,voice,{signal:controller.signal,onStatus:s=>{if(s.status==='queued')controller.abort()}});
await assert.rejects(p,e=>e.code==='cancelled');await flush();assert(canceled);assert.equal(calls.filter(p=>p==='/api/speech/jobs').length,1);assert(!calls.some(p=>p.endsWith('/audio')));
""")

    def test_abort_download_releases_stream_and_does_not_create_audio(self):
        self.node("""
const controller=new AbortController();let cancelled=false,created=0,serverCancel=false;
URL.createObjectURL=()=>{created++;return 'blob:bad'};
const body=new ReadableStream({start(c){c.enqueue(wav.slice(0,12))},cancel(){cancelled=true}});
const c=client(async(path)=>{if(path.endsWith('/cancel')){serverCancel=true;return json(job('cancelled'))}
 if(path.endsWith('/audio')){setTimeout(()=>controller.abort(),10);return new Response(body,{headers:{'content-type':'audio/wav'}})}return json(job())});
await assert.rejects(c.synthesize(text,voice,{signal:controller.signal}),e=>e.code==='cancelled');await flush();
assert(cancelled);assert(serverCancel);assert.equal(created,0);
""")

    def test_failure_and_wrong_narration_do_not_download_or_retry(self):
        self.node("""
let calls=0;const c=client(async()=>{calls++;return json(job('failed',{error:{code:'model_failed',message:'Worker failed'}}))});
await assert.rejects(c.synthesize(text,voice),e=>e.code==='model_failed'&&e.detail.status==='failed');assert.equal(calls,1);
const mismatch=client(async(path)=>path.endsWith('/cancel')?json(job('cancelled')):json(job('done',{text:'different narration'})));
await assert.rejects(mismatch.synthesize(text,voice),e=>e.code==='job_mismatch');
let attempts=0,cleanup=0;await assert.rejects(client(async(path)=>{if(path.endsWith('/cancel')){cleanup++;return json(job('cancelled'))}attempts++;throw Error('offline')}).synthesize(text,voice),e=>e.code==='network_error');assert.equal(attempts,1);assert.equal(cleanup,1);
""")

    def test_unicode_limit_is_codepoints_and_invalid_input_does_not_submit(self):
        self.node("""
let calls=0;const c=client(async(path,o)=>{calls++;const b=JSON.parse(o.body);return json(job('queued',{text:b.text,voice:b.voice}),202)});
await c.create('🦊'.repeat(5000),voice);assert.equal(calls,1);
for(const value of ['', ' ', '🦊'.repeat(5001),42])await assert.rejects(c.create(value,voice),e=>e.code==='invalid_text');
await assert.rejects(c.create(text,''),e=>e.code==='invalid_voice');assert.equal(calls,1);
""")

    def test_overall_generation_deadline_cancels_still_queued_job(self):
        self.node("""
let cancelled=false;const c=client(async(path)=>{if(path.endsWith('/cancel')){cancelled=true;return json(job('cancelled'))}return json(job('queued'))});
const start=performance.now();await assert.rejects(c.synthesize(text,voice,{timeoutMs:30}),e=>e.code==='timeout');await flush();assert(cancelled);assert(performance.now()-start<500);
""")

    def test_cancel_before_submission_receipt_uses_known_request_id(self):
        self.node("""
const controller=new AbortController();let submittedId=null,cancelledId=null;
const c=client(async(path,options)=>{
 if(path.endsWith('/cancel')){cancelledId=path.split('/').at(-2);assert.equal(options.signal.aborted,false);return json(job('cancelled'))}
 submittedId=JSON.parse(options.body).request_id;
 return new Promise((resolve,reject)=>options.signal.addEventListener('abort',()=>reject(new DOMException('aborted','AbortError')),{once:true}));
});
const pending=c.synthesize(text,voice,{signal:controller.signal});await flush();controller.abort();
await assert.rejects(pending,e=>e.code==='cancelled');await flush();assert.equal(submittedId,id);assert.equal(cancelledId,submittedId);
""")


if __name__ == '__main__':
    unittest.main()
