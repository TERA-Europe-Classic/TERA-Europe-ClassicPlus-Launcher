import { describe, it, expect } from 'vitest';
import translations from '../src/translations.json' with { type: 'json' };

// PRD 3.4.7.no-jargon: user-facing strings must not leak implementation
// jargon. The blocklist below targets mod-manager internals that appear in
// code and commit messages but have no business in translated UI copy:
//
//   - "composite"  → TERA package-system internals (CompositePackageMapper)
//   - "mapper"     → internal override table; players don't need to know it exists
//   - "sha"        → SHA-256 is an integrity algorithm, not a user concept
//   - "tmm"        → internal tool / file-format shorthand, not a player concept
//
// Case-insensitive match. If a real translated sentence genuinely needs
// one of these words (e.g. "Shanghai" contains "sha"), add it to the
// allowlist with a rationale — not by weakening the blocklist.
const JARGON_BLOCKLIST = ['composite', 'mapper', 'sha', 'tmm'];

// Full-word-ish matches would let "shape" or "shared" through `sha`. We use
// substring match (the PRD specifies the blocklist as literal strings) and
// an allowlist for known-false-positive fragments below.
const SUBSTRING_ALLOWLIST = [
    // "Shanghai" / "share" / "shape" / "shall" / "shame" etc. — none
    // currently appear in translations, but if a future copy edit adds
    // them, list them here with a rationale.
];

function findJargonLeaks(dict) {
    const leaks = [];
    for (const [lang, entries] of Object.entries(dict)) {
        for (const [key, value] of Object.entries(entries)) {
            if (typeof value !== 'string') continue;
            const lower = value.toLowerCase();
            for (const term of JARGON_BLOCKLIST) {
                if (!lower.includes(term)) continue;
                // Check allowlist: allow the term if every occurrence is
                // inside an allowlisted word.
                const allowed = SUBSTRING_ALLOWLIST.some((w) =>
                    lower.includes(w) && !lower.replace(new RegExp(w, 'g'), '').includes(term)
                );
                if (allowed) continue;
                leaks.push({ lang, key, term, value });
            }
        }
    }
    return leaks;
}

describe('i18n jargon blocklist (PRD 3.4.7)', () => {
    it('no_jargon_in_translations', () => {
        const leaks = findJargonLeaks(translations);
        expect(leaks, `jargon leaks: ${JSON.stringify(leaks, null, 2)}`).toEqual([]);
    });

    it('blocklist covers the current required terms', () => {
        // If someone edits the blocklist and drops a term, this test tells
        // them the PRD contract is now weaker than it's supposed to be.
        expect(JARGON_BLOCKLIST).toEqual(['composite', 'mapper', 'sha', 'tmm']);
    });

    it('detector flags a seeded leak in test input', () => {
        // Self-test: without this, a broken detector would silently pass
        // every real translation and rubber-stamp regressions.
        const fixture = {
            xx: {
                real_string: 'Plain English message with no jargon.',
                leak_string: 'Patch the composite mapper using TMM.',
            },
        };
        const leaks = findJargonLeaks(fixture);
        expect(leaks.length).toBe(3);
        expect(new Set(leaks.map((l) => l.term))).toEqual(new Set(['composite', 'mapper', 'tmm']));
    });
});
