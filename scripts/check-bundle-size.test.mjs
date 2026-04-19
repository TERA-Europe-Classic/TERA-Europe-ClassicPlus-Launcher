#!/usr/bin/env node
// Self-test for the bundle-size gate. Runs on every invocation so the
// scanner can't silently rot into a rubber stamp — same pattern as
// check-changelog-plain-english and check-troubleshoot-coverage.

import assert from 'node:assert/strict';
import { findSizeViolations } from './check-bundle-size.mjs';

function allows_within_threshold() {
    // 4% growth with a 5% ceiling → no violation.
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 1040, zip: 960 },
        maxGrowthPct: 5,
    });
    assert.deepEqual(v, []);
}

function allows_shrinkage() {
    // Shrinking artifact is always allowed.
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 900, zip: 500 },
        maxGrowthPct: 5,
    });
    assert.deepEqual(v, []);
}

function fails_when_setup_grows_too_much() {
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 1060, zip: 1000 }, // 6% > 5%
        maxGrowthPct: 5,
    });
    assert.equal(v.length, 1);
    assert.equal(v[0].key, 'setup');
    assert.ok(/exceeds/.test(v[0].reason));
}

function fails_when_zip_grows_too_much() {
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 1000, zip: 1100 }, // 10% > 5%
        maxGrowthPct: 5,
    });
    assert.equal(v.length, 1);
    assert.equal(v[0].key, 'zip');
}

function fails_on_both_when_both_regress() {
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 1200, zip: 1200 },
        maxGrowthPct: 5,
    });
    assert.equal(v.length, 2);
    const keys = v.map((x) => x.key).sort();
    assert.deepEqual(keys, ['setup', 'zip']);
}

function boundary_exact_threshold_allowed() {
    // Exactly 5.00% growth must pass — `> maxGrowthPct`, not `>=`. This
    // prevents floating-point spookiness from failing a legitimate release
    // that lands on the nose.
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 1050, zip: 1050 },
        maxGrowthPct: 5,
    });
    assert.deepEqual(v, []);
}

function skips_missing_baseline_without_flagging() {
    // First release: no previous sizes. Gate should produce no violations
    // so deploy isn't blocked on the initial cut.
    const v = findSizeViolations({
        previous: { setup: null, zip: null },
        current: { setup: 1_000_000, zip: 1_000_000 },
        maxGrowthPct: 5,
    });
    assert.deepEqual(v, []);
}

function skips_partial_baseline_for_its_half() {
    // Only setup baseline available; zip has no baseline. Setup grows
    // within threshold → no violation for either key.
    const v = findSizeViolations({
        previous: { setup: 1000, zip: null },
        current: { setup: 1040, zip: 9_999_999 },
        maxGrowthPct: 5,
    });
    assert.deepEqual(v, []);
}

function flags_missing_current_as_violation() {
    // The build produced no zip artifact — that's either a build-system
    // regression or a path-resolution bug. Either way, not an accepted
    // state for the gate to pass silently.
    const v = findSizeViolations({
        previous: { setup: 1000, zip: 1000 },
        current: { setup: 1000, zip: null },
        maxGrowthPct: 5,
    });
    assert.equal(v.length, 1);
    assert.equal(v[0].key, 'zip');
    assert.ok(/missing or invalid/.test(v[0].reason));
}

function rejects_negative_threshold() {
    // Defensive: a negative threshold is almost certainly a config typo.
    assert.throws(() =>
        findSizeViolations({
            previous: { setup: 1000 },
            current: { setup: 1000 },
            maxGrowthPct: -5,
        })
    );
}

function run() {
    allows_within_threshold();
    allows_shrinkage();
    fails_when_setup_grows_too_much();
    fails_when_zip_grows_too_much();
    fails_on_both_when_both_regress();
    boundary_exact_threshold_allowed();
    skips_missing_baseline_without_flagging();
    skips_partial_baseline_for_its_half();
    flags_missing_current_as_violation();
    rejects_negative_threshold();
    console.log('check-bundle-size.test: ok (10 tests)');
}

run();
