// Pure helpers for building the Code Map webview HTML safely.
// Kept free of the `vscode` import so they can be unit-tested under plain Node.

import { randomBytes } from 'crypto';

/**
 * Serialize `value` as JSON that is safe to place inside an HTML `<script>`
 * element (either an executable one or a `type="application/json"` data
 * block).
 *
 * `JSON.stringify` alone is NOT safe there: a string such as
 * `"</script><script>alert(1)</script>"` terminates the element early and the
 * rest is parsed as markup/script, and `<!--` switches the HTML tokenizer into
 * script-data-escaped state. U+2028/U+2029 are valid in JSON strings but were
 * line terminators in pre-ES2019 JavaScript. Escaping `<`, `>`, `&`, U+2028
 * and U+2029 as `\uXXXX` keeps the JSON value identical after `JSON.parse`
 * while leaving no character the HTML parser can act on.
 */
export function serializeForHtmlScript(value: unknown): string {
    const json = JSON.stringify(value === undefined ? null : value);
    return json.replace(/[<>&\u2028\u2029]/g, (ch) => {
        return '\\u' + ch.charCodeAt(0).toString(16).padStart(4, '0');
    });
}

/** A fresh random nonce for the webview Content-Security-Policy. */
export function makeNonce(): string {
    const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let out = '';
    const bytes = randomBytes(32);
    for (let i = 0; i < bytes.length; i++) {
        out += alphabet[bytes[i] % alphabet.length];
    }
    return out;
}

/**
 * Content-Security-Policy for the Code Map webview: nothing loads by default;
 * only the nonce-tagged `<script>`/`<style>` elements run/apply, so markup
 * injected through data (e.g. `<img onerror=...>` or a stray `<script>`)
 * cannot execute.
 */
export function buildCsp(cspSource: string, nonce: string): string {
    return [
        "default-src 'none'",
        `img-src ${cspSource} data:`,
        `style-src ${cspSource} 'nonce-${nonce}'`,
        `script-src 'nonce-${nonce}'`,
    ].join('; ');
}
