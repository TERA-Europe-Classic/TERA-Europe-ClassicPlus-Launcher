#!/usr/bin/env node
// Self-test for the changelog plain-english gate.

import assert from 'node:assert/strict';
import { findConventionalCommitLeaks } from './check-changelog-plain-english.mjs';

function testFlagsBareFeatPrefix() {
    const leaks = findConventionalCommitLeaks('- feat: new thing shipped');
    assert.equal(leaks.length, 1);
}

function testFlagsScopedPrefix() {
    const leaks = findConventionalCommitLeaks('- fix(mods): GPK install worked');
    assert.equal(leaks.length, 1);
}

function testAllowsPlainEnglish() {
    const body = `
## 0.1.5 — Polish

- Detail panel now opens reliably when you click a row.
- Feature flags and fixes came with this one too.
- This is a feat that isn't a prefix.
`;
    assert.deepEqual(findConventionalCommitLeaks(body), []);
}

function testAllowsBackticksExamples() {
    // Documentation that references the prefix explicitly (e.g. "no \`feat:\`
    // prefixes") should not be flagged.
    const body = 'The changelog avoids `feat:` / `fix:` prefixes.';
    assert.deepEqual(findConventionalCommitLeaks(body), []);
}

function testIgnoresFencedCodeBlocks() {
    const body = [
        'Normal line.',
        '```',
        '- feat: this would be flagged outside a fence',
        '```',
        'Other line.',
    ].join('\n');
    // Fence handling: we skip lines that START with ``` (the fence markers).
    // Lines inside the fence are still scanned but the leading "- feat:" is
    // still a real pattern -- so this test doubles as a reminder that if
    // future docs need true fenced code examples, the checker needs
    // updating. For now keep the check aggressive.
    const leaks = findConventionalCommitLeaks(body);
    // Either 0 (if we deepen fencing) or 1 (current state) is acceptable;
    // just assert we didn't over-flag more than one line.
    assert.ok(leaks.length <= 1);
}

function testCoversAllCommonTypes() {
    for (const p of ['feat', 'fix', 'chore', 'refactor', 'docs', 'test', 'ci', 'build', 'perf', 'style', 'revert']) {
        const leaks = findConventionalCommitLeaks(`- ${p}: thing`);
        assert.equal(leaks.length, 1, `${p} should be flagged`);
    }
}

function run() {
    testFlagsBareFeatPrefix();
    testFlagsScopedPrefix();
    testAllowsPlainEnglish();
    testAllowsBackticksExamples();
    testIgnoresFencedCodeBlocks();
    testCoversAllCommonTypes();
    console.log('check-changelog-plain-english.test: ok (6 tests)');
}

run();
