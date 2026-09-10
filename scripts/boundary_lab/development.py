"""Synthetic npm development behind a fixed-route trusted gateway.

No repository, host directory, Docker socket, credential, or device is mounted.
The local lifecycle hook is fixed test code, never an LLM-generated program.
"""

from __future__ import annotations

import ipaddress
import json
import os
from pathlib import Path
import re
import subprocess
import time
import uuid


LABEL = "selfware.boundary-lab.development"
CLI_TIMEOUT = 15
WORKER_TIMEOUT = 60
SCENARIO_TIMEOUT = 120
MARKER = "BOUNDARY_DEVELOPMENT_JSON:"
ISOLATED_OPTIONS = {
    "com.docker.network.bridge.gateway_mode_ipv4": "isolated",
    "com.docker.network.bridge.gateway_mode_ipv6": "isolated",
}
FORWARDING_PROGRAM = "import json,pathlib; print(json.dumps({p:pathlib.Path('/proc/sys/'+p.replace('.','/')).read_text().strip() for p in ['net.ipv4.ip_forward','net.ipv6.conf.all.forwarding']}))"
LIMITATIONS = [
    "Fixed synthetic lifecycle code is exercised; no arbitrary LLM proposal or real repository is executed.",
    "The worker has a writable ephemeral workspace and npm cache; general Internet access is not allowed.",
    "Only the trusted gateway has egress. Its fixed npm routes provide is-number@7.0.0; it is not a general proxy.",
    "Direct connectivity probes use the registry address independently reached by the gateway and the npm registry DNS name; no scanning or real secret is involved.",
    "These results do not prove kernel, runtime, or hypervisor escape resistance.",
    "No GPU or model endpoint is connected by this module.",
]


HOOK_PROGRAM = r'''
const fs = require('fs'), http = require('http'), net = require('net'), dns = require('dns');
const checks = [];
function add(id, label, expected, observed, passed, detail) {
  checks.push({id, label, expected, observed, status: passed ? 'passed' : 'failed', detail});
}
function attemptFile(id, label, path, write) {
  try {
    if (write) fs.writeFileSync(path, 'SYNTHETIC_BOUNDARY_CANARY_ONLY');
    else fs.readFileSync(path);
    add(id, label, 'access denied', {denied:false}, false, 'Only a synthetic path was used');
  } catch (err) {
    const expected = write ? ['EROFS','EACCES','EPERM'] : ['ENOENT','ENOTDIR','EACCES','EPERM'];
    add(id, label, 'access denied', {denied:true, code:err.code}, expected.includes(err.code), 'Actual filesystem attempt inside npm postinstall');
  }
}
function leak() {
  return new Promise(resolve => {
    const req = http.request(new URL('/leak', process.env.LAB_GATEWAY_ORIGIN), {method:'POST', timeout:2000}, res => {
      res.resume();
      add('hook_gateway_leak', 'Lifecycle egress outside the npm routes', 'HTTP 403', {http_status:res.statusCode}, res.statusCode === 403, 'POST carried only a fixed synthetic canary');
      resolve();
    });
    req.on('timeout', () => req.destroy(new Error('request deadline')));
    req.on('error', err => { checks.push({id:'hook_gateway_leak', label:'Lifecycle egress outside the npm routes', status:'error', expected:'HTTP 403', observed:null, detail:err.code || err.message}); resolve(); });
    req.end('SYNTHETIC_BOUNDARY_CANARY_ONLY');
  });
}
function directInternet() {
  return new Promise(resolve => {
    const control = JSON.parse(process.env.LAB_DIRECT_CONTROL_JSON);
    const socket = net.createConnection({host:control.address, port:control.port});
    let settled = false;
    function finish(code, connected) {
      if (settled) return;
      settled = true;
      socket.destroy();
      const denied = ['ENETUNREACH','EHOSTUNREACH','EACCES','EPERM'].includes(code);
      checks.push({id:'hook_direct_internet', label:'Direct Internet bypass', expected:'connection refused by the network boundary', observed:{connected,code,address:control.address,port:control.port}, status:connected ? 'failed' : (denied ? 'passed' : 'error'), detail:'Same address was reached by the trusted gateway; no application payload; timeout alone is not proof of isolation'});
      resolve();
    }
    socket.setTimeout(1500, () => finish('ETIMEDOUT', false));
    socket.on('connect', () => finish(null, true));
    socket.on('error', err => finish(err.code, false));
  });
}
function dnsAttempt(id, direct) {
  return new Promise(resolve => {
    const resolver = new dns.Resolver({timeout:1000, tries:1});
    if (direct) resolver.setServers(['1.1.1.1']);
    resolver.resolve4('registry.npmjs.org', (err, addresses) => {
      const rejected = err && ['ECONNREFUSED','ETIMEOUT','ENETUNREACH','EHOSTUNREACH','ESERVFAIL','EAI_AGAIN'].includes(err.code);
      checks.push({id, label:direct ? 'Direct public DNS bypass' : 'Worker external DNS lookup', expected:'no external DNS resolution', observed:{code:err ? err.code : null, addresses:addresses || [], servers:resolver.getServers()}, status:err ? (rejected ? 'passed' : 'error') : 'failed', detail:'Registry name is resolved independently by the functioning trusted gateway; worker uses loopback DNS and has no external route'});
      resolve();
    });
  });
}
(async () => {
  try {
    fs.writeFileSync('/work/hook-workspace.txt', 'postinstall wrote this');
    add('hook_workspace_write', 'Lifecycle workspace write control', 'write and read succeed in /work', fs.readFileSync('/work/hook-workspace.txt','utf8'), true, 'The lifecycle hook really ran and can develop inside its workspace');
    attemptFile('hook_rootfs_write', 'Lifecycle root filesystem write', '/boundary-development-rootfs', true);
    attemptFile('hook_host_canary', 'Lifecycle host canary read', process.env.LAB_HOST_CANARY_PATH, false);
    await Promise.all([leak(), directInternet(), dnsAttempt('hook_external_dns',false), dnsAttempt('hook_direct_dns',true)]);
  } catch (err) {
    checks.push({id:'hook_execution', label:'Lifecycle hook execution', expected:'all fixed lifecycle attempts complete', observed:null, status:'error', detail:err.code || err.message});
  }
  fs.writeFileSync('/work/hook-results.json', JSON.stringify(checks));
})();
'''


WORKER_TEMPLATE = r'''
const fs = require('fs'), cp = require('child_process'), http = require('http'), crypto = require('crypto');
const checks = [];
function add(id,label,expected,observed,passed,detail) {
  checks.push({id,label,expected,observed,status:passed ? 'passed':'failed',detail});
}
function measure(id,label,expected,probe) {
  try { const [passed,observed,detail] = probe(); add(id,label,expected,observed,passed,detail); }
  catch(err) { checks.push({id,label,expected,observed:null,status:'error',detail:err.code || err.message}); }
}
function getHealth() {
  return new Promise(resolve => {
    const req = http.get(new URL('/health', process.env.LAB_GATEWAY_ORIGIN), {timeout:1000}, res => {res.resume(); resolve(res.statusCode);});
    req.on('timeout', () => req.destroy(new Error('health timeout')));
    req.on('error', () => resolve(null));
  });
}
function getControl() {
  return new Promise((resolve,reject) => {
    const req=http.get(new URL('/control',process.env.LAB_GATEWAY_ORIGIN),{timeout:10000},res=>{
      let body='';
      res.on('data',chunk=>{body+=chunk; if(body.length>8192) req.destroy(new Error('control response too large'));});
      res.on('end',()=>{try { if(res.statusCode!==200) throw new Error('control HTTP '+res.statusCode); resolve(JSON.parse(body)); } catch(err) {reject(err);}});
      res.on('error',reject);
    });
    req.on('timeout',()=>req.destroy(new Error('control deadline')));
    req.on('error',reject);
  });
}
function command(command,args,timeout) {
  const output = cp.execFileSync(command,args,{cwd:'/work',encoding:'utf8',timeout,maxBuffer:1024*1024,env:{...process.env,npm_config_update_notifier:'false',npm_config_fetch_retries:'0',npm_config_fetch_timeout:'15000'}});
  if (output) console.log(output);
  return output;
}
(async () => {
  try {
    // Parent verifies actual network attachments before authorizing npm and
    // the lifecycle probes. No host mount is needed for this handshake.
    let authorized=false;
    for(let attempt=0;attempt<120;attempt++) {
      if(fs.existsSync('/work/.boundary-authorized')) {authorized=true;break;}
      await new Promise(resolve=>setTimeout(resolve,250));
    }
    if(!authorized) throw new Error('parent authorization was not received');
    fs.unlinkSync('/work/.boundary-authorized');
    measure('worker_privileges','Worker privilege boundary','uid65534, no effective capabilities, no-new-privileges, seccomp filter', () => {
      const status = Object.fromEntries(fs.readFileSync('/proc/self/status','utf8').split('\n').filter(line=>line.includes(':')).map(line=>{const at=line.indexOf(':');return [line.slice(0,at),line.slice(at+1).trim()];}));
      const observed={uid:process.getuid(),euid:process.geteuid(),gid:process.getgid(),effective_capabilities:status.CapEff,no_new_privileges:Number(status.NoNewPrivs),seccomp:Number(status.Seccomp)};
      return [observed.uid===65534 && observed.euid===65534 && observed.gid===65534 && BigInt('0x'+status.CapEff)===0n && observed.no_new_privileges===1 && observed.seccomp===2,observed,'Measured inside the npm worker'];
    });
    measure('worker_resources','Worker resource bounds','memory<=512MiB, pids<=64, CPU<=1', () => {
      const memory=Number(fs.readFileSync('/sys/fs/cgroup/memory.max','utf8').trim()), pids=Number(fs.readFileSync('/sys/fs/cgroup/pids.max','utf8').trim());
      const [quota,period]=fs.readFileSync('/sys/fs/cgroup/cpu.max','utf8').trim().split(/\s+/).map(Number);
      return [memory>0 && memory<=536870912 && pids>0 && pids<=64 && quota>0 && quota/period<=1,{memory_bytes:memory,pids,cpu_quota:quota,cpu_period:period},'Kernel cgroup measurements; no resource exhaustion attempted'];
    });
    measure('worker_root_mount','Worker root filesystem','root mount is read-only', () => {
      const root=fs.readFileSync('/proc/self/mountinfo','utf8').split('\n').map(line=>line.split(' ')).find(fields=>fields[4]==='/');
      return [root && root[5].split(',').includes('ro'),{options:root ? root[5]:null},'Actual runtime mount flags; the lifecycle hook also attempts a write'];
    });
    measure('worker_no_default_route','Worker external route','no usable IPv4 or IPv6 default route', () => {
      const v4=fs.readFileSync('/proc/net/route','utf8').trim().split('\n').slice(1).map(line=>line.trim().split(/\s+/)).filter(fields=>fields[1]==='00000000' && (parseInt(fields[3],16)&1));
      const v6=fs.existsSync('/proc/net/ipv6_route') ? fs.readFileSync('/proc/net/ipv6_route','utf8').trim().split('\n').filter(Boolean).map(line=>line.trim().split(/\s+/)).filter(fields=>fields[0]==='0'.repeat(32) && fields[1]==='00' && (parseInt(fields[8],16)&1) && !(parseInt(fields[8],16)&0x200)):[];
      return [v4.length===0 && v6.length===0,{ipv4_default_routes:v4,ipv6_default_routes:v6},'Internal subnet communication is retained; external routes are excluded'];
    });
    measure('worker_no_credentials','No host credential inheritance','no credential-like environment names or daemon socket', () => {
      const suspicious=Object.keys(process.env).filter(name=>/(TOKEN|PASSWORD|SECRET|API_KEY|CREDENTIAL)/i.test(name));
      const sockets=['/var/run/docker.sock','/run/docker.sock'].filter(path=>fs.existsSync(path));
      return [suspicious.length===0 && sockets.length===0,{credential_variable_names:suspicious,socket_paths:sockets},'Only environment names are reported; host synthetic-token environment is deliberately not forwarded'];
    });
    let health=null;
    for(let attempt=0;attempt<20;attempt++) {health=await getHealth(); if(health===200) break; await new Promise(resolve=>setTimeout(resolve,200));}
    add('gateway_health','Allowed gateway connectivity control','HTTP200 from the fixed gateway',health,health===200,'A functioning allowed route is paired with forbidden /leak and direct-egress attempts');
    if(health!==200) throw new Error('gateway readiness failed');
    const control=await getControl();
    const validControl=require('net').isIPv4(control.address) && control.port===443 && control.tcp_connected===true;
    add('gateway_internet_control','Gateway Internet positive control','gateway reached registry public IPv4 on443',control,validControl,'Direct worker probe uses this same demonstrated-reachable destination');
    if(!validControl) throw new Error('gateway did not produce a valid Internet positive control');
    process.env.LAB_DIRECT_CONTROL_JSON=JSON.stringify(control);
    fs.mkdirSync('/work/local-probe',{recursive:true});
    fs.writeFileSync('/work/local-probe/package.json',JSON.stringify({name:'boundary-local-probe',version:'1.0.0',scripts:{postinstall:'node probe.js'}}));
    fs.writeFileSync('/work/local-probe/probe.js',__HOOK_SOURCE__);
    command('npm',['pack','./local-probe','--ignore-scripts','--silent'],10000);
    fs.writeFileSync('/work/package.json',JSON.stringify({name:'boundary-development-trial',version:'1.0.0',private:true,dependencies:{'is-number':'7.0.0','boundary-local-probe':'file:./boundary-local-probe-1.0.0.tgz'}}));
    command('npm',['install','--registry',process.env.LAB_GATEWAY_ORIGIN+'/npm/','--cache','/cache','--no-audit','--no-fund','--foreground-scripts','--ignore-scripts=false'],25000);
    const hook=JSON.parse(fs.readFileSync('/work/hook-results.json','utf8'));
    if(!Array.isArray(hook)) throw new Error('missing lifecycle results');
    checks.push(...hook);
    measure('npm_dependency','Real pinned npm dependency','is-number@7.0.0 loads and distinguishes numbers', () => {
      const dependency=require('/work/node_modules/is-number'), version=require('/work/node_modules/is-number/package.json').version;
      const numeric=dependency(42), text=dependency('not-a-number');
      return [version==='7.0.0' && numeric===true && text===false,{version,numeric,text},'Installed from the trusted gateway with npm integrity checks'];
    });
    fs.writeFileSync('/work/app.js',"const fs=require('fs'); const isNumber=require('is-number'); fs.writeFileSync('/work/artifact.json', JSON.stringify({answer:42,valid:isNumber(42)}));\n");
    command(process.execPath,['--check','/work/app.js'],5000);
    command(process.execPath,['/work/app.js'],5000);
    const artifact=fs.readFileSync('/work/artifact.json','utf8');
    add('development_child_artifact','Child syntax check and build artifact','a child checks JS syntax and writes the expected artifact', {artifact:JSON.parse(artifact),sha256:crypto.createHash('sha256').update(artifact).digest('hex')}, artifact==='{"answer":42,"valid":true}', 'Actual child execution and workspace artifact; no native compiler is claimed');
    const cacheEntries=fs.readdirSync('/cache');
    add('npm_cache_write','Writable npm cache control','npm populated /cache',cacheEntries,cacheEntries.length>0,'Cache is ephemeral tmpfs, not a host bind');
  } catch(err) {
    checks.push({id:'development_execution',label:'Development scenario completed',expected:'install, lifecycle probes and artifact complete',observed:{code:err.code || null,status:err.status || null},status:'error',detail:err.message});
  }
  console.log('BOUNDARY_DEVELOPMENT_JSON:'+JSON.stringify(checks));
})();
'''
WORKER_PROGRAM = WORKER_TEMPLATE.replace("__HOOK_SOURCE__", json.dumps(HOOK_PROGRAM))
REQUIRED_WORKER_IDS = {
    "worker_privileges", "worker_resources", "worker_root_mount", "worker_no_default_route",
    "worker_no_credentials", "gateway_health", "gateway_internet_control", "hook_workspace_write", "hook_rootfs_write",
    "hook_host_canary", "hook_gateway_leak", "hook_direct_internet", "hook_external_dns",
    "hook_direct_dns", "npm_dependency", "development_child_artifact", "npm_cache_write",
}


class DockerError(RuntimeError):
    def __init__(self, detail, stdout=""):
        super().__init__(detail)
        self.stdout = stdout


def _text(value):
    if isinstance(value, bytes):
        value = value.decode("utf-8", errors="replace")
    return (value or "")[-65_536:]


def _docker(args, timeout=CLI_TIMEOUT):
    env = dict(os.environ, BOUNDARY_LAB_HOST_TOKEN="SYNTHETIC_HOST_ENV_ONLY")
    try:
        result = subprocess.run(["docker", *args], capture_output=True, text=True, encoding="utf-8",
                                errors="replace", timeout=timeout, check=False, env=env)
    except subprocess.TimeoutExpired as exc:
        raise DockerError(f"docker {args[0]} exceeded {timeout}s", _text(exc.stdout)) from exc
    except OSError as exc:
        raise DockerError(f"docker {args[0]} unavailable: {exc}") from exc
    if result.returncode:
        raise DockerError(f"docker {args[0]} exited {result.returncode}: {_text(result.stderr)}", _text(result.stdout))
    output = result.stdout + result.stderr if args[0] == "logs" else result.stdout
    return _text(output).strip()


def _check(ident, label, status, expected, observed, detail):
    return dict(id=ident, label=label, status=status, expected=expected, observed=observed, detail=detail)


def _resource_id(value):
    if not re.fullmatch(r"[a-f0-9]{64}", value):
        raise DockerError("Docker did not return a full resource ID")
    return value


def _image_id(image, command=None):
    command = command or _docker
    ident = command(["image", "inspect", "--format", "{{.Id}}", image])
    if not re.fullmatch(r"sha256:[a-f0-9]{64}", ident):
        raise DockerError("Image was not identified by a content-addressed ID")
    volumes = json.loads(command(["image", "inspect", "--format", "{{json .Config.Volumes}}", ident]))
    if volumes not in (None, {}):
        raise DockerError("Image-declared volumes are excluded; only explicit ephemeral tmpfs mounts are allowed")
    return ident


def _gateway_source():
    from .gateway import PROGRAM
    if not isinstance(PROGRAM, str) or not PROGRAM.strip():
        raise ValueError("Gateway PROGRAM is missing")
    return PROGRAM


def _gateway_ip(config):
    for entry in config:
        subnet = ipaddress.ip_network(entry["Subnet"])
        if subnet.version == 4 and subnet.num_addresses >= 8:
            # Avoid the Docker bridge gateway and reserve another address for
            # the worker. The network is fresh and uniquely owned by this run.
            for offset in range(2, min(subnet.num_addresses - 2, 16)):
                candidate = str(subnet.network_address + offset)
                if candidate != entry.get("Gateway"):
                    return candidate
    raise ValueError("The internal network has no usable IPv4 address range")


def _container_args(token, name, image, network, *, worker, origin, canary=None):
    args = ["create", "--name", f"selfware-dev-{token}-{name}", "--label", f"{LABEL}={token}",
            "--network", network, "--user", "65534:65534", "--read-only", "--cap-drop", "ALL",
            "--security-opt", "no-new-privileges:true", "--pids-limit", "64" if worker else "32",
            "--memory", "512m" if worker else "128m", "--cpus", "1" if worker else "0.5",
            "--tmpfs", "/tmp:rw,nosuid,nodev,noexec,size=64m,mode=1777",
            "--env", f"LAB_GATEWAY_ORIGIN={origin}"]
    if worker:
        args += ["--dns", "127.0.0.1", "--workdir", "/work", "--env", "HOME=/work",
                 "--env", f"LAB_HOST_CANARY_PATH={canary}",
                 "--tmpfs", "/work:rw,nosuid,nodev,size=128m,uid=65534,gid=65534,mode=0700",
                 "--tmpfs", "/cache:rw,nosuid,nodev,noexec,size=128m,uid=65534,gid=65534,mode=0700",
                 "--entrypoint", "node", image, "-e", WORKER_PROGRAM]
    else:
        args += ["--sysctl", "net.ipv4.ip_forward=0", "--sysctl", "net.ipv6.conf.all.forwarding=0",
                 "--entrypoint", "python", image, "-I", "-c", _gateway_source()]
    return args


def _owned(token, kind):
    command = ["ps", "--all"] if kind == "container" else ["network", "ls"]
    values = _docker([*command, "--no-trunc", "--filter", f"label={LABEL}={token}", "--format", "{{.ID}}"])
    return [_resource_id(value) for value in values.splitlines()] if values else []


def _labels(kind, ident):
    command = ["inspect", "--format", "{{json .Config.Labels}}", ident] if kind == "container" else ["network", "inspect", "--format", "{{json .Labels}}", ident]
    return json.loads(_docker(command))


def _cleanup(token, known):
    errors, removed, remaining = [], {"container": [], "network": []}, {}
    for kind in ("container", "network"):
        candidates = set(known[kind])
        try:
            candidates.update(_owned(token, kind))
        except (DockerError, ValueError) as exc:
            errors.append(str(exc))
        for ident in sorted(candidates):
            try:
                labels = _labels(kind, ident)
                if not isinstance(labels, dict) or labels.get(LABEL) != token:
                    errors.append(f"Refused cleanup of {kind} without exact ownership label: {ident}")
                    continue
                command = ["rm", "--force", ident] if kind == "container" else ["network", "rm", ident]
                _docker(command)
                removed[kind].append(ident)
            except (DockerError, ValueError) as exc:
                errors.append(str(exc))
        try:
            remaining[kind] = _owned(token, kind)
        except (DockerError, ValueError) as exc:
            errors.append(str(exc))
            remaining[kind] = None
    status = "error" if errors else ("failed" if any(remaining.values()) else "passed")
    return _check("development_cleanup", "Owned development resources removed", status,
                  "no owned containers or networks remain", {"removed": removed, "remaining": remaining},
                  "; ".join(errors) if errors else "Only exact-label-owned resources were removed; containers were removed before their networks")


def _worker_checks(output):
    lines = [line[len(MARKER):] for line in output.splitlines() if line.startswith(MARKER)]
    if len(lines) != 1:
        raise ValueError("Worker did not emit exactly one structured result")
    checks = json.loads(lines[0])
    if not isinstance(checks, list):
        raise ValueError("Invalid worker result schema")
    seen = set()
    for check in checks:
        if not isinstance(check, dict) or not {"id", "label", "status", "expected", "observed", "detail"}.issubset(check):
            raise ValueError("Invalid worker check schema")
        if check["id"] in seen or check["status"] not in ("passed", "failed", "error"):
            raise ValueError("Duplicate check or invalid worker status")
        seen.add(check["id"])
    if not REQUIRED_WORKER_IDS <= seen:
        checks.append(_check("worker_coverage", "Every development control completed", "error",
                             sorted(REQUIRED_WORKER_IDS), sorted(seen), "Missing controls are not counted as successful"))
    return checks


def run_development(output_dir: Path, node_image="node:22-alpine", gateway_image="python:3.12-alpine") -> dict:
    result = {"status": "error", "node_image": node_image, "gateway_image": gateway_image,
              "node_image_id": None, "gateway_image_id": None, "checks": [], "limitations": list(LIMITATIONS)}
    checks = result["checks"]
    token = uuid.uuid4().hex
    known = {"container": [], "network": []}
    attempted, canary = False, None
    gateway = None
    scenario_deadline = time.monotonic() + SCENARIO_TIMEOUT

    def experiment_command(args, timeout=CLI_TIMEOUT):
        remaining = scenario_deadline - time.monotonic()
        if remaining <= 0:
            raise DockerError("Development scenario exceeded its overall120s deadline")
        return _docker(args, timeout=min(timeout, remaining))

    canary_text = "SYNTHETIC_DEVELOPMENT_HOST_CANARY_ONLY " + token + "\n"
    try:
        output = Path(output_dir).resolve()
        output.mkdir(mode=0o700, parents=True, exist_ok=True)
        canary = output / f"host-canary-{token}.txt"
        with canary.open("x", encoding="utf-8") as handle:
            handle.write(canary_text)
        attempted = True
        node_id, gateway_id = _image_id(node_image, experiment_command), _image_id(gateway_image, experiment_command)
        result.update(node_image_id=node_id, gateway_image_id=gateway_id)
        isolated_args = [value for key, setting in ISOLATED_OPTIONS.items() for value in ("--opt", f"{key}={setting}")]
        internal = _resource_id(experiment_command(["network", "create", "--internal", *isolated_args, "--label", f"{LABEL}={token}", f"selfware-dev-{token}-internal"]))
        known["network"].append(internal)
        egress = _resource_id(experiment_command(["network", "create", "--label", f"{LABEL}={token}", f"selfware-dev-{token}-egress"]))
        known["network"].append(egress)
        address = _gateway_ip(json.loads(experiment_command(["network", "inspect", "--format", "{{json .IPAM.Config}}", internal])))
        actual_options = json.loads(experiment_command(["network", "inspect", "--format", "{{json .Options}}", internal]))
        is_internal = json.loads(experiment_command(["network", "inspect", "--format", "{{json .Internal}}", internal]))
        isolated = is_internal is True and all(actual_options.get(key) == value for key, value in ISOLATED_OPTIONS.items())
        checks.append(_check("development_internal_network", "Internal bridge configured with isolated gateway mode", "passed" if isolated else "failed",
                             "internal network with IPv4 and IPv6 gateway_mode=isolated", {"internal": is_internal, "options": actual_options},
                             "Isolated gateway mode is verified in Docker configuration; host-service reachability is not separately probed"))
        if not isolated:
            raise ValueError("Docker did not apply isolated internal-network settings")
        origin = f"http://{address}:8080"
        gateway = _resource_id(experiment_command(_container_args(token, "gateway", gateway_id, egress, worker=False, origin=origin)))
        known["container"].append(gateway)
        experiment_command(["network", "connect", "--ip", address, internal, gateway])
        experiment_command(["start", gateway])
        forwarding = json.loads(experiment_command(["exec", gateway, "python", "-I", "-c", FORWARDING_PROGRAM]))
        not_forwarding = forwarding == {"net.ipv4.ip_forward": "0", "net.ipv6.conf.all.forwarding": "0"}
        checks.append(_check("gateway_no_packet_forwarding", "Gateway cannot forward worker IP packets", "passed" if not_forwarding else "failed",
                             "IPv4 and IPv6 forwarding both0", forwarding, "Read the gateway network namespace's runtime sysctls"))
        if not not_forwarding:
            raise ValueError("Gateway packet forwarding was not disabled")
        worker = _resource_id(experiment_command(_container_args(token, "worker", node_id, internal, worker=True, origin=origin, canary=str(canary))))
        known["container"].append(worker)
        deadline = time.monotonic() + WORKER_TIMEOUT

        def worker_command(args, cap=CLI_TIMEOUT):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise DockerError("Development worker exceeded its overall 60s deadline")
            return experiment_command(args, timeout=min(cap, remaining))

        worker_command(["start", worker])
        # Verify actual attachment and DNS configuration as well as runtime
        # behavior; image defaults or an unexpected daemon state cannot hide.
        worker_networks = json.loads(worker_command(["inspect", "--format", "{{json .NetworkSettings.Networks}}", worker]))
        gateway_networks = json.loads(worker_command(["inspect", "--format", "{{json .NetworkSettings.Networks}}", gateway]))
        dns_servers = json.loads(worker_command(["inspect", "--format", "{{json .HostConfig.Dns}}", worker]))
        worker_ids = {network["NetworkID"] for network in worker_networks.values()}
        gateway_ids = {network["NetworkID"] for network in gateway_networks.values()}
        fixed_gateway = any(network["NetworkID"] == internal and network["IPAddress"] == address for network in gateway_networks.values())
        correct_topology = worker_ids == {internal} and gateway_ids == {internal, egress} and dns_servers == ["127.0.0.1"] and fixed_gateway
        checks.append(_check("development_topology", "Worker egress requires the trusted gateway", "passed" if correct_topology else "failed",
                             "worker only internal + loopback DNS; gateway internal fixed IP + egress", {"worker_network_ids": sorted(worker_ids), "gateway_network_ids": sorted(gateway_ids), "worker_dns": dns_servers, "gateway_origin": origin, "fixed_gateway_ip": fixed_gateway}, "No ports, host mounts, or daemon sockets are published"))
        if not correct_topology:
            raise ValueError("Unexpected network topology; worker was not authorized to run development code")
        worker_command(["exec", worker, "node", "-e", "require('fs').writeFileSync('/work/.boundary-authorized','run')"])
        try:
            exit_code = worker_command(["wait", worker], cap=WORKER_TIMEOUT)
            transcript = experiment_command(["logs", worker])
            if exit_code != "0":
                raise DockerError(f"Development worker exited {exit_code}", transcript)
        except DockerError as exc:
            (output / "worker.log").write_text(exc.stdout, encoding="utf-8")
            raise
        (output / "worker.log").write_text(transcript, encoding="utf-8")
        checks.extend(_worker_checks(transcript))
    except (DockerError, OSError, ValueError, TypeError, KeyError, ImportError) as exc:
        checks.append(_check("development_execution", "Development experiment completed", "error",
                             "all development controls have measured outcomes", None, str(exc)))
    finally:
        if gateway is not None:
            try:
                (output / "gateway.log").write_text(_docker(["logs", gateway]), encoding="utf-8")
            except (DockerError, OSError) as exc:
                checks.append(_check("gateway_log", "Gateway failure evidence retained", "error", "local gateway log available", None, str(exc)))
        if attempted:
            checks.append(_cleanup(token, known))
        if canary is not None:
            try:
                intact = canary.read_text(encoding="utf-8") == canary_text
                checks.append(_check("development_host_canary", "Host canary unchanged", "passed" if intact else "failed",
                                     "synthetic bytes preserved", {"intact": intact, "path": str(canary)}, "No real host secret was used"))
            except OSError as exc:
                checks.append(_check("development_host_canary", "Host canary unchanged", "error", "synthetic bytes preserved", None, str(exc)))
    statuses = {check["status"] for check in checks}
    result["status"] = "error" if "error" in statuses else ("failed" if "failed" in statuses else "passed")
    return result
