#!/usr/bin/env node
// Self-test for the crate-docs gate. Pins the extractor against fixtures so
// a silent regression in the detector can't rubber-stamp an undocumented
// module.

import assert from 'node:assert/strict';
import { extractCrateDoc } from './check-mods-crate-docs.mjs';

function testExtractsLeadingDocBlock() {
    const src = `//! A mod file.\n//! Second line.\n//!\n//! Third para.\nuse std;\n`;
    const d = extractCrateDoc(src);
    assert.ok(d);
    assert.ok(d.includes('A mod file.'));
    assert.ok(d.includes('Third para.'));
}

function testReturnsNullWhenMissing() {
    const src = `use std;\nfn main() {}\n`;
    assert.equal(extractCrateDoc(src), null);
}

function testReturnsNullWhenFirstNonBlankIsNotDoc() {
    const src = `\n\nuse std;\n//! Too late — attached to use not module.\n`;
    assert.equal(extractCrateDoc(src), null);
}

function testHandlesLeadingBlankLines() {
    const src = `\n\n//! After blanks.\n//! Works.\nuse std;`;
    const d = extractCrateDoc(src);
    assert.ok(d);
    assert.ok(d.includes('After blanks.'));
}

function testTreatsInlineBlankInsideBlockAsContinuation() {
    // A blank line between two //! runs is a paragraph break, not a terminator.
    const src = `//! First para.\n\n//! Second para.\nuse std;`;
    const d = extractCrateDoc(src);
    assert.ok(d);
    assert.ok(d.includes('First para.'));
    assert.ok(d.includes('Second para.'));
}

function run() {
    testExtractsLeadingDocBlock();
    testReturnsNullWhenMissing();
    testReturnsNullWhenFirstNonBlankIsNotDoc();
    testHandlesLeadingBlankLines();
    testTreatsInlineBlankInsideBlockAsContinuation();
    console.log('check-mods-crate-docs.test: ok (5 tests)');
}

run();
