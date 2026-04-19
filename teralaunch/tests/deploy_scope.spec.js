#!/usr/bin/env node
// PRD 3.1.14.deploy-scope-gate.
//
// Scans .github/workflows/deploy.yml for upload URLs and asserts every one
// targets a path under /classicplus/. Fails with a clear error if any URL
// points at / or /classic/ or anywhere else on the kasserver root.
//
// Also ships a small self-test so the scanner's own rejection logic is
// exercised on every run — a silent gate is worse than no gate.
//
// Runs under plain Node (no test framework). Deploy workflow invokes it as
// a pre-upload step; exit 0 = proceed, non-zero = fail the job.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const SELF_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SELF_DIR, '..', '..');
const DEPLOY_YML = resolve(REPO_ROOT, '.github', 'workflows', 'deploy.yml');

const ALLOWED_PATH_PREFIX = '/classicplus/';

// Hosts whose paths we actually care about. An upload URL for a third-party
// service (GitHub release API, etc.) is irrelevant to the scope gate.
const KASSERVER_HOSTS = [
    'web.tera-germany.de',
    // SFTP_HOST is a secret; the workflow uses it as a literal in the BASE
    // string. We detect that form separately.
];

/**
 * Extracts every URL from `body` that we'd route through the upload path.
 * Covers:
 *   - ftp://…  / ftps://…  (SFTP_HOST templated or literal)
 *   - https://web.tera-germany.de/… (the updater endpoint in latest.json)
 *
 * Returns a list of { url, path } records.
 */
export function extractUploadUrls(body) {
    const out = [];

    // 1. ftp(s) URLs, including ones templated with ${SFTP_HOST} or
    //    ${{ secrets.SFTP_HOST }}. Host is opaque — we only care about the
    //    path, so we scan to the first `/` after the `scheme://` marker.
    const ftpRe = /ftps?:\/\/[^\s"']+/g;
    for (const m of body.matchAll(ftpRe)) {
        const full = m[0];
        const rest = full.slice(full.indexOf('//') + 2);
        const slashIdx = rest.indexOf('/');
        const path = slashIdx >= 0 ? rest.slice(slashIdx) : '/';
        out.push({ url: full, path });
    }

    // 2. https URLs to the kasserver-fronting host.
    for (const host of KASSERVER_HOSTS) {
        const hostRe = new RegExp(
            `https:\\/\\/${host.replace(/\./g, '\\.')}([^"\\s'$]*)`,
            'g',
        );
        for (const m of body.matchAll(hostRe)) {
            const path = m[1] || '/';
            out.push({ url: m[0], path });
        }
    }

    return out;
}

/**
 * Returns an array of human-readable violation strings. Empty = gate passed.
 */
export function findScopeViolations(body) {
    const urls = extractUploadUrls(body);
    const violations = [];

    if (urls.length === 0) {
        violations.push(
            'deploy.yml contains no upload URLs matching the scope-gate patterns ' +
            '(ftp(s):// or https://<kasserver-host>). Either the patterns drifted ' +
            'or the file was moved — refusing to deploy.',
        );
        return violations;
    }

    for (const { url, path } of urls) {
        // Host-fronting URLs may include the /classic/ prefix: the public update
        // endpoint is https://web.tera-germany.de/classic/classicplus/… . Accept
        // either /classicplus/ (ftp direct) or /classic/classicplus/ (https cdn).
        const ok =
            path.startsWith(ALLOWED_PATH_PREFIX) ||
            path.startsWith('/classic/classicplus/') ||
            path.startsWith('/classic' + ALLOWED_PATH_PREFIX);

        if (!ok) {
            violations.push(
                `URL ${url} has path "${path}" — must be under ${ALLOWED_PATH_PREFIX} ` +
                `or /classic/classicplus/ on kasserver.`,
            );
        }
    }

    return violations;
}

// --- Self-tests ------------------------------------------------------------

function runSelfTests() {
    // Positive samples that must pass.
    const good = [
        'BASE="ftp://${SFTP_HOST}/classicplus/"',
        'url = "https://web.tera-germany.de/classic/classicplus/$env:ZIP_NAME"',
        'ftps://host/classicplus/inner/',
    ];
    for (const g of good) {
        assert.deepEqual(
            findScopeViolations(g),
            [],
            `positive sample flagged: ${g}`,
        );
    }

    // Negative samples that must fail.
    const bad = [
        'ftp://${SFTP_HOST}/',
        'ftp://${SFTP_HOST}/classic/',
        'ftps://host/classicmod/',
        'url = "https://web.tera-germany.de/latest.json"',
        'url = "https://web.tera-germany.de/classic/latest.json"',
    ];
    for (const b of bad) {
        const v = findScopeViolations(b);
        assert.ok(
            v.length >= 1,
            `negative sample not flagged: ${b}`,
        );
    }

    // Empty body must fail (scanner didn't find any URLs to gate).
    assert.ok(
        findScopeViolations('').length >= 1,
        'empty body should have been flagged as no-URLs-found',
    );
}

function main() {
    runSelfTests();
    console.log('deploy-scope-gate: self-tests passed (%d patterns)', 5 + 5 + 1);

    const body = readFileSync(DEPLOY_YML, 'utf8');
    const violations = findScopeViolations(body);

    if (violations.length === 0) {
        const urls = extractUploadUrls(body);
        console.log(
            'deploy-scope-gate: OK — %d upload URL(s) all under %s',
            urls.length,
            ALLOWED_PATH_PREFIX,
        );
        process.exit(0);
    }

    console.error('deploy-scope-gate: FAIL — %d violation(s):', violations.length);
    for (const v of violations) {
        console.error('  - %s', v);
    }
    process.exit(1);
}

// Run main only when this file is the entry point — avoids side effects
// when a sibling test module imports `findScopeViolations` for its own
// checks.
const entryBasename = (process.argv[1] || '').replace(/\\/g, '/').split('/').pop();
if (entryBasename === 'deploy_scope.spec.js') {
    main();
}
