// Run with `npm test` (compiles, then `node --test out/test/`).
import { test } from 'node:test';
import * as assert from 'node:assert/strict';

import { buildCsp, makeNonce, serializeForHtmlScript } from '../htmlSafe';
import { renderWebviewHtml } from '../webview';
import type { CodeGraph } from '../contextManager';

const HOSTILE = '</script><script>alert(1)</script><!-- & \u2028\u2029 >';

test('serializeForHtmlScript leaves no HTML-significant characters', () => {
    const out = serializeForHtmlScript({ label: HOSTILE });
    for (const ch of ['<', '>', '&', '\u2028', '\u2029']) {
        assert.ok(!out.includes(ch), `serialized JSON still contains ${JSON.stringify(ch)}`);
    }
    assert.deepEqual(JSON.parse(out), { label: HOSTILE });
});

test('serializeForHtmlScript maps undefined to null', () => {
    assert.equal(serializeForHtmlScript(undefined), 'null');
});

test('makeNonce is long and varies', () => {
    const a = makeNonce();
    const b = makeNonce();
    assert.match(a, /^[A-Za-z0-9]{32}$/);
    assert.notEqual(a, b);
});

test('buildCsp only allows nonce scripts', () => {
    const csp = buildCsp('vscode-resource:', 'abc');
    assert.ok(csp.includes("default-src 'none'"));
    assert.ok(csp.includes("script-src 'nonce-abc'"));
    assert.ok(!csp.includes('unsafe-inline'));
    assert.ok(!csp.includes('unsafe-eval'));
});

test('a hostile graph label cannot break out of the data block', () => {
    const graph = {
        nodes: [{ id: 'n1', label: HOSTILE, kind: 'file', tokens: 10, fusion_group: HOSTILE }],
        edges: [],
    } as unknown as CodeGraph;
    const html = renderWebviewHtml('vscode-resource:', 'NONCE123', graph, [HOSTILE], {
        used: 1,
        total: 2,
        percent: 50,
    });

    // Exactly our two script elements (data block + app), both nonce-tagged.
    assert.equal((html.match(/<script/g) || []).length, 2);
    assert.equal((html.match(/<script[^>]*nonce="NONCE123"/g) || []).length, 2);
    assert.ok(!html.includes('alert(1)</script>'));
    assert.ok(html.includes('Content-Security-Policy'));

    // The data block round-trips to the original values.
    const m = html.match(
        /<script type="application\/json" id="selfware-data" nonce="NONCE123">([\s\S]*?)<\/script>/
    );
    assert.ok(m, 'data block present');
    const data = JSON.parse(m![1]);
    assert.equal(data.graph.nodes[0].label, HOSTILE);
    assert.deepEqual(data.contextNodeIds, [HOSTILE]);
});
