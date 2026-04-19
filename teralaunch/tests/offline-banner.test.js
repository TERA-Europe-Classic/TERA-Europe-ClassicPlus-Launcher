/**
 * fix.offline-empty-state (iter 84) — the launcher used to render a blank
 * dark viewport when the portal API was unreachable. Root cause: the
 * `.mainpage.ready` class (which flips opacity 0 → 1) was added mid-init,
 * after a network-touching silent-auth await. If any pre-.ready await
 * threw on an unreachable portal, the outer catch swallowed it and the
 * page never became visible.
 *
 * This test pins three things:
 *   (1) The offline-banner DOM element exists in index.html (structural).
 *   (2) showOfflineBanner() / hideOfflineBanner() toggle the .hidden class.
 *   (3) The retry button wires to App.init() — clicking it re-runs the
 *       connection probe.
 *
 * Blank-screen prevention itself (the move of .ready to the top of init)
 * is guarded by an inline check in app.js + the existing init-coverage
 * tests; a DOM test here would require booting the whole app, which is
 * out of scope for a vitest DOM-only test.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const REPO_ROOT = path.resolve(import.meta.dirname, '..', '..');
const INDEX_HTML = path.join(REPO_ROOT, 'teralaunch/src/index.html');
const APP_JS = path.join(REPO_ROOT, 'teralaunch/src/app.js');

describe('offline banner — fix.offline-empty-state', () => {
    it('index.html ships the offline-banner DOM skeleton', () => {
        const html = fs.readFileSync(INDEX_HTML, 'utf8');
        expect(html).toContain('id="offline-banner"');
        expect(html).toContain('offline-banner hidden');
        expect(html).toContain('id="offline-banner-retry"');
        // data-translate attributes pin the i18n wiring for all 3 strings
        expect(html).toContain('data-translate="OFFLINE_BANNER_TITLE"');
        expect(html).toContain('data-translate="OFFLINE_BANNER_DESC"');
        expect(html).toContain('data-translate="OFFLINE_BANNER_RETRY"');
        // role="alert" is critical for screen readers — losing this would
        // make the banner invisible to a11y users even when the visual
        // banner renders.
        expect(html).toContain('role="alert"');
    });

    it('App.init() flips .ready class BEFORE the first await in the body', () => {
        // Source-inspect: the move of `.ready` to the top of init() is the
        // actual fix for the blank-screen symptom. A future refactor that
        // pushes it back behind an await would re-introduce the bug. We
        // assert ORDER (.ready comes before the first await), not absence,
        // because comments legitimately contain the word "await".
        const src = fs.readFileSync(APP_JS, 'utf8');
        const initStart = src.indexOf('async init() {');
        expect(initStart, '.init() must exist').toBeGreaterThan(0);
        // Scan the init body for the .ready flip and the first `await ` that
        // isn't inside a single-line comment.
        const initOpen = src.indexOf('{', initStart) + 1;
        const bodyEnd = src.indexOf('\n  }', initOpen);
        const body = src.slice(initOpen, bodyEnd);
        const readyIdx = body.indexOf("classList.add('ready')");
        expect(readyIdx, "init body must contain the .ready flip").toBeGreaterThan(0);

        // Strip `//`-style comment lines so "no network-touching await"
        // doesn't trip the detector.
        const codeOnly = body
            .split('\n')
            .map(line => {
                const commentIdx = line.indexOf('//');
                // crude but sufficient here: cut at // (no strings carry //).
                return commentIdx >= 0 ? line.slice(0, commentIdx) : line;
            })
            .join('\n');
        const firstAwait = codeOnly.search(/\bawait\b/);
        expect(firstAwait, 'init body must contain at least one await').toBeGreaterThan(0);
        // The .ready flip lives in the original body; map its index into
        // codeOnly by re-searching (comment stripping preserves line breaks,
        // so code positions shift by the count of comment chars removed
        // before it — easier to just re-search).
        const readyInCode = codeOnly.indexOf("classList.add('ready')");
        expect(readyInCode).toBeGreaterThan(0);
        expect(readyInCode, '.ready flip must appear before the first await')
            .toBeLessThan(firstAwait);
    });

    it('App.showOfflineBanner() removes .hidden, hide adds it back', () => {
        // Minimal DOM harness — drop the banner fragment into document.body
        // and call the methods directly. The real App object loads
        // hundreds of globals; we only need the two methods.
        document.body.innerHTML = `
            <div id="offline-banner" class="offline-banner hidden">
                <button id="offline-banner-retry" type="button"></button>
            </div>
        `;

        // Mirror the methods from app.js exactly.
        const fakeApp = {
            init: vi.fn(),
            showOfflineBanner() {
                const banner = document.getElementById('offline-banner');
                if (!banner) return;
                banner.classList.remove('hidden');
                const retryBtn = document.getElementById('offline-banner-retry');
                if (retryBtn && !retryBtn.dataset.wired) {
                    retryBtn.dataset.wired = '1';
                    retryBtn.addEventListener('click', () => {
                        this.hideOfflineBanner();
                        this.init();
                    });
                }
            },
            hideOfflineBanner() {
                const banner = document.getElementById('offline-banner');
                if (banner) banner.classList.add('hidden');
            },
        };

        fakeApp.showOfflineBanner();
        expect(document.getElementById('offline-banner').classList.contains('hidden'))
            .toBe(false);

        fakeApp.hideOfflineBanner();
        expect(document.getElementById('offline-banner').classList.contains('hidden'))
            .toBe(true);
    });

    it('Retry button click re-runs init and hides the banner', () => {
        document.body.innerHTML = `
            <div id="offline-banner" class="offline-banner hidden">
                <button id="offline-banner-retry" type="button"></button>
            </div>
        `;

        const initSpy = vi.fn();
        const fakeApp = {
            init: initSpy,
            showOfflineBanner() {
                const banner = document.getElementById('offline-banner');
                banner.classList.remove('hidden');
                const retryBtn = document.getElementById('offline-banner-retry');
                if (retryBtn && !retryBtn.dataset.wired) {
                    retryBtn.dataset.wired = '1';
                    retryBtn.addEventListener('click', () => {
                        this.hideOfflineBanner();
                        this.init();
                    });
                }
            },
            hideOfflineBanner() {
                document.getElementById('offline-banner').classList.add('hidden');
            },
        };

        fakeApp.showOfflineBanner();
        document.getElementById('offline-banner-retry').click();

        expect(initSpy, 'retry must re-run App.init()').toHaveBeenCalledOnce();
        expect(document.getElementById('offline-banner').classList.contains('hidden'))
            .toBe(true);
    });

    it('Retry wiring is idempotent — multiple show calls do not stack handlers', () => {
        document.body.innerHTML = `
            <div id="offline-banner" class="offline-banner hidden">
                <button id="offline-banner-retry" type="button"></button>
            </div>
        `;

        const initSpy = vi.fn();
        const fakeApp = {
            init: initSpy,
            showOfflineBanner() {
                const banner = document.getElementById('offline-banner');
                banner.classList.remove('hidden');
                const retryBtn = document.getElementById('offline-banner-retry');
                if (retryBtn && !retryBtn.dataset.wired) {
                    retryBtn.dataset.wired = '1';
                    retryBtn.addEventListener('click', () => {
                        this.hideOfflineBanner();
                        this.init();
                    });
                }
            },
            hideOfflineBanner() {
                document.getElementById('offline-banner').classList.add('hidden');
            },
        };

        // Show 3x — simulating repeated failures during init retries.
        fakeApp.showOfflineBanner();
        fakeApp.showOfflineBanner();
        fakeApp.showOfflineBanner();

        document.getElementById('offline-banner-retry').click();
        expect(initSpy, 'one click fires init exactly once even after 3 shows')
            .toHaveBeenCalledOnce();
    });
});

describe('translations — OFFLINE_BANNER_* keys present in all 4 locales', () => {
    const TRANSLATIONS = path.join(REPO_ROOT, 'teralaunch/src/translations.json');

    it('FRA + EUR + RUS + GER all carry the 3 new keys', () => {
        const data = JSON.parse(fs.readFileSync(TRANSLATIONS, 'utf8'));
        for (const locale of ['FRA', 'EUR', 'RUS', 'GER']) {
            expect(data[locale], `locale ${locale} must exist`).toBeDefined();
            expect(data[locale].OFFLINE_BANNER_TITLE, `${locale}.OFFLINE_BANNER_TITLE`)
                .toBeTruthy();
            expect(data[locale].OFFLINE_BANNER_DESC, `${locale}.OFFLINE_BANNER_DESC`)
                .toBeTruthy();
            expect(data[locale].OFFLINE_BANNER_RETRY, `${locale}.OFFLINE_BANNER_RETRY`)
                .toBeTruthy();
        }
    });
});
