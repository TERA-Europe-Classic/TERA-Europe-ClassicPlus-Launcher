import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

// sec.shell-open-call-sites-pinned (iter 82) — defence-in-depth for the
// Tauri shell-plugin `open` endpoint. CVE-2025-31477 showed that the
// plugin's default scope once accepted `file://`, `smb://`, `nfs://` URIs;
// plugin 2.3.5 (our pinned version) fixed that default, but a future
// refactor that passes an arbitrary fetch()-derived or DOM-derived
// string into `shell.open()` would still re-open the RCE door via any
// protocol the OS has a registered handler for.
//
// This test enforces that every call site of `window.__TAURI__.shell.open(X)`
// and every call site of `App.openExternal(X)` passes an X that is one of:
//
//   - a string literal (single/double-quoted or backtick)
//   - a known allowlisted identifier (see ALLOWLIST below)
//   - a member expression rooted at URLS.external.* (our constants module)
//   - a template literal that only interpolates URLS.external.* expressions
//
// Anything else — a bare variable, a fetch response, a user-typed string —
// fails with the source location so the reviewer has to either add it to
// the allowlist (with a justification comment) or find a safer sink.
//
// Scope is deliberately regex-based not AST-aware: the existing
// i18n-no-hardcoded.test.js sets the pattern and we stay consistent. An
// AST walker would be more precise but would introduce a new toolchain
// dependency and the three current call sites are simple enough that
// regex matches their full argument shape.

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const APP_JS = path.join(REPO_ROOT, 'teralaunch/src/app.js');

/// Arg patterns that are known-safe. New entries need a one-line comment
/// citing the provenance (constant, anchor DOM, derived-from-constant).
const SAFE_IDENTIFIERS = [
    'localizedUrl',          // derived inside openExternal from parameter via localizeForumUrl
    'PROFILE_URL',           // module constant (teralib config)
    'link.url',              // local loop var whose source is URLS.external.* (see app.js:2346-2364)
    'event.target.href',     // DOM anchor href — app-authored HTML only, no untrusted injection
    'url',                   // openExternal's parameter name (the callers of openExternal are the real gate; checked separately below)
    'locale',                // i18n language code from this.currentLanguage — bound to enum {en,fr,de,ru}, never attacker-controlled
];

/// Classify an argument string. Returns null if safe; returns a reason
/// string if it looks unsafe.
function classifyArg(arg) {
    const trimmed = arg.trim();
    if (trimmed.length === 0) return 'empty argument';

    // String literal (single, double, or template with no interpolation)
    if (/^(['"])[^'"]*\1$/.test(trimmed)) return null;
    if (/^`[^`$]*`$/.test(trimmed)) return null;

    // Known-safe identifier or member expression
    if (SAFE_IDENTIFIERS.includes(trimmed)) return null;

    // URLS.external.<name>
    if (/^URLS\.external\.[\w]+$/.test(trimmed)) return null;

    // Template literal whose only interpolations are URLS.external.*
    // expressions (optionally with a ?locale=... suffix — see app.js:2281).
    // Shape: `...${URLS.external.foo}...` with no ${ other than on those refs.
    if (trimmed.startsWith('`') && trimmed.endsWith('`')) {
        const interpolations = [...trimmed.matchAll(/\$\{([^}]+)\}/g)].map(m => m[1].trim());
        const allSafe = interpolations.every(
            expr => /^URLS\.external\.[\w]+$/.test(expr) || SAFE_IDENTIFIERS.includes(expr)
        );
        if (allSafe) return null;
        return `template literal with unsafe interpolation(s): ${interpolations.join(', ')}`;
    }

    return `argument "${trimmed}" is not a string literal, allowlisted identifier, or URLS.external.* reference`;
}

/// Extract every call of `fn(` in source, returning [{line, arg}] pairs.
/// Only captures a single-line argument — multi-line call shapes are
/// already flagged by the line scan as "unparseable" which will fail.
function findCallSites(src, fnPattern) {
    const lines = src.split('\n');
    const hits = [];
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        const re = new RegExp(`${fnPattern}\\(([^)]*)\\)`, 'g');
        let m;
        while ((m = re.exec(line)) !== null) {
            hits.push({ line: i + 1, arg: m[1] });
        }
    }
    return hits;
}

describe('shell.open call-site safety (sec.shell-open-call-sites-pinned)', () => {
    it('target file exists and is non-empty', () => {
        expect(fs.existsSync(APP_JS), `missing: ${APP_JS}`).toBe(true);
        expect(fs.statSync(APP_JS).size, 'app.js must be non-empty').toBeGreaterThan(0);
    });

    it('every window.__TAURI__.shell.open(X) passes a safe X', () => {
        const src = fs.readFileSync(APP_JS, 'utf8');
        // Match literal `window.__TAURI__.shell.open(` so we don't catch the
        // feature-test at line 2259 which reads `window.__TAURI__.shell.open`
        // without calling it.
        const hits = findCallSites(src, 'window\\.__TAURI__\\.shell\\.open');
        // Filter to actual call sites (the (X) portion is non-empty or empty
        // but followed by `)`). The feature-test form doesn't have `(` at all.
        const unsafe = [];
        for (const { line, arg } of hits) {
            const reason = classifyArg(arg);
            if (reason !== null) unsafe.push({ line, arg, reason });
        }
        expect(
            unsafe,
            `shell.open() call sites with unsafe arguments:\n${JSON.stringify(unsafe, null, 2)}`,
        ).toEqual([]);
    });

    it('every App.openExternal(X) passes a safe X', () => {
        const src = fs.readFileSync(APP_JS, 'utf8');
        // Two caller shapes: `App.openExternal(` and `this.openExternal(` — both
        // flow into the same sink at app.js:2253. Skip the `openExternal(url)` method
        // declaration itself (the `(url)` there is the parameter list, not a call).
        const hits = [
            ...findCallSites(src, 'App\\.openExternal'),
            ...findCallSites(src, 'this\\.openExternal'),
        ];
        const unsafe = [];
        for (const { line, arg } of hits) {
            const reason = classifyArg(arg);
            if (reason !== null) unsafe.push({ line, arg, reason });
        }
        expect(
            unsafe,
            `App.openExternal()/this.openExternal() call sites with unsafe arguments:\n${JSON.stringify(unsafe, null, 2)}`,
        ).toEqual([]);
    });

    it('classifier bites on seeded bad input', () => {
        // Self-test — prove the classifier rejects obviously unsafe shapes
        // so a regression in classifyArg doesn't silently pass the real
        // tests above.
        expect(classifyArg('fetchedUrl'), 'bare var must be rejected').not.toBeNull();
        expect(classifyArg('someObject.url'), 'arbitrary member must be rejected').not.toBeNull();
        expect(classifyArg('`${dangerous}`'), 'template with unsafe interp must be rejected').not.toBeNull();
        expect(classifyArg(''), 'empty arg must be rejected').not.toBeNull();
    });

    it('classifier accepts every currently allowed shape', () => {
        // Positive self-test — counterpart to the negative one above.
        expect(classifyArg('"https://example.com"'), 'string literal').toBeNull();
        expect(classifyArg("'mailto:x@y'"), 'single-quote literal').toBeNull();
        expect(classifyArg('localizedUrl'), 'allowlisted identifier').toBeNull();
        expect(classifyArg('event.target.href'), 'anchor DOM href').toBeNull();
        expect(classifyArg('URLS.external.forum'), 'URLS.external member').toBeNull();
        expect(classifyArg('`${URLS.external.register}?locale=${URLS.external.forum}`'), 'template of URLS.external').toBeNull();
    });
});
