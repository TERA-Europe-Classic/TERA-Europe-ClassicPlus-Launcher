#!/usr/bin/env node
// PRD 3.8.2.crate-level-comments — CI coverage gate.
//
// Walks every `.rs` file under teralaunch/src-tauri/src/services/mods/ and
// asserts the file begins with a module-level `//!` doc comment *of non-
// trivial length* (>= 80 chars across the opening `//!` block). This
// prevents the anti-pattern of a single-line stub `//!` that "covers" a
// module without actually documenting what it does.
//
// Exit 0 = 100% coverage with substantive docs. Non-zero lists offenders.

import { readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SELF_DIR = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(SELF_DIR, '..');
const MODS_SRC = resolve(REPO_ROOT, 'teralaunch', 'src-tauri', 'src', 'services', 'mods');

const MIN_DOC_CHARS = 80;

/** Returns the leading `//!` block as a single string, or null if absent. */
export function extractCrateDoc(source) {
    const lines = source.split(/\r?\n/);
    const block = [];
    for (const line of lines) {
        const t = line.trimStart();
        if (t.startsWith('//!')) {
            block.push(t.slice(3).trim());
            continue;
        }
        if (t === '' && block.length > 0) {
            // Blank line inside the doc block is legal (paragraph break).
            block.push('');
            continue;
        }
        // First non-doc, non-blank line terminates the leading block.
        if (block.length > 0) break;
        if (t === '') continue; // leading blank lines before the doc
        return null; // first non-doc line and no doc yet -> no crate doc
    }
    return block.length > 0 ? block.join('\n').trim() : null;
}

function listRs(dir) {
    const out = [];
    for (const e of readdirSync(dir, { withFileTypes: true })) {
        const p = resolve(dir, e.name);
        if (e.isDirectory()) out.push(...listRs(p));
        else if (e.name.endsWith('.rs')) out.push(p);
    }
    return out;
}

function main() {
    const files = listRs(MODS_SRC);
    const offenders = [];

    for (const file of files) {
        const src = readFileSync(file, 'utf8');
        const doc = extractCrateDoc(src);
        if (!doc) {
            offenders.push({ file, reason: 'no //! crate doc' });
            continue;
        }
        if (doc.length < MIN_DOC_CHARS) {
            offenders.push({
                file,
                reason: `//! block too short (${doc.length} chars < ${MIN_DOC_CHARS})`,
            });
        }
    }

    if (offenders.length === 0) {
        console.log(
            'check-mods-crate-docs: ok — %d files with substantive //! docs',
            files.length,
        );
        process.exit(0);
    }

    console.error('check-mods-crate-docs: FAIL');
    for (const o of offenders) {
        console.error('  - %s: %s', o.file.replace(REPO_ROOT, '<repo>'), o.reason);
    }
    process.exit(1);
}

const entry = (process.argv[1] || '').replace(/\\/g, '/').split('/').pop();
if (entry === 'check-mods-crate-docs.mjs') {
    main();
}
