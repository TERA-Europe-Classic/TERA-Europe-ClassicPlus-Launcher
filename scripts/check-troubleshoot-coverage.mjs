#!/usr/bin/env node
// PRD 3.8.3.troubleshoot-md — CI grep gate.
//
// Walks every production `.rs` file under teralaunch/src-tauri/src/services/
// mods/, extracts every `.map_err(|e| format!("<template>", ...))?` error
// template (and bare `return Err(format!(...))`/`return Err("...".into())`
// variants), and asserts each template appears in docs/mod-manager/TROUBLESHOOT.md.
//
// The check is "signature substring" based, not exact string: it lifts the
// first 6-10 distinctive words from the template (stripping placeholders
// and trailing colons) and requires those words to appear verbatim in the
// troubleshoot doc. Keeps the gate robust to minor punctuation drift while
// still catching genuine coverage gaps.
//
// Exit 0 = coverage complete; non-zero = missing templates listed.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SELF_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SELF_DIR, '..');
const MODS_SRC = resolve(REPO_ROOT, 'teralaunch', 'src-tauri', 'src', 'services', 'mods');
const DOC = resolve(REPO_ROOT, 'docs', 'mod-manager', 'TROUBLESHOOT.md');

/** Recursively list .rs files under `dir`. */
function listRs(dir) {
    const out = [];
    for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) out.push(...listRs(p));
        else if (e.name.endsWith('.rs')) out.push(p);
    }
    return out;
}

/**
 * Extract a matchable prefix from a template. Takes text up to the first
 * `{...}` placeholder, trimmed and collapsed. Returns null if the prefix
 * is under 15 characters (too short to reliably match a doc section).
 *
 * Why prefix-only: the troubleshoot doc spells placeholders as `<...>` for
 * readability, which doesn't match `{}` from Rust's format!. The prefix
 * before the first placeholder is the stable, user-visible lede and is
 * copy-paste identical in both places.
 */
function signature(tmpl) {
    const prefix = tmpl.split(/\{[^}]*\}/)[0].trim();
    if (prefix.length < 15) return null;
    return prefix;
}

/**
 * Scan a source file for error templates. Matches three shapes:
 *   .map_err(|e| format!("TEMPLATE", ...))
 *   return Err(format!("TEMPLATE", ...))
 *   return Err("TEMPLATE".into())
 * Returns a list of raw template strings (minus surrounding quotes).
 */
export function extractTemplates(source) {
    const templates = [];
    const res = [
        /\.map_err\(\s*\|[^|]*\|\s*format!\(\s*"([^"\\]*(?:\\.[^"\\]*)*)"/g,
        /return\s+Err\(\s*format!\(\s*"([^"\\]*(?:\\.[^"\\]*)*)"/g,
        /return\s+Err\(\s*"([^"\\]*(?:\\.[^"\\]*)*)"\s*\.into\(\)/g,
    ];
    for (const re of res) {
        for (const m of source.matchAll(re)) {
            templates.push(m[1]);
        }
    }
    return templates;
}

function main() {
    const files = listRs(MODS_SRC);
    const templates = new Map(); // signature -> {template, file}
    for (const file of files) {
        const src = readFileSync(file, 'utf8');
        for (const t of extractTemplates(src)) {
            // Skip test-only fixtures.
            if (t.startsWith('Test ') || t === '{ not json') continue;
            const sig = signature(t);
            if (!sig) continue; // too short to match reliably
            if (!templates.has(sig)) templates.set(sig, { template: t, file });
        }
    }

    const doc = readFileSync(DOC, 'utf8');
    const missing = [];
    for (const [sig, meta] of templates) {
        if (!doc.includes(sig)) {
            missing.push({ sig, ...meta });
        }
    }

    if (missing.length === 0) {
        console.log(
            'check-troubleshoot-coverage: ok — %d production error templates covered',
            templates.size,
        );
        process.exit(0);
    }

    console.error('check-troubleshoot-coverage: FAIL');
    console.error(
        'The following production error templates are not referenced in TROUBLESHOOT.md:',
    );
    for (const m of missing) {
        console.error('  - %s', m.template);
        console.error('      from %s', m.file.replace(REPO_ROOT, '<repo>'));
        console.error('      signature: %s', m.sig);
    }
    console.error(
        '\nEither add matching copy to docs/mod-manager/TROUBLESHOOT.md or loosen',
    );
    console.error(
        'the template (e.g. consolidate a shared prefix) so the user-facing set',
    );
    console.error('stays at ~10.');
    process.exit(1);
}

// Run when invoked directly (not when imported for tests).
const entry = (process.argv[1] || '').replace(/\\/g, '/').split('/').pop();
if (entry === 'check-troubleshoot-coverage.mjs') {
    main();
}
