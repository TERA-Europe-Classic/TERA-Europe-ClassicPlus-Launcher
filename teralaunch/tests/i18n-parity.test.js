import { describe, it, expect } from 'vitest';
import translations from '../src/translations.json' with { type: 'json' };

// PRD 3.7.1.key-parity: every locale exposes the same set of keys.
// A missing key in one locale leaks as a raw `MODS_*`-style string in
// the UI; an extra key in one locale hides a translation that should be
// mirrored everywhere. Both are regressions worth failing CI over.
//
// The scan is keys-only — values can legitimately differ (e.g. a German
// label being longer than the French equivalent), but the set of
// addressable keys must be identical.

function diffKeySets(reference, other) {
    const refSet = new Set(Object.keys(reference));
    const otherSet = new Set(Object.keys(other));
    return {
        missing: [...refSet].filter((k) => !otherSet.has(k)).sort(),
        extra: [...otherSet].filter((k) => !refSet.has(k)).sort(),
    };
}

describe('i18n key parity (PRD 3.7.1)', () => {
    const langs = Object.keys(translations);

    it('translations.json has at least two locales', () => {
        // Sanity: the parity check is meaningless with a single locale.
        // If someone drops all but one locale, this test tells them.
        expect(langs.length).toBeGreaterThanOrEqual(2);
    });

    it('keys_equal_across_locales', () => {
        const reference = translations[langs[0]];
        const diffs = {};
        for (const lang of langs.slice(1)) {
            const d = diffKeySets(reference, translations[lang]);
            if (d.missing.length || d.extra.length) {
                diffs[lang] = d;
            }
        }
        expect(diffs, `i18n key drift vs ${langs[0]}: ${JSON.stringify(diffs, null, 2)}`).toEqual({});
    });

    it('every locale has the same key count', () => {
        const counts = Object.fromEntries(
            langs.map((l) => [l, Object.keys(translations[l]).length])
        );
        const uniqueCounts = new Set(Object.values(counts));
        expect(uniqueCounts.size, `key counts: ${JSON.stringify(counts)}`).toBe(1);
    });

    it('detector flags a seeded missing key', () => {
        // Self-test: without this, a broken diff function would silently
        // pass every real locale and rubber-stamp regressions.
        const fixture = {
            lang_a: { shared_key: 'a', only_in_a: 'a-only' },
            lang_b: { shared_key: 'b' },
        };
        const d = diffKeySets(fixture.lang_a, fixture.lang_b);
        expect(d.missing).toEqual(['only_in_a']);
        expect(d.extra).toEqual([]);

        // And reversed: lang_b missing nothing, but has extras from lang_a's view
        const d2 = diffKeySets(fixture.lang_b, fixture.lang_a);
        expect(d2.missing).toEqual([]);
        expect(d2.extra).toEqual(['only_in_a']);
    });
});
