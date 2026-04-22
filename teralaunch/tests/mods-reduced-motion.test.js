import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const MODS_CSS = path.join(REPO_ROOT, 'teralaunch/src/mods.css');

describe('mods reduced motion (PRD 3.5.5)', () => {
    it('disables modal, status pulse, toggle, and progress motion when reduced motion is requested', () => {
        const css = fs.readFileSync(MODS_CSS, 'utf8');
        expect(css).toContain('@media (prefers-reduced-motion: reduce)');
        expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*\.mods-modal-shell/);
        expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*\.mods-detail/);
        expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*\.mods-download-tray-bar-fill/);
        expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*\.mods-row-toggle-thumb/);
        expect(css).toMatch(/@media \(prefers-reduced-motion: reduce\)[\s\S]*\.mods-row-running-dot/);
        expect(css).toMatch(/animation:\s*none/i);
        expect(css).toMatch(/transition:\s*none/i);
    });
});
