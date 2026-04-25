import { describe, it, expect } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const INDEX_HTML = path.join(REPO_ROOT, 'teralaunch/src/index.html');
const APP_JS = path.join(REPO_ROOT, 'teralaunch/src/app.js');

describe('launcher update toast', () => {
    it('uses stable numeric subtitle styling for download progress', () => {
        const html = fs.readFileSync(INDEX_HTML, 'utf8');
        // Toast width: 400px is wide enough to fit "Downloading 18.4 MB / 52.9 MB"
        // (28 chars) without clipping the trailing "MB". Old value 344px was
        // too narrow once the subtitle's 24ch cap was removed; keep this
        // pinned so a future shrink doesn't reintroduce the clip.
        expect(html).toMatch(/\.update-toast\s*\{[^}]*width:\s*400px/i);
        expect(html).toMatch(/\.update-toast\s*\{[^}]*max-width:\s*calc\(100vw\s*-\s*32px\)/i);
        expect(html).toContain('.update-toast-subtitle');
        // Stability still comes from `tabular-nums` (digit-width invariant)
        // plus parent flex sizing — no fixed `width: Nch` cap on the subtitle,
        // and `text-overflow: ellipsis` for the rare case it does overflow.
        expect(html).toMatch(/\.update-toast-subtitle\s*\{[^}]*font-variant-numeric:\s*tabular-nums/i);
        expect(html).toMatch(/\.update-toast-subtitle\s*\{[^}]*overflow:\s*hidden/i);
        expect(html).toMatch(/\.update-toast-subtitle\s*\{[^}]*text-overflow:\s*ellipsis/i);
        expect(html).toMatch(/\.update-toast-text\s*\{[^}]*flex:\s*1/i);
    });

    it('avoids rebuilding icon markup on every progress tick when state is unchanged', () => {
        const js = fs.readFileSync(APP_JS, 'utf8');
        expect(js).toContain('lastUpdateToastState');
        expect(js).toContain('if (lastUpdateToastState !== state)');
    });
});
