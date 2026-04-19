#!/usr/bin/env node
// PRD 3.8.5.player-changelog — CI grep gate.
//
// Scans docs/CHANGELOG.md for conventional-commit prefixes that would
// indicate a developer commit message leaked into player-facing copy.
// Empty body is fine (not all releases ship player-relevant changes, and
// the file can legitimately start empty under an Unreleased heading).

import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SELF_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SELF_DIR, '..');
const DOC = resolve(REPO_ROOT, 'docs', 'CHANGELOG.md');

// Conventional-commit prefix regex. Anchored to bullet / line start so we
// don't false-positive on narrative prose that happens to say "feat".
// Accepts the trailing `(scope)` form too.
const CC_RE =
    /(^|\n)\s*(?:-|\*)?\s*(feat|fix|chore|refactor|docs|test|ci|build|perf|style|revert)(\([^)]+\))?:/i;

export function findConventionalCommitLeaks(body) {
    const leaks = [];
    const lines = body.split(/\r?\n/);
    for (let i = 0; i < lines.length; i++) {
        const line = lines[i];
        // Allow occurrences inside inline-code or fenced code blocks (a doc
        // might reference the prefix explicitly as an example).
        if (line.trimStart().startsWith('```')) continue;
        if (line.includes('`feat:`') || line.includes('`fix:`')) continue;
        if (CC_RE.test(line)) {
            leaks.push({ line: i + 1, text: line });
        }
    }
    return leaks;
}

function main() {
    const body = readFileSync(DOC, 'utf8');
    const leaks = findConventionalCommitLeaks(body);

    if (leaks.length === 0) {
        console.log(
            'check-changelog-plain-english: ok — %d lines scanned, 0 conventional-commit prefixes',
            body.split('\n').length,
        );
        process.exit(0);
    }

    console.error('check-changelog-plain-english: FAIL');
    console.error(
        'Found %d line(s) with conventional-commit prefixes — rewrite in plain English:',
        leaks.length,
    );
    for (const l of leaks) {
        console.error('  L%d: %s', l.line, l.text.trim());
    }
    process.exit(1);
}

const entry = (process.argv[1] || '').replace(/\\/g, '/').split('/').pop();
if (entry === 'check-changelog-plain-english.mjs') {
    main();
}
