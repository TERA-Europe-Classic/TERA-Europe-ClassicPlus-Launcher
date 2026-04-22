import { describe, it, expect, beforeEach, vi } from 'vitest';

describe('mods download tray DOM perf (PRD 3.4.4)', () => {
    beforeEach(() => {
        vi.resetModules();
        global.CSS = { escape: (value) => String(value) };
        document.body.innerHTML = `
            <div id="mods-download-tray">
                <div id="mods-download-tray-count">1</div>
                <div id="mods-download-tray-items">
                    <div class="mods-download-tray-item" data-dl-id="classicplus.tcc">
                        <div class="mods-download-tray-item-header">
                            <span class="mods-download-tray-name">TCC</span>
                            <span class="mods-download-tray-progress">10%</span>
                        </div>
                        <div class="mods-download-tray-item-meta">
                            <span class="mods-download-tray-bytes">1.0 MB / 10.0 MB</span>
                        </div>
                        <div class="mods-download-tray-bar">
                            <div class="mods-download-tray-bar-fill" style="width:10%"></div>
                        </div>
                    </div>
                </div>
            </div>
        `;
        window.__TAURI__ = {
            core: { invoke: vi.fn() },
            tauri: { invoke: vi.fn() },
            event: { listen: vi.fn(async () => () => {}) },
        };
    });

    it('tray_surgical_update', async () => {
        const { ModsView } = await import('../src/mods.js');
        ModsView.$tray = document.getElementById('mods-download-tray');
        ModsView.$trayItems = document.getElementById('mods-download-tray-items');
        ModsView.$trayCount = document.getElementById('mods-download-tray-count');
        ModsView.state.downloads = new Map([
            ['classicplus.tcc', { progress: 10, state: 'downloading', received_bytes: 1_000_000, total_bytes: 10_000_000 }],
        ]);

        const renderSpy = vi.spyOn(ModsView, 'renderDownloadTray');
        const itemBefore = ModsView.$trayItems.querySelector('[data-dl-id="classicplus.tcc"]');

        ModsView.updateDownloadTrayItem('classicplus.tcc', 43, 4_500_000, 10_000_000);

        const itemAfter = ModsView.$trayItems.querySelector('[data-dl-id="classicplus.tcc"]');
        expect(itemAfter).toBe(itemBefore);
        expect(renderSpy).not.toHaveBeenCalled();
        expect(itemAfter.querySelector('.mods-download-tray-progress')?.textContent).toBe('43%');
        expect(itemAfter.querySelector('.mods-download-tray-bytes')?.textContent).toBe('4.3 MB / 9.5 MB');
        expect(itemAfter.querySelector('.mods-download-tray-bar-fill')?.style.width).toBe('43%');
    });
});
