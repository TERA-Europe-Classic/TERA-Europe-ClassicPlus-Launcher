import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const INDEX_HTML = path.join(REPO_ROOT, 'teralaunch/src/index.html');
const APP_JS = path.join(REPO_ROOT, 'teralaunch/src/app.js');

describe('launcher update toast', () => {
    it('uses stable numeric subtitle styling for download progress', () => {
        const html = fs.readFileSync(INDEX_HTML, 'utf8');
        expect(html).toContain('.update-toast-subtitle');
        expect(html).toMatch(/\.update-toast-subtitle\s*\{[^}]*font-variant-numeric:\s*tabular-nums/i);
        expect(html).toMatch(/\.update-toast-subtitle\s*\{[^}]*min-width:\s*\d+ch/i);
        expect(html).toMatch(/\.update-toast-text\s*\{[^}]*flex:\s*1/i);
    });

    it('avoids rebuilding icon markup on every progress tick when state is unchanged', () => {
        const js = fs.readFileSync(APP_JS, 'utf8');
        expect(js).toContain('lastUpdateToastState');
        expect(js).toContain('if (lastUpdateToastState !== state)');
    });
});
