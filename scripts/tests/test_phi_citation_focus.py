#!/usr/bin/env python3
"""Exact citation mapping contracts against the production JavaScript module.

Node evaluates the same exported resolver used in the browser. These tests
verify source locations, not semantic truth of model-generated explanations.
"""
import json
from pathlib import Path
import shutil
import subprocess
import unittest

WORKSPACE_MODULE = Path(__file__).resolve().parents[2] / 'src/evolve/web/phi/phi_workspace.js'


class PhiCitationFocusTests(unittest.TestCase):
    def node(self, assertions):
        if not shutil.which('node'):
            self.skipTest('Node is required for actual citation-module tests')
        program = '''
import assert from 'node:assert/strict';
import {focusForClaim, resolveEvidence, WorkspaceError} from MODULE;
const source = [
  '// Outside the supplied citation.',
  'pub fn mean(values: &[f64]) -> Option<f64> {',
  '    if values.is_empty() {',
  '        return None;',
  '    }',
  '    let total = values.iter().sum::<f64>();',
  '    Some(total / values.len() as f64)',
  '}',
  'fn outside_token() {}'
];
function fixture(lines=source, first=2, last=8) {
  const document={path:'src/stats.rs',hash:'checked-snapshot',content:lines.join('\\n')};
  const evidence={path:document.path,content_hash:document.hash,start_line:first,end_line:last,
    excerpt:lines.slice(first-1,last).map((line,i)=>String(first+i).padStart(6)+' | '+line).join('\\n')};
  return {document,evidence,fallback:resolveEvidence(evidence,document)};
}
const {document,evidence,fallback}=fixture();
'''.replace('MODULE', json.dumps(WORKSPACE_MODULE.as_uri())) + assertions
        result = subprocess.run(['node', '--input-type=module', '-'], input=program,
                                text=True, capture_output=True, timeout=20)
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_named_source_line_points_to_that_whole_checked_line(self):
        self.node('''
for (const claim of ['On line 6, the iterator computes the total.', 'LINE 6 computes the total.']) {
  assert.deepEqual(focusForClaim(claim,evidence,document),{
    line:6,endLine:6,start:0,end:source[5].length,kind:'named source line'});
}
''')

    def test_named_inclusive_ranges_include_the_whole_final_short_line(self):
        self.node('''
for (const separator of ['–','-',' to ',' through ']) {
  const result=focusForClaim(`Lines 3${separator}5 handle an empty collection.`,evidence,document);
  assert.deepEqual(result,{line:3,endLine:5,start:0,end:source[4].length,kind:'named source line'});
  assert.equal(source[result.endLine-1].slice(0,result.end),'    }');
}
''')

    def test_named_lines_and_fragments_cannot_escape_supplied_evidence(self):
        self.node('''
for (const claim of ['On line 1, an introduction appears.','Line 9 defines another function.',
  'Lines 1–3 explain the flow.','Lines 7 through 9 finish the flow.',
  'Lines 7 to 3 describe the function.','Line 0 is relevant.','Line 9999999 is relevant.',
  'The `outside_token` helper is relevant.']) {
  assert.deepEqual(focusForClaim(claim,evidence,document),fallback,claim);
}
// An invalid named line may still have an exact in-citation source fragment;
// even that refinement must remain inside the checked source range.
const bounded=focusForClaim('On line 99, `return None` is mentioned.',evidence,document);
assert.equal(bounded.line,4);assert.equal(bounded.start,8);assert.equal(bounded.end,19);
''')

    def test_ambiguous_fragments_fall_back_but_unique_quotes_are_exact(self):
        self.node('''
for (const text of ['The `values` input is used.', 'The "total" variable is computed.']) {
  assert.deepEqual(focusForClaim(text,evidence,document),fallback);
}
for (const text of ['The `return None` branch is explicit.', 'The "return None" branch is explicit.']) {
  const result=focusForClaim(text,evidence,document);
  assert.deepEqual(result,{line:4,start:8,end:19,kind:'quoted fragment'});
  assert.equal(source[result.line-1].slice(result.start,result.end),'return None');
}
const sameLine=fixture(['fn demo() { total + total }'],1,1);
assert.deepEqual(focusForClaim('Use `total`.',sameLine.evidence,sameLine.document),sameLine.fallback);
''')

    def test_unquoted_declaration_phrases_point_to_exact_source_text(self):
        self.node('''
const result=focusForClaim('The pub fn mean returns an optional average.',evidence,document);
assert.equal(result.line,2);assert.equal(result.start,4);assert.equal(result.end,11);
assert.equal(source[result.line-1].slice(result.start,result.end),'fn mean');
for(const [declaration,line] of [
  ['function render','export function render() {}'],
  ['def render','def render(value):'],
  ['class Renderer','class Renderer:'],
  ['struct Renderer','pub struct Renderer {'],
  ['enum State','enum State {']
]) {
  const f=fixture(['// preface',line,'}'],2,3);
  const focused=focusForClaim(`The ${declaration} declaration defines the type or operation.`,f.evidence,f.document);
  assert.equal(focused.line,2);
  assert.equal(line.slice(focused.start,focused.end),declaration);
}
''')

    def test_declaration_name_prefix_is_not_an_exact_symbol_match(self):
        self.node('''
for(const suffix of ['_squared','ing','2','é']) {
  const f=fixture([`pub fn mean${suffix}() {`,'    0','}'],1,3);
  assert.deepEqual(focusForClaim('The pub fn mean returns a number.',f.evidence,f.document),f.fallback);
}
const keywordPrefix=fixture(['notfn mean() {','    0','}'],1,3);
assert.deepEqual(focusForClaim('The fn mean returns a number.',keywordPrefix.evidence,keywordPrefix.document),keywordPrefix.fallback);
const javascript=fixture(['function mean$() {','    return 0;','}'],1,3);
assert.deepEqual(focusForClaim('The function mean returns a number.',javascript.evidence,javascript.document),javascript.fallback);
// Explicit quoted substrings remain valid locations without asserting a symbol.
const quoted=fixture(['pub fn mean_squared() {','    0','}'],1,3);
assert.deepEqual(focusForClaim('The fragment `fn mean` appears.',quoted.evidence,quoted.document),
  {line:1,start:4,end:11,kind:'quoted fragment'});
''')

    def test_stale_hash_path_or_excerpt_rejects_before_any_refinement(self):
        self.node('''
for (const claim of ['On line 6, the total is computed.', 'The pub fn mean returns a value.', 'The `return None` branch is explicit.']) {
  assert.throws(()=>focusForClaim(claim,evidence,{...document,hash:'new-snapshot'}),WorkspaceError);
  assert.throws(()=>focusForClaim(claim,evidence,{...document,path:'src/another.rs'}),WorkspaceError);
  assert.throws(()=>focusForClaim(claim,{...evidence,excerpt:evidence.excerpt+' altered'},document),WorkspaceError);
}
''')


if __name__ == '__main__':
    unittest.main()
