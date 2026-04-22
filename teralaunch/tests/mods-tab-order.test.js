import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const MODS_HTML = path.join(REPO_ROOT, 'teralaunch/src/mods.html');

describe('mods tab order (PRD 3.5.6)', () => {
    it('follows tabs -> toolbar -> category chips -> content rows -> tray in DOM order', () => {
        const html = fs.readFileSync(MODS_HTML, 'utf8');

        const navIdx = html.indexOf('class="mods-nav-row"');
        const toolbarIdx = html.indexOf('class="mods-toolbar-row"');
        const filtersIdx = html.indexOf('class="mods-filters-row"');
        const installedRowsIdx = html.indexOf('id="mods-installed-external"');
        const trayIdx = html.indexOf('id="mods-download-tray"');

        expect(navIdx).toBeGreaterThan(0);
        expect(toolbarIdx).toBeGreaterThan(0);
        expect(filtersIdx).toBeGreaterThan(0);
        expect(installedRowsIdx).toBeGreaterThan(0);
        expect(trayIdx).toBeGreaterThan(0);

        expect(navIdx, 'tabs/navigation should come before toolbar controls').toBeLessThan(toolbarIdx);
        expect(toolbarIdx, 'toolbar controls should come before category chips').toBeLessThan(filtersIdx);
        expect(filtersIdx, 'filters should come before installed rows').toBeLessThan(installedRowsIdx);
        expect(installedRowsIdx, 'content rows should come before tray').toBeLessThan(trayIdx);
    });
});
