const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const { pathToFileURL } = require('node:url');

const {
  enforceRequestPolicy,
  isWorkspaceFileUrl,
  validateUrl,
} = require('../playwright-bridge.js');

function fakeNavigationRoute(url) {
  const calls = [];
  const page = {};
  const frame = { page: () => page };
  page.mainFrame = () => frame;
  return {
    calls,
    request() {
      return {
        url: () => url,
        frame: () => frame,
        isNavigationRequest: () => true,
      };
    },
    async abort(reason) {
      calls.push(['abort', reason]);
    },
    async continue() {
      calls.push(['continue']);
    },
  };
}

async function withWorkspaceFixture(callback) {
  const fixture = fs.mkdtempSync(path.join(os.tmpdir(), 'selfware-bridge-policy-'));
  const workspace = path.join(fixture, 'workspace');
  const outside = path.join(fixture, 'outside.txt');
  fs.mkdirSync(workspace);
  fs.writeFileSync(path.join(workspace, 'index.html'), '<a href="../outside.txt">escape</a>');
  fs.writeFileSync(outside, 'outside workspace');
  const previousRoot = process.env.SELFWARE_WORKSPACE_ROOT;
  const previousPrivate = process.env.SELFWARE_ALLOW_PRIVATE_NETWORK;
  process.env.SELFWARE_WORKSPACE_ROOT = workspace;
  delete process.env.SELFWARE_ALLOW_PRIVATE_NETWORK;
  try {
    await callback({ workspace, outside });
  } finally {
    if (previousRoot === undefined) delete process.env.SELFWARE_WORKSPACE_ROOT;
    else process.env.SELFWARE_WORKSPACE_ROOT = previousRoot;
    if (previousPrivate === undefined) delete process.env.SELFWARE_ALLOW_PRIVATE_NETWORK;
    else process.env.SELFWARE_ALLOW_PRIVATE_NETWORK = previousPrivate;
    fs.rmSync(fixture, { recursive: true, force: true });
  }
}

test('workspace file requests remain allowed', { concurrency: false }, async () => {
  await withWorkspaceFixture(async ({ workspace }) => {
    const url = pathToFileURL(path.join(workspace, 'index.html')).href;
    assert.equal(isWorkspaceFileUrl(url), true);
    assert.doesNotThrow(() => validateUrl(url));

    const route = fakeNavigationRoute(url);
    await enforceRequestPolicy(route);
    assert.deepEqual(route.calls, [['continue']]);
  });
});

test('link navigation cannot escape to an outside file', { concurrency: false }, async () => {
  await withWorkspaceFixture(async ({ outside }) => {
    // This route has the same shape Playwright emits when a click, redirect,
    // history traversal, frame, popup, or page.evaluate changes location.
    const url = pathToFileURL(outside).href;
    const route = fakeNavigationRoute(url);
    await enforceRequestPolicy(route);
    assert.deepEqual(route.calls, [['abort', 'blockedbyclient']]);
    assert.throws(() => validateUrl(url), /only inside the current workspace/);
  });
});

test('a workspace symlink cannot alias an outside file', {
  concurrency: false,
  skip: process.platform === 'win32',
}, async () => {
  await withWorkspaceFixture(async ({ workspace, outside }) => {
    const alias = path.join(workspace, 'outside-alias.txt');
    fs.symlinkSync(outside, alias);
    const url = pathToFileURL(alias).href;
    assert.equal(isWorkspaceFileUrl(url), false);

    const route = fakeNavigationRoute(url);
    await enforceRequestPolicy(route);
    assert.deepEqual(route.calls, [['abort', 'blockedbyclient']]);
  });
});

test('browser-initiated unsupported schemes fail closed', { concurrency: false }, async () => {
  await withWorkspaceFixture(async () => {
    for (const url of ['data:text/html,hello', 'ftp://example.com/file']) {
      const route = fakeNavigationRoute(url);
      await enforceRequestPolicy(route);
      assert.deepEqual(route.calls, [['abort', 'blockedbyclient']], url);
      assert.throws(() => validateUrl(url), /Unsupported URL scheme/, url);
    }
  });
});

test('IPv4-mapped IPv6 cannot bypass private and special-address blocking', {
  concurrency: false,
}, async () => {
  await withWorkspaceFixture(async () => {
    const blocked = [
      '0.0.0.1',
      '10.1.2.3',
      '100.64.0.1',
      '127.0.0.1',
      '169.254.169.254',
      '172.16.0.1',
      '192.168.0.1',
      '224.0.0.1',
    ];

    for (const ipv4 of blocked) {
      const url = `http://[::ffff:${ipv4}]/metadata`;
      assert.throws(
        () => validateUrl(url),
        /Blocked request to private\/internal address/,
        ipv4,
      );

      const route = fakeNavigationRoute(url);
      await enforceRequestPolicy(route);
      assert.deepEqual(route.calls, [['abort', 'blockedbyclient']], ipv4);
    }

    // Exercise the fully expanded spelling as well as WHATWG's compressed
    // canonical form used by the route policy.
    const expanded = 'http://[0:0:0:0:0:ffff:a9fe:a9fe]/latest/meta-data/';
    assert.throws(() => validateUrl(expanded), /Blocked request/);
  });
});

test('public IPv4-mapped IPv6 remains allowed', { concurrency: false }, async () => {
  await withWorkspaceFixture(async () => {
    const url = 'https://[::ffff:8.8.8.8]/';
    assert.doesNotThrow(() => validateUrl(url));

    const route = fakeNavigationRoute(url);
    await enforceRequestPolicy(route);
    assert.deepEqual(route.calls, [['continue']]);
  });
});
