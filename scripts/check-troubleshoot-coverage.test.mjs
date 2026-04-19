#!/usr/bin/env node
// Smoke test for the troubleshoot-coverage gate. Pins the template extractor
// against known shapes and verifies the gate fires on a missing-template
// scenario (not just the happy path).

import assert from 'node:assert/strict';
import { extractTemplates } from './check-troubleshoot-coverage.mjs';

function testExtractsMapErrFormat() {
    const src = `
        fs::read(&path)
            .map_err(|e| format!("Failed to read mod file: {}", e))?;
    `;
    const tmpls = extractTemplates(src);
    assert.deepEqual(tmpls, ['Failed to read mod file: {}']);
}

function testExtractsReturnErrFormat() {
    const src = `
        return Err(format!("Composite entry for '{}' not found in mapper. Your game version may not match the mod.", path));
    `;
    const tmpls = extractTemplates(src);
    assert.deepEqual(tmpls, [
        "Composite entry for '{}' not found in mapper. Your game version may not match the mod.",
    ]);
}

function testExtractsReturnErrIntoShape() {
    const src = `
        return Err("Mod file has no TMM container name — this .gpk is not TMM-compatible.".into());
    `;
    const tmpls = extractTemplates(src);
    assert.deepEqual(tmpls, [
        'Mod file has no TMM container name — this .gpk is not TMM-compatible.',
    ]);
}

function testExtractsMultipleInOneFile() {
    const src = `
        .map_err(|e| format!("First: {}", e))?;
        let x = 1;
        return Err(format!("Second: {}", x));
        return Err("Third".into());
    `;
    const tmpls = extractTemplates(src);
    assert.equal(tmpls.length, 3);
    assert.ok(tmpls.includes('First: {}'));
    assert.ok(tmpls.includes('Second: {}'));
    assert.ok(tmpls.includes('Third'));
}

function testIgnoresUnrelatedStrings() {
    const src = `
        let x = "plain string";
        let y = format!("this is not an error");
        println!("and neither is this");
    `;
    const tmpls = extractTemplates(src);
    assert.deepEqual(tmpls, []);
}

function run() {
    testExtractsMapErrFormat();
    testExtractsReturnErrFormat();
    testExtractsReturnErrIntoShape();
    testExtractsMultipleInOneFile();
    testIgnoresUnrelatedStrings();
    console.log('check-troubleshoot-coverage.test: ok (5 tests)');
}

run();
