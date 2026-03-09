#!/usr/bin/env node
// Playwright Bridge for Selfware
//
// A long-running Node.js process that accepts JSON commands on stdin
// and returns results on stdout. Uses newline-delimited JSON (NDJSON).
//
// Protocol: Each line on stdin is a JSON object with fields:
//   { "id": <number>, "action": <string>, ...params }
// Each line on stdout is a JSON response:
//   { "id": <number>, "success": <bool>, "result": <any>, "error": <string|null> }
//
// Lifecycle:
//   - The bridge launches a Playwright browser on first use (lazy init).
//   - It maintains multiple pages (tabs) indexed from 0.
//   - The "shutdown" action closes everything and exits.

const readline = require('readline');

let browser = null;
let context = null;
let pages = [];
let currentTabIndex = 0;

const MAX_PAGES = 5;
const DEFAULT_TIMEOUT = 30000;
const NAV_RATE_LIMIT_MS = 1000;
let lastNavTime = 0;

// ---- Helpers ----

function respond(id, success, result, error) {
  const resp = JSON.stringify({ id, success, result: result ?? null, error: error ?? null });
  process.stdout.write(resp + '\n');
}

async function ensureBrowser() {
  if (browser) return;
  let pw;
  try {
    pw = require('playwright');
  } catch (_) {
    try {
      pw = require('playwright-core');
    } catch (_) {
      throw new Error('Playwright not installed. Run: npm install playwright');
    }
  }
  browser = await pw.chromium.launch({ headless: true });
  context = await browser.newContext();
  const page = await context.newPage();
  pages = [page];
  currentTabIndex = 0;
}

function currentPage() {
  if (pages.length === 0) throw new Error('No pages open');
  if (currentTabIndex < 0 || currentTabIndex >= pages.length) {
    throw new Error(`Invalid tab index ${currentTabIndex}, have ${pages.length} tabs`);
  }
  return pages[currentTabIndex];
}

async function enforceNavRateLimit() {
  const now = Date.now();
  const elapsed = now - lastNavTime;
  if (elapsed < NAV_RATE_LIMIT_MS) {
    await new Promise(resolve => setTimeout(resolve, NAV_RATE_LIMIT_MS - elapsed));
  }
  lastNavTime = Date.now();
}

function validateUrl(url) {
  if (!url || typeof url !== 'string') throw new Error('URL is required');
  const parsed = new URL(url);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:' && parsed.protocol !== 'data:') {
    // Allow data: for blank pages; block file:// etc.
    if (parsed.protocol === 'file:') {
      throw new Error('file:// URLs are blocked for security');
    }
  }
  // Block common private IPs unless SELFWARE_ALLOW_PRIVATE_NETWORK=1
  if (process.env.SELFWARE_ALLOW_PRIVATE_NETWORK !== '1') {
    const host = parsed.hostname;
    if (host === 'localhost' || host === '127.0.0.1' || host === '::1' ||
        host.startsWith('10.') || host.startsWith('192.168.') ||
        /^172\.(1[6-9]|2\d|3[01])\./.test(host) ||
        host === '0.0.0.0' || host === '169.254.' || host.startsWith('169.254.')) {
      throw new Error(`Blocked request to private/internal address: ${host}`);
    }
  }
}

// ---- Action handlers ----

const handlers = {
  // -- Navigation --
  async goto(cmd) {
    validateUrl(cmd.url);
    await enforceNavRateLimit();
    const page = currentPage();
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    const response = await page.goto(cmd.url, {
      timeout,
      waitUntil: cmd.wait_until || 'load',
    });
    return {
      url: page.url(),
      status: response ? response.status() : null,
      ok: response ? response.ok() : null,
    };
  },

  async back(_cmd) {
    await currentPage().goBack({ timeout: DEFAULT_TIMEOUT });
    return { url: currentPage().url() };
  },

  async forward(_cmd) {
    await currentPage().goForward({ timeout: DEFAULT_TIMEOUT });
    return { url: currentPage().url() };
  },

  async reload(cmd) {
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().reload({ timeout });
    return { url: currentPage().url() };
  },

  async wait_for(cmd) {
    const page = currentPage();
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    if (cmd.selector) {
      await page.waitForSelector(cmd.selector, { timeout, state: cmd.state || 'visible' });
      return { waited_for: 'selector', selector: cmd.selector };
    } else if (cmd.url) {
      await page.waitForURL(cmd.url, { timeout });
      return { waited_for: 'url', url: cmd.url };
    } else if (cmd.load_state) {
      await page.waitForLoadState(cmd.load_state, { timeout });
      return { waited_for: 'load_state', state: cmd.load_state };
    } else {
      throw new Error('wait_for requires selector, url, or load_state');
    }
  },

  // -- Interaction --
  async click(cmd) {
    if (!cmd.selector) throw new Error('selector is required for click');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().click(cmd.selector, {
      timeout,
      button: cmd.button || 'left',
      clickCount: cmd.click_count || 1,
    });
    return { clicked: cmd.selector };
  },

  async type(cmd) {
    if (!cmd.selector) throw new Error('selector is required for type');
    if (cmd.text == null) throw new Error('text is required for type');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().type(cmd.selector, cmd.text, { timeout, delay: cmd.delay || 0 });
    return { typed: cmd.text, into: cmd.selector };
  },

  async fill(cmd) {
    if (!cmd.selector) throw new Error('selector is required for fill');
    if (cmd.text == null) throw new Error('text is required for fill');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().fill(cmd.selector, cmd.text, { timeout });
    return { filled: cmd.selector, with: cmd.text };
  },

  async select(cmd) {
    if (!cmd.selector) throw new Error('selector is required for select');
    if (cmd.value == null && cmd.values == null) throw new Error('value or values is required for select');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    const values = cmd.values || [cmd.value];
    const selected = await currentPage().selectOption(cmd.selector, values, { timeout });
    return { selected, selector: cmd.selector };
  },

  async check(cmd) {
    if (!cmd.selector) throw new Error('selector is required for check');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().check(cmd.selector, { timeout });
    return { checked: cmd.selector };
  },

  async uncheck(cmd) {
    if (!cmd.selector) throw new Error('selector is required for uncheck');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().uncheck(cmd.selector, { timeout });
    return { unchecked: cmd.selector };
  },

  async hover(cmd) {
    if (!cmd.selector) throw new Error('selector is required for hover');
    const timeout = cmd.timeout_ms || DEFAULT_TIMEOUT;
    await currentPage().hover(cmd.selector, { timeout });
    return { hovered: cmd.selector };
  },

  async press(cmd) {
    if (!cmd.key) throw new Error('key is required for press');
    const page = currentPage();
    if (cmd.selector) {
      await page.press(cmd.selector, cmd.key, { timeout: cmd.timeout_ms || DEFAULT_TIMEOUT });
    } else {
      await page.keyboard.press(cmd.key);
    }
    return { pressed: cmd.key };
  },

  // -- Content extraction --
  async text(cmd) {
    if (!cmd.selector) throw new Error('selector is required for text');
    const page = currentPage();
    if (cmd.all) {
      const elements = await page.locator(cmd.selector).allTextContents();
      return { texts: elements, count: elements.length };
    }
    const text = await page.textContent(cmd.selector, { timeout: cmd.timeout_ms || DEFAULT_TIMEOUT });
    return { text };
  },

  async html(cmd) {
    if (!cmd.selector) throw new Error('selector is required for html');
    const page = currentPage();
    if (cmd.outer) {
      const html = await page.locator(cmd.selector).first().evaluate(el => el.outerHTML);
      return { html };
    }
    const html = await page.innerHTML(cmd.selector, { timeout: cmd.timeout_ms || DEFAULT_TIMEOUT });
    return { html };
  },

  async attribute(cmd) {
    if (!cmd.selector) throw new Error('selector is required for attribute');
    if (!cmd.name) throw new Error('name is required for attribute');
    const value = await currentPage().getAttribute(cmd.selector, cmd.name, {
      timeout: cmd.timeout_ms || DEFAULT_TIMEOUT,
    });
    return { attribute: cmd.name, value };
  },

  async value(cmd) {
    if (!cmd.selector) throw new Error('selector is required for value');
    const val = await currentPage().inputValue(cmd.selector, {
      timeout: cmd.timeout_ms || DEFAULT_TIMEOUT,
    });
    return { value: val };
  },

  async count(cmd) {
    if (!cmd.selector) throw new Error('selector is required for count');
    const count = await currentPage().locator(cmd.selector).count();
    return { count, selector: cmd.selector };
  },

  async visible(cmd) {
    if (!cmd.selector) throw new Error('selector is required for visible');
    const visible = await currentPage().isVisible(cmd.selector);
    return { visible, selector: cmd.selector };
  },

  // -- Page info --
  async title(_cmd) {
    const title = await currentPage().title();
    return { title };
  },

  async url(_cmd) {
    return { url: currentPage().url() };
  },

  async screenshot(cmd) {
    const page = currentPage();
    const opts = {};
    if (cmd.path) opts.path = cmd.path;
    if (cmd.full_page) opts.fullPage = true;
    if (cmd.selector) {
      const buffer = await page.locator(cmd.selector).screenshot(opts);
      return {
        path: cmd.path || null,
        size: buffer.length,
        base64: cmd.path ? null : buffer.toString('base64'),
      };
    }
    const buffer = await page.screenshot(opts);
    return {
      path: cmd.path || null,
      size: buffer.length,
      base64: cmd.path ? null : buffer.toString('base64'),
    };
  },

  async pdf(cmd) {
    const page = currentPage();
    const opts = {};
    if (cmd.path) opts.path = cmd.path;
    if (cmd.format) opts.format = cmd.format;
    const buffer = await page.pdf(opts);
    return {
      path: cmd.path || null,
      size: buffer.length,
    };
  },

  // -- JavaScript --
  async evaluate(cmd) {
    if (cmd.expression == null) throw new Error('expression is required for evaluate');
    const page = currentPage();
    const result = await page.evaluate(cmd.expression);
    return { result };
  },

  async evaluate_handle(cmd) {
    if (cmd.expression == null) throw new Error('expression is required for evaluate_handle');
    const page = currentPage();
    const handle = await page.evaluateHandle(cmd.expression);
    const json = await handle.jsonValue().catch(() => '<non-serializable>');
    await handle.dispose();
    return { result: json };
  },

  // -- Multi-tab --
  async new_tab(cmd) {
    if (pages.length >= MAX_PAGES) {
      throw new Error(`Maximum ${MAX_PAGES} tabs reached`);
    }
    const page = await context.newPage();
    pages.push(page);
    currentTabIndex = pages.length - 1;
    if (cmd.url) {
      validateUrl(cmd.url);
      await enforceNavRateLimit();
      await page.goto(cmd.url, { timeout: cmd.timeout_ms || DEFAULT_TIMEOUT });
    }
    return { tab_index: currentTabIndex, total_tabs: pages.length };
  },

  async switch_tab(cmd) {
    if (cmd.tab_index == null) throw new Error('tab_index is required for switch_tab');
    if (cmd.tab_index < 0 || cmd.tab_index >= pages.length) {
      throw new Error(`tab_index ${cmd.tab_index} out of range [0, ${pages.length - 1}]`);
    }
    currentTabIndex = cmd.tab_index;
    return { tab_index: currentTabIndex, url: currentPage().url() };
  },

  async close_tab(_cmd) {
    if (pages.length <= 1) throw new Error('Cannot close the last tab');
    const page = pages[currentTabIndex];
    await page.close();
    pages.splice(currentTabIndex, 1);
    if (currentTabIndex >= pages.length) currentTabIndex = pages.length - 1;
    return { closed: true, tab_index: currentTabIndex, total_tabs: pages.length };
  },

  async list_tabs(_cmd) {
    const tabs = pages.map((p, i) => ({
      index: i,
      url: p.url(),
      active: i === currentTabIndex,
    }));
    return { tabs, current: currentTabIndex };
  },

  // -- Lifecycle --
  async shutdown(_cmd) {
    if (browser) {
      await browser.close().catch(() => {});
      browser = null;
      context = null;
      pages = [];
    }
    return { shutdown: true };
  },
};

// ---- Main loop ----

const rl = readline.createInterface({ input: process.stdin, terminal: false });

rl.on('line', async (line) => {
  let cmd;
  try {
    cmd = JSON.parse(line);
  } catch (e) {
    respond(null, false, null, `Invalid JSON: ${e.message}`);
    return;
  }

  const id = cmd.id ?? null;
  const action = cmd.action;

  if (!action) {
    respond(id, false, null, 'Missing "action" field');
    return;
  }

  const handler = handlers[action];
  if (!handler) {
    respond(id, false, null, `Unknown action: ${action}`);
    return;
  }

  try {
    // Lazy-init browser on first real action (not shutdown)
    if (action !== 'shutdown') {
      await ensureBrowser();
    }
    const result = await handler(cmd);
    respond(id, true, result, null);
  } catch (e) {
    respond(id, false, null, e.message || String(e));
  }

  // Exit after shutdown
  if (action === 'shutdown') {
    process.exit(0);
  }
});

rl.on('close', async () => {
  if (browser) {
    await browser.close().catch(() => {});
  }
  process.exit(0);
});

// Handle unexpected errors
process.on('uncaughtException', (err) => {
  respond(null, false, null, `Uncaught exception: ${err.message}`);
});

process.on('unhandledRejection', (err) => {
  respond(null, false, null, `Unhandled rejection: ${err}`);
});
