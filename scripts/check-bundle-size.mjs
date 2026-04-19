#!/usr/bin/env node
/**
 * PRD 3.6.6.bundle-size-gate.
 *
 * Compares current-build artifact sizes against the previous release tag's
 * artifacts and fails CI if any artifact grew by more than the configured
 * threshold (default 5%). Guards against silent bloat — a stray debug
 * symbol, a missed minify step, or a dependency upgrade that drags in
 * megabytes of new transitives.
 *
 * Usage as a CI step (deploy.yml):
 *   node scripts/check-bundle-size.mjs \
 *     --current-setup <path/to/...-setup.exe> \
 *     --current-zip   <path/to/...-nsis.zip> \
 *     --previous-tag  v0.1.11 \
 *     --max-growth-pct 5
 *
 * When no previous tag exists (first release, or the previous release has
 * no uploaded assets yet), the gate logs a note and exits 0 — the PRD goal
 * is to catch regressions, not to block the initial release.
 */

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const ARTIFACT_KEYS = /** @type {const} */ (['setup', 'zip']);

/**
 * Pure function: given a `{setup, zip}` map of current sizes and previous
 * sizes (bytes), return an array of violations (empty if none).
 *
 * A violation fires when `current > previous * (1 + maxGrowthPct/100)`.
 * A missing previous entry (undefined / null) for either artifact is
 * treated as "no baseline for that key" — no violation fires for it.
 *
 * Shrinkage, equality, or growth ≤ threshold → no violation.
 */
export function findSizeViolations({ previous, current, maxGrowthPct }) {
    if (!Number.isFinite(maxGrowthPct) || maxGrowthPct < 0) {
        throw new Error(`maxGrowthPct must be a non-negative finite number, got ${maxGrowthPct}`);
    }
    const violations = [];
    for (const key of ARTIFACT_KEYS) {
        const prev = previous?.[key];
        const curr = current?.[key];
        if (!Number.isFinite(prev) || prev <= 0) continue;
        if (!Number.isFinite(curr) || curr < 0) {
            violations.push({ key, reason: 'current size missing or invalid', previous: prev, current: curr });
            continue;
        }
        const ratio = (curr - prev) / prev;
        const pct = ratio * 100;
        if (pct > maxGrowthPct) {
            violations.push({
                key,
                reason: `growth ${pct.toFixed(2)}% exceeds threshold ${maxGrowthPct}%`,
                previous: prev,
                current: curr,
            });
        }
    }
    return violations;
}

function parseArgs(argv) {
    const args = {};
    for (let i = 0; i < argv.length; i++) {
        const a = argv[i];
        if (!a.startsWith('--')) continue;
        const key = a.slice(2);
        const val = argv[i + 1];
        if (val === undefined || val.startsWith('--')) {
            args[key] = true;
        } else {
            args[key] = val;
            i++;
        }
    }
    return args;
}

function statSizeOrNull(file) {
    try {
        return fs.statSync(file).size;
    } catch {
        return null;
    }
}

// Fetch previous-release artifact sizes via `gh release view <tag> --json assets`.
// Matched by glob on asset name: any asset whose name contains "setup" is
// the NSIS installer; any asset ending in .nsis.zip (but NOT .sig) is the
// updater zip. Keeps the CLI dependency count low.
function fetchPreviousSizes(tag) {
    if (!tag) return { setup: null, zip: null };
    let raw;
    try {
        raw = execFileSync('gh', ['release', 'view', tag, '--json', 'assets'], {
            encoding: 'utf8',
            stdio: ['ignore', 'pipe', 'pipe'],
        });
    } catch (err) {
        console.warn(`[bundle-size] Could not fetch previous release ${tag}: ${err.message}`);
        return { setup: null, zip: null };
    }
    const parsed = JSON.parse(raw);
    const assets = Array.isArray(parsed?.assets) ? parsed.assets : [];
    const setup = assets.find((a) => /setup.*\.exe$/i.test(a.name))?.size ?? null;
    const zip = assets.find((a) => /\.nsis\.zip$/i.test(a.name) && !/\.sig$/i.test(a.name))?.size ?? null;
    return { setup, zip };
}

function main() {
    const args = parseArgs(process.argv.slice(2));
    const setupPath = args['current-setup'];
    const zipPath = args['current-zip'];
    const previousTag = args['previous-tag'];
    const maxGrowthPct = Number(args['max-growth-pct'] ?? 5);

    if (!setupPath || !zipPath) {
        console.error('Usage: check-bundle-size.mjs --current-setup <path> --current-zip <path> [--previous-tag <tag>] [--max-growth-pct 5]');
        process.exit(2);
    }

    const current = {
        setup: statSizeOrNull(setupPath),
        zip: statSizeOrNull(zipPath),
    };
    if (current.setup == null || current.zip == null) {
        console.error(`[bundle-size] current artifacts missing. setup=${setupPath} zip=${zipPath}`);
        process.exit(2);
    }

    const previous = fetchPreviousSizes(previousTag);
    if (previous.setup == null && previous.zip == null) {
        console.log(`[bundle-size] No previous sizes available (tag=${previousTag ?? 'none'}). Skipping gate.`);
        process.exit(0);
    }

    const violations = findSizeViolations({ previous, current, maxGrowthPct });

    const row = (key) => {
        const p = previous[key];
        const c = current[key];
        const delta = p != null && c != null ? `${(((c - p) / p) * 100).toFixed(2)}%` : 'n/a';
        return `  ${key.padEnd(6)} prev=${p ?? 'n/a'} curr=${c} Δ=${delta}`;
    };
    console.log(`[bundle-size] threshold=+${maxGrowthPct}% against ${previousTag}`);
    console.log(row('setup'));
    console.log(row('zip'));

    if (violations.length === 0) {
        console.log('[bundle-size] OK — no artifact exceeded the growth threshold.');
        process.exit(0);
    }

    console.error('[bundle-size] FAIL — the following artifacts grew too much:');
    for (const v of violations) {
        console.error(`  - ${v.key}: ${v.reason} (previous=${v.previous}, current=${v.current})`);
    }
    process.exit(1);
}

// Only run main when invoked directly (works on Windows too — argv[1] is
// the script path with backslashes, so basename-compare is robust).
const invokedScriptName = path.basename(process.argv[1] ?? '');
if (invokedScriptName === 'check-bundle-size.mjs') {
    main();
}
