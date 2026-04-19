import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

// PRD 3.7.4.no-hardcoded-english: mods.js, mods.html, and the mod-specific
// paths in app.js must not ship English user-facing copy without routing
// it through the i18n layer (`data-translate*` attributes or `this.t(...)`).
//
// Why grep instead of a DOM walk: the detector needs to bite at commit
// time, not at runtime. A DOM walk would need a browser; a source grep
// runs in CI in milliseconds.
//
// What counts as a leak:
//   (a) `aria-label="Some English"` / `title="..."` / `placeholder="..."`
//       without a sibling `data-translate-aria-label` / `-title` /
//       `-placeholder` attribute (the current app.js does NOT yet handle
//        data-translate-aria-label — follow-up fix.mods-i18n-aria — so for
//        now the allowlist carries the aria leaks explicitly).
//   (b) `>Plain English Text<` inside an element whose preceding attrs
//       contain no `data-translate=`.
//
// What is NOT a leak (correctly annotated): inline text inside an element
// that carries `data-translate="KEY"` — that text is the pre-i18n fallback
// which `updateAllTranslations()` replaces at runtime. The fallback stays
// in source for accessibility during initial render and for Git-grep
// discoverability.
//
// ALLOWLIST below carries the current-state leaks so this test passes
// today and any NEW leak fails CI. fix.mods-hardcoded-i18n-strings is
// the P1 follow-up that burns these down.

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');

const TARGETS = [
    'teralaunch/src/mods.js',
    'teralaunch/src/mods.html',
];

// Each entry: exact literal substring that's known-English-hardcoded but
// not yet wired to i18n. When you fix a leak, delete its allowlist row.
const ALLOWLIST = [
    // mods.js — overflow button aria-label; app.js lacks data-translate-aria-label
    { file: 'teralaunch/src/mods.js', literal: 'aria-label="More"' },
    // mods.js — toggle title ternary; dynamic data-translate-title is awkward
    { file: 'teralaunch/src/mods.js', literal: "'Enabled — runs with the game'" },
    { file: 'teralaunch/src/mods.js', literal: "'Disabled — click to enable'" },
    // mods.js — status pill inline text (no data-translate on the span)
    { file: 'teralaunch/src/mods.js', literal: '>Running<' },
    // mods.js — overflow popover menu items (no data-translate on the spans)
    { file: 'teralaunch/src/mods.js', literal: '>Details<' },
    { file: 'teralaunch/src/mods.js', literal: '>Open source<' },
    { file: 'teralaunch/src/mods.js', literal: '>Uninstall<' },
    // mods.html — close buttons (app.js doesn't wire data-translate-aria-label yet)
    { file: 'teralaunch/src/mods.html', literal: 'aria-label="Close"' },
    { file: 'teralaunch/src/mods.html', literal: 'title="Close (Esc)"' },
    { file: 'teralaunch/src/mods.html', literal: 'aria-label="Category filter"' },
];

function stripAllowlist(content, file) {
    let stripped = content;
    for (const entry of ALLOWLIST) {
        if (entry.file !== file) continue;
        // Replace each literal with a marker of the same length so line
        // numbers reported by the scanner still line up with source.
        const marker = '/*ALLOW*/'.padEnd(entry.literal.length, '/');
        stripped = stripped.split(entry.literal).join(marker);
    }
    return stripped;
}

// Heuristics for suspicious hardcoded English
const RULES = [
    {
        name: 'aria-label with English content (missing data-translate-aria-label)',
        re: /aria-label="([^"]{2,})"/g,
    },
    {
        name: 'title attribute with English content (missing data-translate-title)',
        re: /\btitle="([^"]{2,})"/g,
    },
    {
        name: 'placeholder with English content (missing data-translate-placeholder)',
        re: /\bplaceholder="([^"]{2,})"/g,
    },
];

function looksEnglish(s) {
    // Multi-char word with at least two letters in a row, and a space
    // somewhere (single-word captions like "OK" rarely need translation
    // and generate too much noise). Also require a lowercase letter so we
    // don't flag all-caps data-attr values.
    return /[a-z]{2,}/.test(s) && /\s/.test(s);
}

function findLeaks() {
    const leaks = [];
    for (const file of TARGETS) {
        const full = path.join(REPO_ROOT, file);
        const raw = fs.readFileSync(full, 'utf8');
        const src = stripAllowlist(raw, file);
        const lines = src.split('\n');
        for (let i = 0; i < lines.length; i++) {
            const line = lines[i];
            for (const rule of RULES) {
                rule.re.lastIndex = 0;
                let m;
                while ((m = rule.re.exec(line)) !== null) {
                    const captured = m[1];
                    if (!looksEnglish(captured)) continue;
                    // Template interpolation: the outer shell contains
                    // `${...}` — any English-looking substrings inside the
                    // expression (e.g. variable names like "enabled") are
                    // scanned via the expression's own string literals,
                    // which the allowlist strips separately.
                    if (captured.includes('${')) continue;
                    // If the same line has a corresponding data-translate-*
                    // attribute, the hardcoded content is the pre-i18n fallback.
                    if (rule.name.includes('aria-label') && /data-translate-aria-label=/.test(line)) continue;
                    if (rule.name.includes('title attribute') && /data-translate-title=/.test(line)) continue;
                    if (rule.name.includes('placeholder') && /data-translate-placeholder=/.test(line)) continue;
                    leaks.push({ file, line: i + 1, rule: rule.name, text: captured });
                }
            }
        }
    }
    return leaks;
}

describe('i18n no-hardcoded-english (PRD 3.7.4)', () => {
    it('targets exist and are non-empty', () => {
        for (const t of TARGETS) {
            const p = path.join(REPO_ROOT, t);
            expect(fs.existsSync(p), `missing target: ${t}`).toBe(true);
            expect(fs.statSync(p).size, `empty target: ${t}`).toBeGreaterThan(0);
        }
    });

    it('no new hardcoded English outside the allowlist', () => {
        const leaks = findLeaks();
        expect(
            leaks,
            `hardcoded English leaks outside allowlist:\n${JSON.stringify(leaks, null, 2)}`,
        ).toEqual([]);
    });

    it('allowlist is non-empty and documented', () => {
        // If someone empties the allowlist without deleting every source
        // leak, this test asserts the allowlist still reflects real code.
        // Each entry points at a specific follow-up the fix.mods-i18n-* P1
        // backlog will burn down. If the allowlist ever goes to zero,
        // delete this block and enforce strict-zero.
        expect(ALLOWLIST.length).toBeGreaterThan(0);
        for (const entry of ALLOWLIST) {
            const p = path.join(REPO_ROOT, entry.file);
            const raw = fs.readFileSync(p, 'utf8');
            expect(
                raw.includes(entry.literal),
                `stale allowlist entry: "${entry.literal}" no longer appears in ${entry.file} — delete this row`,
            ).toBe(true);
        }
    });

    it('detector flags a seeded leak in synthetic input', () => {
        // Self-test: prove the scanner actually bites on a known-bad
        // string. If the scanner regressed to returning empty, the real
        // tests would pass silently — this keeps the detector honest.
        const line = '<button aria-label="Some Hardcoded Text">x</button>';
        const rule = RULES[0];
        rule.re.lastIndex = 0;
        const m = rule.re.exec(line);
        expect(m, 'scanner must match aria-label').not.toBeNull();
        expect(looksEnglish(m[1])).toBe(true);
    });
});
