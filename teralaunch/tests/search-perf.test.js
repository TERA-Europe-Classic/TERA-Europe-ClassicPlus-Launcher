/**
 * PRD 3.6.4.search-one-frame: filtering the mods list must run under one
 * 60fps frame (≤16 ms) on 300 entries. The browse+installed tabs both
 * render by iterating entries and calling `filterMatches(entry)` — any
 * slow path (regex, DOM lookup, repeated allocations) shows up here as
 * a dropped frame while the user types.
 */
import { describe, it, expect, vi } from 'vitest';

// mods.js reads window.__TAURI__.tauri and ...event at module-evaluation
// time. Stub the surface before import so the module doesn't explode.
global.window.__TAURI__ = {
    tauri: { invoke: vi.fn() },
    event: { listen: vi.fn(() => Promise.resolve(() => {})) },
};

const { ModsView } = await import('../src/mods.js');

function makeCatalogEntries(n) {
    const entries = [];
    const cats = ['ui', 'utility', 'visual', 'gameplay', 'sound'];
    const kinds = ['external', 'gpk'];
    for (let i = 0; i < n; i++) {
        entries.push({
            id: `catalog.entry.${i}`,
            kind: kinds[i % 2],
            name: `Mod ${i} ${cats[i % cats.length]}`,
            author: `author_${i % 20}`,
            description: `A description referencing term_${i} and stuff`,
            short_description: `short_${i}`,
            category: cats[i % cats.length],
            version: '1.0.0',
        });
    }
    return entries;
}

function runFilter(ctx, entries) {
    // Mirrors the per-entry call shape used inside `render()`.
    const out = [];
    for (const entry of entries) {
        if (ModsView.filterMatches.call(ctx, entry)) out.push(entry);
    }
    return out;
}

describe('mods search perf (PRD 3.6.4)', () => {
    it('under_one_frame', () => {
        const entries = makeCatalogEntries(300);
        const ctx = { state: { filter: 'all', category: 'all', query: 'term_17' } };

        // Warm-up run to prime V8 and let JIT settle so we don't measure
        // the first-call inline-cache-miss cost.
        runFilter(ctx, entries);

        // Take the median of 7 timed runs — robust against one-off GC
        // pauses without being so long that it slows the test suite.
        const samples = [];
        for (let i = 0; i < 7; i++) {
            const t0 = performance.now();
            runFilter(ctx, entries);
            samples.push(performance.now() - t0);
        }
        samples.sort((a, b) => a - b);
        const median = samples[Math.floor(samples.length / 2)];

        expect(median, `median of 7 samples: ${samples.map((s) => s.toFixed(3)).join(', ')}ms`).toBeLessThanOrEqual(16);
    });

    it('filters actually apply (sanity control)', () => {
        // Without this, a broken filterMatches that always returned true
        // (or early-returned) would pass the perf test trivially while
        // still being visibly broken to the user.
        const entries = makeCatalogEntries(50);
        const ctx = { state: { filter: 'gpk', category: 'all', query: '' } };
        const filtered = runFilter(ctx, entries);
        expect(filtered.length).toBeGreaterThan(0);
        expect(filtered.every((e) => e.kind === 'gpk')).toBe(true);
    });

    it('query narrows matches', () => {
        const entries = makeCatalogEntries(100);
        const ctx = { state: { filter: 'all', category: 'all', query: 'term_42' } };
        const filtered = runFilter(ctx, entries);
        // Description contains "term_42" exactly once (entry 42). No other
        // entry's description includes that exact fragment — the scanner
        // uses substring match so `term_420` would also match if present,
        // but our fixture only goes up to `term_99`.
        expect(filtered.length).toBeGreaterThanOrEqual(1);
        expect(filtered.some((e) => e.id === 'catalog.entry.42')).toBe(true);
    });
});
