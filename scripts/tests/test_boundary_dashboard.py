"""Offline renderer regressions: safe embedding and honest interactive states."""

import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "boundary_dashboard", ROOT / "scripts/boundary_lab/dashboard.py"
)
dashboard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(dashboard)


class DashboardTests(unittest.TestCase):
    def page(self, report):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "nested/index.html"
            dashboard.render(report, output)
            return output.read_text()

    def embedded(self, page):
        payload = re.search(
            r'<script id="report-data" type="application/json">(.*?)</script>',
            page,
            re.S,
        ).group(1)
        return json.loads(payload)

    def test_optional_empty_report_remains_standalone(self):
        page = self.page({})
        self.assertEqual(self.embedded(page), {})
        self.assertNotIn("__REPORT_JSON__", page)
        self.assertNotRegex(page, r"<script[^>]+src=")
        self.assertNotRegex(page, r'<link[^>]+rel="stylesheet"')
        self.assertIn("No measured latency receipts yet", page)
        self.assertIn("No probes run in this browser", page)

    def test_hostile_receipt_cannot_close_json_script(self):
        payload = '</script><script>globalThis.injected=true</script>&\u2028\u2029'
        report = {"limitations": [payload], "endpoint": {"model": payload}}
        page = self.page(report)
        self.assertEqual(self.embedded(page), report)
        self.assertNotIn(payload, page)
        self.assertEqual(page.count("</script>"), 2)
        self.assertIn(r"\u003c/script\u003e", page)
        self.assertIn(r"\u2028", page)

    def test_nonfinite_diagnostics_become_unknown_not_invalid_json(self):
        report = {"host": None, "latency": float("nan"), "other": float("inf"),
                  "receipt_path": Path("receipts/check.json")}
        parsed = self.embedded(self.page(report))
        self.assertIsNone(parsed["latency"])
        self.assertIsNone(parsed["other"])
        self.assertEqual(parsed["receipt_path"], "receipts/check.json")

    def run_browser_logic(self, report, assertion_script):
        if not shutil.which("node"):
            self.skipTest("Node is optional for offline dashboard JavaScript checks")
        page = self.page(report)
        scripts = re.findall(r"<script(?: [^>]*)?>(.*?)</script>", page, re.S)
        # A minimal DOM allows testing the actual shipped script and event
        # handlers without fetching browser packages or running any probes.
        harness = r'''
class Element {
 constructor(id="") { this.id=id; this.textContent=""; this.children=[]; this.dataset={};
  this.attributes=new Map();if(["latency-chart","chart-legend","development-route"].includes(id))this.attributes.set("hidden","");
  this.value=["group-filter","status-filter"].includes(id)?"all":"";
  this.listeners={}; this.classList={toggle(){},add(){},remove(){}}; }
 get hidden(){return this.hasAttribute("hidden")}
 set hidden(value){if(value)this.setAttribute("hidden","");else this.removeAttribute("hidden")}
 append(...children){this.children.push(...children)}
 replaceChildren(...children){this.children=children}
 setAttribute(key,value){this.attributes.set(key,String(value))}
 removeAttribute(key){this.attributes.delete(key)}
 hasAttribute(key){return this.attributes.has(key)}
 addEventListener(event,callback){this.listeners[event]=callback}
 remove(){} click(){}
}
// SVG elements do not inherit HTMLElement.hidden's reflected attribute.
// Assignment creates an expando, leaving the CSS [hidden] selector active.
class SvgElement extends Element {
 get hidden(){return this.hiddenExpando}
 set hidden(value){this.hiddenExpando=value}
}
const nodes=new Map();
const document={getElementById(id){if(!nodes.has(id))nodes.set(id,id==="latency-chart"?new SvgElement(id):new Element(id));return nodes.get(id)},
 querySelectorAll(){return[]},createElement(){return new Element()},
 createElementNS(){return new SvgElement()},body:new Element()};
for(const id of["latency-chart","chart-legend","development-route"])document.getElementById(id);
const assert=require("node:assert/strict");
'''
        source = harness + "\ndocument.getElementById('report-data').textContent="
        source += json.dumps(json.dumps(report)) + ";\n" + scripts[-1]
        source += "\n" + assertion_script
        result = subprocess.run([shutil.which("node"), "-"], input=source,
                                text=True, capture_output=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_experiments_never_render_as_passes(self):
        self.run_browser_logic({}, r'''
const rows=nodes.get("receipt-body").children;
assert.equal(rows.length,4);
for(const row of rows)assert.equal(row.children[2].children[0].textContent,"Not run");
assert.equal(nodes.get("development-route").hasAttribute("hidden"),true);
assert(!nodes.get("group-filter").children.some(option=>option.value==="development"));
assert.equal(nodes.get("latency-chart").hasAttribute("hidden"),true);
const svg=document.createElementNS("http://www.w3.org/2000/svg","svg");
svg.setAttribute("hidden","");svg.hidden=false;
assert.equal(svg.hasAttribute("hidden"),true,"SVG mock must preserve actual attribute semantics");
assert.equal(nodes.get("equal-share").textContent,"56,250 tokens");
nodes.get("concurrency").value="1";
nodes.get("concurrency").listeners.input();
assert.equal(nodes.get("equal-share").textContent,"900,000 tokens");
nodes.get("context").value="1000000";
nodes.get("context").listeners.input();
assert.equal(nodes.get("allocation-state").textContent,"OVER ILLUSTRATED POOL");
assert.equal(nodes.get("remaining").textContent,"100,000 over pool");
''')

    def test_actual_receipt_schema_preserves_partial_and_review_states(self):
        report = {
            "host": {"memory_bytes": 103079215104, "architecture": "arm64"},
            "docker": {"memory_bytes": 8319238144, "cpu_count": 12},
            "endpoint": {"model": "fixture-model", "max_model_len": 1000000,
                         "models": [{"id": "fixture-model", "owned_by": "sglang"}]},
            "experiments": {
                "endpoint": {"status": "partial", "levels": [
                    {"concurrency": 2, "p50_ms": 100, "p95_ms": 100,
                     "receipts": [{"status": "completed", "latency_ms": 100},
                                  {"status": "error", "latency_ms": 9000}]}
                ]},
                "policy": {"checks": [
                    {"label": "Needs an independent oracle", "status": "needs_review"},
                    {"label": "Named successful control", "status": "passed"},
                    {"label": "Conflicting result", "status": "error", "passed": True}
                ]},
                "proposals": {"status": "invalid_proposals", "error": "Invalid schema"},
            },
        }
        self.run_browser_logic(report, r'''
assert.equal(nodes.get("host-memory").textContent,"96 GiB");
assert.equal(nodes.get("docker-memory").textContent,"7.75 GiB");
assert.equal(nodes.get("map-endpoint").textContent,"sglang");
assert.equal(nodes.get("sample-count").textContent,"1");
const rows=nodes.get("receipt-body").children;
const states=rows.map(row=>row.children[2].children[0]);
assert(states.some(v=>v.textContent==="needs review"&&v.className==="badge review"));
assert(states.some(v=>v.textContent==="partial"&&v.className==="badge review"));
assert(states.some(v=>v.textContent==="invalid proposals"&&v.className==="badge failed"));
assert(states.some(v=>v.textContent==="Passed"&&v.className==="badge passed"));
const conflict=rows.find(row=>row.children[1].children[0].textContent==="Conflicting result");
assert.equal(conflict.children[2].children[0].className,"badge failed");
nodes.get("search").value="not present in receipts";
nodes.get("search").listeners.input();
assert.equal(nodes.get("receipt-body").children[0].children[0].textContent,"No receipts match these filters.");
''')

    def test_saved_five_level_report_is_charted_and_paginated(self):
        # Receipt shape and latency values from the saved 2026-09-10 run.
        # Keeping the projection here makes the regression portable to CI;
        # tests never depend on that run's /tmp directory or a live endpoint.
        observed = [
            (1, 564.664, 564.664, [564.664]),
            (2, 724.342, 736.534, [736.534, 724.342]),
            (4, 729.412, 761.054, [692.395, 729.412, 749.912, 761.054]),
            (8, 785.205, 828.2, [713.491, 761.22, 828.2, 785.205,
                               808.596, 809.311, 808.815, 784.295]),
            (16, 811.183, 865.178, [865.178, 863.966, 814.912, 862.886,
                                  815.133, 764.861, 790.16, 778.175,
                                  820.214, 760.991, 811.183, 786.843,
                                  768.418, 831.709, 779.757, 819.629]),
        ]
        levels = [
            {"concurrency": concurrency, "client_peak_active_calls": concurrency,
             "completed": concurrency, "failed": 0, "p50_ms": p50, "p95_ms": p95,
             "receipts": [{"status": "completed", "latency_ms": latency,
                           "usage": {"prompt_tokens": 51, "completion_tokens": 19},
                           "positive_control": {"expected": "synthetic echo", "matched": True},
                           "metadata": {"unrelated": {"nested": "not a check"}}}
                          for latency in latencies]}
            for concurrency, p50, p95, latencies in observed
        ]
        report = {"status": "completed_with_findings", "experiments": {
            "endpoint": {"status": "completed", "levels": levels},
            "docker": {"status": "passed", "checks": [
                {"id": f"docker-{n}", "status": "passed", "observed": {"value": True}}
                for n in range(15)]},
            "policy": {"status": "passed", "checks": [
                {"id": f"policy-{n}", "status": "passed", "arguments": {"path": "fixture"}}
                for n in range(15)]},
            "proposals": {"status": "invalid_proposals", "proposals": [],
                          "error": {"kind": "invalid_proposals", "message": "schema mismatch"},
                          "receipts": [{"status": "completed", "content":
                                        "large diagnostic " * 2000 + "diagnostic-tail-marker"}]},
        }}
        self.run_browser_logic(report, r'''
assert.equal(nodes.get("levels-count").textContent,"5");
assert.equal(nodes.get("sample-count").textContent,"31");
assert.equal(nodes.get("latency-chart").hasAttribute("hidden"),false,"measured SVG must escape CSS [hidden]");
assert.equal(nodes.get("chart-empty").hidden,true);
assert.equal(nodes.get("page-count").textContent,"1–12 of 71");
let visited=0;
for(let page=0;page<6;page++){
 const rows=nodes.get("receipt-body").children;
 assert(rows.length<=12);
 visited+=rows.length;
 for(const row of rows){
  assert(row.children[1].children[1].textContent.length<=350);
  assert(!row.children[1].children[0].textContent.includes(" / usage"));
  assert(!row.children[1].children[0].textContent.includes(" / metadata"));
 }
 if(page<5)nodes.get("page-next").listeners.click();
}
assert.equal(visited,71);
assert.equal(nodes.get("page-next").disabled,true);
nodes.get("search").value="diagnostic-tail-marker";
nodes.get("search").listeners.input();
assert.equal(nodes.get("page-count").textContent,"1–2 of 2");
assert.equal(nodes.get("page-prev").disabled,true);
assert(nodes.get("receipt-body").children[1].children[1].children[2].children[1].textContent.includes("diagnostic-tail-marker"));
''')

    def test_optional_development_receipts_and_network_route(self):
        # Synthetic presentation fixture only: none of these checks execute.
        report = {"experiments": {"development": {
            "status": "partial",
            "checks": [
                {"id": "workspace", "status": "passed", "expected": "writable",
                 "observed": "writable", "detail": "Synthetic workspace control"},
                {"id": "gateway_route", "status": "failed", "expected": "fixed gateway",
                 "observed": "unavailable", "detail": "Synthetic negative result"},
                {"id": "gpu_plan", "status": "not_run", "expected": "future deployment",
                 "observed": None, "detail": "Planning does not execute a GPU workload"},
            ],
            "limitations": ["Synthetic fixture is not runtime evidence.",
                            "<b>Receipt text stays data.</b>"],
        }}}
        self.run_browser_logic(report, r'''
assert.equal(nodes.get("development-route").hasAttribute("hidden"),false);
assert.equal(nodes.get("development-status").className,"badge review");
assert.equal(nodes.get("development-gpu-state").className,"badge notrun");
assert.equal(nodes.get("development-gpu-state").textContent,"GPU deployment plan: not run");
assert(nodes.get("development-check-count").textContent.startsWith("3 supplied checks."));
assert(nodes.get("group-filter").children.some(option=>option.value==="development"));
nodes.get("development-gateway").listeners.click();
assert(nodes.get("development-description").textContent.includes("fixed npm registry policy"));
assert.equal(nodes.get("development-gateway").attributes.get("aria-pressed"),"true");
nodes.get("development-evidence").listeners.click();
assert.equal(nodes.get("group-filter").value,"development");
const rows=nodes.get("receipt-body").children;
assert.equal(rows.length,4,"one supplied stage plus exactly three supplied checks");
assert(rows.every(row=>row.children[0].textContent==="development"));
const workspace=rows.find(row=>row.children[1].children[0].textContent==="workspace");
assert.equal(workspace.children[2].children[0].className,"badge passed");
const original=JSON.parse(workspace.children[1].children[2].children[1].textContent);
assert.equal(original.expected,"writable");assert.equal(original.observed,"writable");
assert.equal(original.detail,"Synthetic workspace control");
const gpu=rows.find(row=>row.children[1].children[0].textContent==="gpu_plan");
assert.equal(gpu.children[2].children[0].className,"badge notrun");
assert(nodes.get("limitations").children.some(item=>item.textContent==="Development: Synthetic fixture is not runtime evidence."));
assert(nodes.get("limitations").children.some(item=>item.textContent==="Development: <b>Receipt text stays data.</b>"));
''')

    def test_unrun_development_does_not_fabricate_checks(self):
        self.run_browser_logic({"experiments": {"development": {"status": "not_run"}}}, r'''
assert.equal(nodes.get("development-status").className,"badge notrun");
assert(nodes.get("development-check-count").textContent.startsWith("0 supplied checks."));
nodes.get("development-evidence").listeners.click();
const rows=nodes.get("receipt-body").children;
assert.equal(rows.length,1);
assert.equal(rows[0].children[2].children[0].className,"badge notrun");
assert.equal(nodes.get("development-gpu-state").className,"badge notrun");
''')


if __name__ == "__main__":
    unittest.main()
