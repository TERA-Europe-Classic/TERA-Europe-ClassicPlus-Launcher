import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const INDEX_HTML = path.join(REPO_ROOT, 'teralaunch/src/index.html');
const APP_JS = path.join(REPO_ROOT, 'teralaunch/src/app.js');

describe('launcher window dragging', () => {
    it('marks the header as a drag region while keeping controls no-drag', () => {
        const html = fs.readFileSync(INDEX_HTML, 'utf8');
        expect(html).toMatch(/\.header\s*\{[^}]*-webkit-app-region:\s*drag/i);
        expect(html).toMatch(/\.nav-btn\s*\{[^}]*-webkit-app-region:\s*no-drag/i);
        expect(html).toMatch(/\.header-right\s*\{[^}]*-webkit-app-region:\s*no-drag/i);
        expect(html).toMatch(/\.region-select\s*\{[^}]*-webkit-app-region:\s*no-drag/i);
    });

    it('does not short-circuit setupWindowDragging anymore', () => {
        const js = fs.readFileSync(APP_JS, 'utf8');
        expect(js).not.toContain('Dragging is intentionally disabled for now.');
        expect(js).not.toMatch(/setupWindowDragging\(\)\s*\{\s*return;\s*\}/);
    });
});
